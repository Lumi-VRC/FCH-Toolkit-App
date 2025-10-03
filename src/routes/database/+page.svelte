<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  type Note = { ts: string; text: string };
  let entries = $state([] as Array<{ userId: string; notes: Note[]; username?: string; watch?: boolean; soundPath?: string }>);
  let open: Record<string, boolean> = $state({});
  let editing: Record<string, boolean> = $state({});
  let confirming: Record<string, boolean> = $state({});
  let draft: Record<string, string> = $state({});
  let soundDraft: Record<string, string> = $state({});
  let query = $state('');
  let showWatchOnly = $state(false);
  let adding = $state(false);
  let newUserId = $state('');
  let newUsername = $state('');
  let newNote = $state('');
  let newWatch = $state(true);
  let newSound = $state('');

  async function prefillUsername() {
    const id = (newUserId || '').trim();
    if (!id) { newUsername = ''; return; }
    try {
      const res: any = await invoke('get_latest_username_for_user', { userId: id });
      newUsername = res?.username || '';
    } catch { newUsername = ''; }
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
      newUserId = ''; newUsername = ''; newNote = ''; newWatch = true; newSound = ''; adding = false;
    } catch {}
  }

  async function load() {
    try {
      const res: any = await invoke('get_all_notes');
      const map = res?.notes || {};
      const usernames = res?.usernames || {};
      const watchlist = res?.watchlist || {};
      const sounds = res?.sounds || {};
      const list: Array<{ userId: string; notes: Note[]; username?: string; watch?: boolean; soundPath?: string }> = Object.entries(map)
        .map(([userId, notes]: any) => ({
          userId,
          notes: notes as Note[],
          username: usernames[userId],
          watch: !!watchlist[userId],
          soundPath: typeof sounds[userId] === 'string' ? sounds[userId] : undefined
        }))
        .sort((a,b)=>a.userId.localeCompare(b.userId));
      entries = list;
      for (const e of list) {
        open[e.userId] = true;
        draft[e.userId] = (e.notes?.[e.notes.length-1]?.text) || '';
        soundDraft[e.userId] = e.soundPath || '';
      }
      open = { ...open }; draft = { ...draft }; soundDraft = { ...soundDraft };
    } catch {}
  }

  async function save(userId: string) {
    try { await invoke('add_note', { userId, text: draft[userId] || '' }); editing[userId] = false; } catch {}
    editing = { ...editing };
  }

  async function saveSoundOverride(userId: string) {
    const path = (soundDraft[userId] || '').trim();
    try {
      await invoke('set_user_sound', { userId, path: path.length > 0 ? path : null });
      entries = entries.map((ent) => ent.userId === userId ? { ...ent, soundPath: path || undefined } : ent);
    } catch {}
  }
  async function remove(userId: string) {
    const row = document.getElementById('row-' + userId);
    if (row) {
      row.classList.add('glow');
      await new Promise(r => setTimeout(r, 600));
    }
    try { await invoke('delete_user', { userId }); entries = entries.filter(e => e.userId !== userId); } catch {}
  }

  function filteredEntries() {
    const q = (query || '').toLowerCase();
    return entries.filter(e => {
      const noteText = (e.notes?.[e.notes.length-1]?.text || '').toLowerCase();
      const match = !q || e.userId.toLowerCase().includes(q) || (e.username||'').toLowerCase().includes(q) || noteText.includes(q);
      return match && (!showWatchOnly || !!e.watch);
    });
  }

  onMount(load);
</script>

