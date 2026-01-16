<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';

  type Note = { ts: string; text: string };
  let entries = $state([]);
  let open = $state({});
  let editing = $state({});
  let confirming = $state({});
  let draft = $state({});
  let soundDraft = $state({});
  let query = $state('');
  let showWatchOnly = $state(false);
  let adding = $state(false);
  let newUserId = $state('');
  let newUsername = $state('');
  let newNote = $state('');
  let newWatch = $state(true);
  let newSound = $state('');
  let containerElement;
  let wasVisible = false;
  let observer = null;

  async function load() {
    const startTime = performance.now();
    console.log('[PERF] database load() START');
    try {
      const dbStartTime = performance.now();
      const res = await invoke('get_all_notes');
      const dbDuration = performance.now() - dbStartTime;
      console.log(`[PERF] database get_all_notes DB call: ${dbDuration.toFixed(2)}ms`);
      
      const processStartTime = performance.now();
      const map = res?.notes || {};
      const usernames = res?.usernames || {};
      const watchlist = res?.watchlist || {};
      const sounds = res?.sounds || {};
      const list = Object.entries(map)
        .map(([userId, notes]) => ({
          userId,
          notes: notes || [],
          username: usernames[userId],
          watch: !!watchlist[userId],
          soundPath: typeof sounds[userId] === 'string' ? sounds[userId] : undefined
        }))
        .sort((a, b) => a.userId.localeCompare(b.userId));
      entries = list;
      for (const e of list) {
        open[e.userId] = true;
        draft[e.userId] = (e.notes?.[e.notes.length - 1]?.text) || '';
        soundDraft[e.userId] = e.soundPath || '';
      }
      open = { ...open };
      draft = { ...draft };
      soundDraft = { ...soundDraft };
      const processDuration = performance.now() - processStartTime;
      console.log(`[PERF] database load() data processing: ${processDuration.toFixed(2)}ms (${list.length} entries)`);
      
      const totalDuration = performance.now() - startTime;
      console.log(`[PERF] database load() END: ${totalDuration.toFixed(2)}ms`);
    } catch (err) {
      console.error('Failed to load notes:', err);
    }
  }

  async function addEntry() {
    const id = (newUserId || '').trim();
    if (!id) return;
    try {
      if (newNote != null) {
        await invoke('add_note', { userId: id, text: newNote || '' });
      }
      await invoke('set_watch', { userId: id, watch: !!newWatch });
      if (newUsername != null) {
        await invoke('set_username', { userId: id, username: newUsername || '' });
      }
      await invoke('set_user_sound', { userId: id, path: newSound.trim() ? newSound : null });
      // Refresh list
      await load();
      // Reset UI
      newUserId = '';
      newUsername = '';
      newNote = '';
      newWatch = true;
      newSound = '';
      adding = false;
    } catch (err) {
      console.error('Failed to add entry:', err);
    }
  }

  async function save(userId) {
    try {
      await invoke('add_note', { userId, text: draft[userId] || '' });
      editing[userId] = false;
      editing = { ...editing };
      // Reload to get updated timestamp
      await load();
    } catch (err) {
      console.error('Failed to save note:', err);
    }
  }

  async function saveSoundOverride(userId) {
    const path = (soundDraft[userId] || '').trim();
    try {
      await invoke('set_user_sound', { userId, path: path.length > 0 ? path : null });
      // Update local state immediately for responsive UI
      entries = entries.map((ent) => ent.userId === userId ? { ...ent, soundPath: path || undefined } : ent);
    } catch (err) {
      console.error('Failed to save sound:', err);
    }
  }

  async function remove(userId) {
    const row = document.getElementById('row-' + userId);
    if (row) {
      row.classList.add('glow');
      await new Promise(r => setTimeout(r, 600));
    }
    try {
      await invoke('delete_user', { userId });
      entries = entries.filter(e => e.userId !== userId);
    } catch (err) {
      console.error('Failed to delete user:', err);
    }
  }

  function filteredEntries() {
    const q = (query || '').toLowerCase();
    return entries.filter(e => {
      const noteText = (e.notes?.[e.notes.length - 1]?.text || '').toLowerCase();
      const match = !q || e.userId.toLowerCase().includes(q) || (e.username || '').toLowerCase().includes(q) || noteText.includes(q);
      return match && (!showWatchOnly || !!e.watch);
    });
  }

  function firstChar(s) {
    if (!s) return '?';
    return s.slice(0, 1);
  }

  // Check if the tab is currently visible
  function isTabVisible() {
    if (!containerElement) return false;
    const parent = containerElement.closest('.tab');
    if (!parent) return false;
    // Check if aria-hidden is not set (meaning tab is active)
    return parent.getAttribute('aria-hidden') === null;
  }

  // Check visibility and refresh if needed
  function checkVisibility() {
    const isVisible = isTabVisible();
    // If tab just became visible (wasn't visible before, but is now), refresh
    if (isVisible && !wasVisible) {
      console.log('[PERF] database tab became visible, triggering load()');
      load();
    }
    wasVisible = isVisible;
  }

  onMount(() => {
    const mountStartTime = performance.now();
    console.log('[PERF] database onMount START');
    // Initial load
    load();
    
    // Set up MutationObserver to watch for aria-hidden changes on parent tab
    if (containerElement) {
      const parent = containerElement.closest('.tab');
      if (parent) {
        wasVisible = isTabVisible();
        
        // Watch for changes to aria-hidden attribute
        observer = new MutationObserver(() => {
          checkVisibility();
        });
        
        observer.observe(parent, {
          attributes: true,
          attributeFilter: ['aria-hidden']
        });
      }
    }
    const mountDuration = performance.now() - mountStartTime;
    console.log(`[PERF] database onMount END: ${mountDuration.toFixed(2)}ms`);
  });

  onDestroy(() => {
    if (observer) {
      observer.disconnect();
    }
  });