<div class="panel">
  <div class="tools">
    <input placeholder="Search by user, id, note..." bind:value={query} />
    <button class="icon" title="Add" onclick={() => { adding = !adding; }} aria-label="Add user">+</button>
    <label class="chk"><input type="checkbox" bind:checked={showWatchOnly} /> Watchlisted only</label>
  </div>
  {#if adding}
    <div class="add-panel">
      <div class="grid">
        <label for="new-user-id">User ID</label>
        <div class="row">
          <input id="new-user-id" placeholder="usr_..." bind:value={newUserId} onchange={prefillUsername} onblur={prefillUsername} />
          <button class="mini" onclick={prefillUsername}>Lookup</button>
        </div>
        <label for="new-username">Username</label>
        <input id="new-username" placeholder="Will backfill if known" bind:value={newUsername} />
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
        <div class="avatar" aria-hidden="true">{(e.username || e.userId).slice(0,1) || '?'}</div>
        <div class="col">
          <div class="name">{e.username || 'Unknown'} <span class="muted">({e.userId})</span></div>
          <div class="sub">Last updated: {e.notes?.[e.notes.length-1]?.ts || '—'}</div>
          {#if open[e.userId]}
            <div class="note-editor">
              <textarea bind:value={draft[e.userId]} placeholder="Write a note..." readonly={!editing[e.userId]} class:readonly={!editing[e.userId]}></textarea>
              <div class="note-actions">
                {#if editing[e.userId]}
                  <button onclick={() => { draft[e.userId] = e.notes?.[e.notes.length-1]?.text || ''; editing[e.userId]=false; draft={...draft}; editing={...editing}; }}>Cancel</button>
                  <button onclick={() => save(e.userId)}>Save</button>
                  {#if confirming[e.userId]}
                    <button class="danger" onclick={() => remove(e.userId)}>Confirm Delete</button>
                  {:else}
                    <button class="danger" onclick={() => { confirming[e.userId] = true; confirming = { ...confirming }; }}>Delete</button>
                  {/if}
                {:else}
                  <button onclick={() => { editing[e.userId]=true; editing={...editing}; }}>Edit</button>
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
                <button onclick={async () => { try { const res: any = await invoke('browse_sound'); const p = res?.path || null; if (p) { soundDraft[e.userId] = p; soundDraft = { ...soundDraft }; await saveSoundOverride(e.userId); } } catch {} }}>Browse</button>
                {#if soundDraft[e.userId]?.trim()?.length}
                  <button onclick={() => { soundDraft[e.userId] = ''; soundDraft = { ...soundDraft }; saveSoundOverride(e.userId); }}>Clear</button>
                {/if}
              </div>
            </div>
            </div>
          {/if}
        </div>
        <div class="actions">
          <button class="toggle" class:active={e.watch} onclick={async () => { try { await invoke('set_watch', { userId: e.userId, watch: !e.watch }); e.watch = !e.watch; entries = [...entries]; } catch {} }}>{e.watch ? 'Unwatch' : 'Watch'}</button>
        </div>
      </div>
    {/each}
  {/if}
</div>

<style>
  .panel { display: grid; gap: 8px; }
  .tools { display: flex; gap: 8px; }
  .tools input { flex: 1; border: 1px solid var(--border); background: var(--bg); color: var(--fg); border-radius: 8px; padding: 6px 10px; }
  .tools .icon { width: 34px; height: 34px; border-radius: 8px; border: 1px solid var(--border); background: var(--bg); color: var(--fg); cursor: pointer; }
  .tools .chk { display: inline-flex; align-items: center; gap: 6px; color: var(--fg-muted); font-size: 12px; }
  .add-panel { border: 1px solid var(--border); border-radius: 10px; padding: 12px; background: var(--bg-elev); display: grid; gap: 10px; }
  .add-panel .grid { display: grid; gap: 6px; }
  .add-panel label { color: var(--fg-muted); font-size: 12px; }
  .add-panel label.inline { display: inline-flex; align-items: center; gap: 8px; }
  .add-panel input, .add-panel textarea { border: 1px solid var(--border); border-radius: 8px; background: var(--bg); color: var(--fg); padding: 6px 10px; }
  .add-panel textarea { min-height: 70px; resize: vertical; }
  .add-panel .row { display: grid; grid-template-columns: 1fr auto; gap: 8px; }
  .add-panel .mini { border: 1px solid var(--border); background: var(--bg); color: var(--fg); border-radius: 8px; padding: 4px 10px; cursor: pointer; }
  .add-panel .actions { display: flex; justify-content: flex-end; gap: 8px; }
  .row { display: grid; grid-template-columns: 36px 1fr auto; gap: 12px; align-items: start; padding: 10px 12px; border: 1px solid var(--border); border-radius: 10px; background: var(--bg-elev); }
  :global(.row.glow) { animation: glowfade 600ms ease-out forwards; }
  @keyframes glowfade { 0% { box-shadow: 0 0 0 0 rgba(255,182,193,0.8); background: rgba(255,182,193,0.15); } 100% { box-shadow: 0 0 0 0 rgba(255,182,193,0); background: var(--bg-elev); } }
  .avatar { width: 36px; height: 36px; border-radius: 8px; background: var(--bg); color: var(--fg-muted); display: inline-flex; align-items: center; justify-content: center; font-weight: 600; }
  .name { color: var(--fg); font-weight: 600; }
  .name .muted { color: var(--fg-muted); font-weight: 400; font-size: 12px; }
  .sub { color: var(--fg-muted); font-size: 12px; }
  .note-editor { margin-top: 8px; display: grid; gap: 8px; }
  .note-editor textarea { width: 100%; min-height: 80px; border: 1px solid var(--border); border-radius: 8px; background: var(--bg); color: var(--fg); padding: 8px; resize: vertical; }
  .note-editor textarea.readonly { background: #1c1c1c; color: #bdbdbd; }
  .note-actions { display: inline-flex; gap: 8px; }
  .actions .toggle, .note-actions button { border: 1px solid var(--border); background: var(--bg); color: var(--fg); border-radius: 8px; padding: 4px 10px; cursor: pointer; }
  .actions .toggle.active { background: rgba(255,182,193,0.25); border-color: #ffb6c1; }
  .note-actions .danger { border-color: #8b2b2b; color: #ffb6b6; }
  .empty { padding: 16px; color: var(--fg-muted); }
</style>