</script>

<div class="panel" bind:this={containerElement}>
  <div class="warning-panel">
    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
      <path d="M12 9v4"></path>
      <path d="M12 17h.01"></path>
      <path d="M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0z"></path>
    </svg>
    <span>Note: These entries are completely separate from the group watchlist. These are local entries, visible only to you.</span>
  </div>
  <div class="tools">
    <input placeholder="Search by user, id, note..." bind:value={query} />
    <button class="icon" title="Add" onclick={() => { adding = !adding; }} aria-label="Add user">+</button>
    <label class="chk"><input type="checkbox" bind:checked={showWatchOnly} /> Watchlisted only</label>
  </div>
  {#if adding}
    <div class="add-panel">
      <div class="grid">
        <label for="new-user-id">User ID</label>
        <input id="new-user-id" placeholder="usr_..." bind:value={newUserId} />
        <label for="new-username">Username</label>
        <input id="new-username" placeholder="Optional username" bind:value={newUsername} />
        <label for="new-note">Note</label>
        <textarea id="new-note" placeholder="Optional note..." bind:value={newNote}></textarea>
        <label class="inline" for="new-watch"><input id="new-watch" type="checkbox" bind:checked={newWatch} /> Watchlist</label>
        <label for="new-sound">Custom Sound Override</label>
        <input id="new-sound" placeholder="C:\\path\\to\\sound.mp3" bind:value={newSound} />
      </div>
      <div class="actions">
        <button onclick={() => { adding = false; }}>Cancel</button>
        <button onclick={addEntry}>Add</button>
      </div>
    </div>
  {/if}
  {#if filteredEntries().length === 0}
    <div class="empty">No notes yet.</div>
  {:else}
    {#each filteredEntries() as e (e.userId)}
      <div class="row" id={`row-${e.userId}`} role="listitem">
        <div class="avatar" aria-hidden="true">{firstChar((e.username && e.username !== 'Not Yet Recorded') ? e.username : e.userId)}</div>
        <div class="col">
          <div class="name">{(e.username && e.username !== 'Not Yet Recorded') ? e.username : e.userId} <span class="muted">({e.userId})</span></div>
          <div class="sub">Last updated: {e.notes?.[e.notes.length - 1]?.ts || '—'}</div>
          {#if open[e.userId]}
            <div class="note-editor">
              <textarea bind:value={draft[e.userId]} placeholder="Write a note..." readonly={!editing[e.userId]} class:readonly={!editing[e.userId]}></textarea>
              <div class="note-actions">
                {#if editing[e.userId]}
                  <button onclick={() => { draft[e.userId] = e.notes?.[e.notes.length - 1]?.text || ''; editing[e.userId] = false; draft = { ...draft }; editing = { ...editing }; }}>Cancel</button>
                  <button onclick={() => save(e.userId)}>Save</button>
                  {#if confirming[e.userId]}
                    <button class="danger" onclick={() => remove(e.userId)}>Confirm Delete</button>
                  {:else}
                    <button class="danger" onclick={() => { confirming[e.userId] = true; confirming = { ...confirming }; }}>Delete</button>
                  {/if}
                {:else}
                  <button onclick={() => { editing[e.userId] = true; editing = { ...editing }; }}>Edit</button>
                  {#if confirming[e.userId]}
                    <button class="danger" onclick={() => remove(e.userId)}>Confirm Delete</button>
                  {:else}
                    <button class="danger" onclick={() => { confirming[e.userId] = true; confirming = { ...confirming }; }}>Delete</button>
                  {/if}
                {/if}
              </div>
              <div class="sound-editor">
                <label for={`sound-${e.userId}`}>Custom Sound Path</label>
                <div class="sound-row">
                  <input id={`sound-${e.userId}`} placeholder="C:\\path\\override.mp3" bind:value={soundDraft[e.userId]} />
                  <button onclick={() => saveSoundOverride(e.userId)}>Save Sound</button>
                  <button onclick={async () => { try { const res = await invoke('browse_sound'); const p = res?.path || null; if (p) { soundDraft[e.userId] = p; soundDraft = { ...soundDraft }; await saveSoundOverride(e.userId); } } catch (err) { console.error('Failed to browse sound:', err); } }}>Browse</button>
                  {#if soundDraft[e.userId]?.trim()?.length}
                    <button onclick={() => { soundDraft[e.userId] = ''; soundDraft = { ...soundDraft }; saveSoundOverride(e.userId); }}>Clear</button>
                  {/if}
                </div>
              </div>
            </div>
          {/if}
        </div>
        <div class="actions">
          <button class="toggle" class:active={e.watch} onclick={async () => { 
            try { 
              await invoke('set_watch', { userId: e.userId, watch: !e.watch }); 
              // Update local state
              const entry = entries.find(ent => ent.userId === e.userId);
              if (entry) {
                entry.watch = !e.watch;
                entries = [...entries];
              }
            } catch (err) {
              console.error('Failed to set watch:', err);
            }
          }}>{e.watch ? 'Unwatch' : 'Watch'}</button>
        </div>
      </div>
    {/each}
  {/if}
</div>

<style>
  .panel {
    display: grid;
    gap: 8px;
  }

  .warning-panel {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 12px 16px;
    background: rgba(255, 193, 7, 0.1);
    border: 1px solid rgba(255, 193, 7, 0.3);
    border-radius: 8px;
    color: var(--fg);
    font-size: 13px;
    line-height: 1.5;
  }

  .warning-panel svg {
    flex-shrink: 0;
    margin-top: 2px;
    color: #ffc107;
    stroke: #ffc107;
  }

  .warning-panel span {
    flex: 1;
  }

  .tools {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .tools input {
    flex: 1;
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--fg);
    border-radius: 8px;
    padding: 6px 10px;
    font-size: 13px;
  }

  .tools .icon {
    width: 34px;
    height: 34px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--fg);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 20px;
    font-weight: 600;
  }

  .tools .icon:hover {
    background: var(--bg-hover);
    border-color: var(--accent);
  }

  .tools .chk {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--fg-muted);
    font-size: 12px;
    white-space: nowrap;
  }

  .add-panel {
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 12px;
    background: var(--bg-elev);
    display: grid;
    gap: 10px;
  }

  .add-panel .grid {
    display: grid;
    gap: 6px;
  }

  .add-panel label {
    color: var(--fg-muted);
    font-size: 12px;
  }

  .add-panel label.inline {
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }

  .add-panel input,
  .add-panel textarea {
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg);
    color: var(--fg);
    padding: 6px 10px;
    font-size: 13px;
  }

  .add-panel textarea {
    min-height: 70px;
    resize: vertical;
    font-family: inherit;
  }

  .add-panel .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  .add-panel .actions button {
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--fg);
    border-radius: 8px;
    padding: 4px 10px;
    cursor: pointer;
    font-size: 13px;
  }

  .add-panel .actions button:hover {
    background: var(--bg-hover);
    border-color: var(--accent);
  }

  .row {
    display: grid;
    grid-template-columns: 36px 1fr auto;
    gap: 12px;
    align-items: start;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--bg-elev);
  }

  :global(.row.glow) {
    animation: glowfade 600ms ease-out forwards;
  }

  @keyframes glowfade {
    0% {
      box-shadow: 0 0 0 0 rgba(255, 182, 193, 0.8);
      background: rgba(255, 182, 193, 0.15);
    }
    100% {
      box-shadow: 0 0 0 0 rgba(255, 182, 193, 0);
      background: var(--bg-elev);
    }
  }

  .avatar {
    width: 36px;
    height: 36px;
    border-radius: 8px;
    background: var(--bg);
    color: var(--fg-muted);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-weight: 600;
    font-size: 16px;
  }

  .name {
    color: var(--fg);
    font-weight: 600;
    font-size: 14px;
  }

  .name .muted {
    color: var(--fg-muted);
    font-weight: 400;
    font-size: 12px;
  }

  .sub {
    color: var(--fg-muted);
    font-size: 12px;
    margin-top: 2px;
  }

  .note-editor {
    margin-top: 8px;
    display: grid;
    gap: 8px;
  }

  .note-editor textarea {
    width: 100%;
    min-height: 80px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg);
    color: var(--fg);
    padding: 8px;
    resize: vertical;
    font-family: inherit;
    font-size: 13px;
  }

  .note-editor textarea.readonly {
    background: rgba(28, 28, 28, 0.5);
    color: var(--fg-muted);
  }

  .note-actions {
    display: inline-flex;
    gap: 8px;
  }

  .note-actions button {
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--fg);
    border-radius: 8px;
    padding: 4px 10px;
    cursor: pointer;
    font-size: 13px;
  }

  .note-actions button:hover {
    background: var(--bg-hover);
    border-color: var(--accent);
  }

  .note-actions .danger {
    border-color: rgba(139, 43, 43, 0.6);
    color: #ffb6b6;
  }

  .note-actions .danger:hover {
    background: rgba(139, 43, 43, 0.2);
  }

  .sound-editor {
    margin-top: 8px;
    display: grid;
    gap: 6px;
  }

  .sound-editor label {
    color: var(--fg-muted);
    font-size: 12px;
  }

  .sound-row {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .sound-row input {
    flex: 1;
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--fg);
    border-radius: 8px;
    padding: 6px 10px;
    font-size: 13px;
  }

  .sound-row button {
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--fg);
    border-radius: 8px;
    padding: 4px 10px;
    cursor: pointer;
    font-size: 13px;
  }

  .sound-row button:hover {
    background: var(--bg-hover);
    border-color: var(--accent);
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .actions .toggle {
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--fg);
    border-radius: 8px;
    padding: 4px 10px;
    cursor: pointer;
    font-size: 13px;
  }

  .actions .toggle:hover {
    background: var(--bg-hover);
    border-color: var(--accent);
  }

  .actions .toggle.active {
    background: rgba(255, 182, 193, 0.25);
    border-color: #ffb6c1;
  }

  .empty {
    padding: 16px;
    color: var(--fg-muted);
    text-align: center;
  }
</style>
