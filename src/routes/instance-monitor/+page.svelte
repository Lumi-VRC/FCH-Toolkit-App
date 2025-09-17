<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';

  type UserJoinLog = { id?: number; type: 'user'; userId: string; username?: string; joinedAt?: string; leftAt?: string };
  type SystemLog = { type: 'system'; ts?: string; message: string; worldId?: string; instanceId?: string; region?: string };
  type LogEntry = SystemLog | UserJoinLog;

  let activeUsers = $state<UserJoinLog[]>([]);
  let pagedLogs = $state<LogEntry[]>([]);
  let logPage = $state(0);
  const LOGS_PER_PAGE = 100;
  let logsLoadedOnce = $state(false);
  let confirmingDelete = $state(false);

  let tab = $state<'monitor' | 'logs'>('monitor');
  let logsQuery = $state('');
  let showNoteFor: string | null = $state(null);
  let noteText = $state('');
  let noted = $state(new Map<string, boolean>());
  const loadingNotes = new Set<string>();
  let watch = $state(new Map<string, boolean>());

  async function handleDeleteClick() {
    if (!confirmingDelete) {
      confirmingDelete = true;
      return;
    }

    try {
      await invoke('purge_join_log_table');
      pagedLogs = [];
      activeUsers = [];
      logPage = 0;
      logsLoadedOnce = false; // allow re-fetch on next tab click
      console.debug('Successfully purged join log table');
    } catch(e) {
      console.error("Failed to purge join log table", e);
    } finally {
      confirmingDelete = false;
    }
  }

  function showJoinLogs() {
    tab = 'logs';
    if (!logsLoadedOnce) {
      fetchPage(0);
      logsLoadedOnce = true;
    }
  }

  function mapRowToEntry(l: any): LogEntry {
    if (l.isSystem || l.isSystem === true) {
      return { type: 'system', ts: l.joinedAt || l.ts, message: l.message || 'System', worldId: l.worldId, instanceId: l.instanceId, region: l.region } as SystemLog;
    }
    return { type: 'user', id: l.id, userId: l.userId, username: l.username, joinedAt: l.joinedAt, leftAt: l.leftAt } as UserJoinLog;
  }

  async function fetchPage(page: number) {
    try {
      const offset = page * LOGS_PER_PAGE;
      const newLogs: any[] = await invoke('get_join_logs_page', { offset, limit: LOGS_PER_PAGE });
      pagedLogs = newLogs.map(mapRowToEntry);
      logPage = page;
      console.debug(`Loaded page ${page}`, { count: pagedLogs.length });
    } catch(e) {
      console.error(`Failed to get log page ${page}`, e);
    }
  }

  function ensureNoteLoaded(userId: string) {
    if (!userId || noted.has(userId) || loadingNotes.has(userId)) return;
    loadingNotes.add(userId);
    (async () => {
      try {
        const res: any = await invoke('get_note', { userId });
        const has = !!(res && res.text && String(res.text).length > 0);
        noted.set(userId, has);
        noted = new Map(noted);
      } catch {}
      loadingNotes.delete(userId);
    })();
  }

  function ensureWatchLoaded(userId: string) {
    if (!userId || watch.has(userId)) return;
    (async () => {
      try { const res: any = await invoke('get_watch', { userId }); watch.set(userId, !!(res && res.watch)); watch = new Map(watch); } catch {}
    })();
  }

  function s(n: number) { return n === 1 ? '' : 's'; }
  function fmt(ts?: string): string {
    if (!ts) return '—';
    const m = ts.match(/^(\d{4})\.(\d{2})\.(\d{2})\s+(\d{2}):(\d{2})/);
    if (!m) return ts;
    const mm = m[2];
    const dd = m[3];
    let hh = parseInt(m[4] || '0', 10);
    const min = m[5];
    const ampm = hh >= 12 ? 'pm' : 'am';
    hh = hh % 12; if (hh === 0) hh = 12;
    return `${mm}/${dd} ${hh}:${min}${ampm}`;
  }

  // Safely get first grapheme for avatar initial (handles emojis/combining marks)
  function firstGrapheme(s?: string): string {
    if (!s) return '?';
    try {
      // @ts-ignore - Intl.Segmenter may not be in lib DOM types
      const seg = new Intl.Segmenter(undefined, { granularity: 'grapheme' });
      // @ts-ignore
      const iter = seg.segment(s)[Symbol.iterator]();
      const next = iter.next();
      return (next && next.value && next.value.segment) ? next.value.segment : s.slice(0, 1);
    } catch {
      return s.slice(0, 1);
    }
  }

  function filteredLogs(): LogEntry[] {
    const all = pagedLogs;
    const q = (logsQuery || '').trim().toLowerCase();
    if (!q) return all;
    return all.filter(l => {
      if ((l as any).type === 'system') {
        const s = l as SystemLog;
        const tsHuman = fmt(s.ts).toLowerCase();
        return (
          (s.message || '').toLowerCase().includes(q) ||
          (s.ts || '').toLowerCase().includes(q) ||
          tsHuman.includes(q) ||
          (s.worldId || '').toLowerCase().includes(q) ||
          (s.instanceId || '').toLowerCase().includes(q) ||
          (s.region || '').toLowerCase().includes(q)
        );
      } else {
        const u = l as UserJoinLog;
        const joinedHuman = fmt(u.joinedAt).toLowerCase();
        const leftHuman = fmt(u.leftAt).toLowerCase();
        return (
          (u.username || '').toLowerCase().includes(q) ||
          (u.userId || '').toLowerCase().includes(q) ||
          (u.joinedAt || '').toLowerCase().includes(q) ||
          (u.leftAt || '').toLowerCase().includes(q) ||
          joinedHuman.includes(q) ||
          leftHuman.includes(q)
        );
      }
    });
  }

  onMount(() => {
    const unsubs: Array<() => void> = [];

    (async () => {
      try { await invoke('start_log_watcher'); } catch(e) { console.error("Failed to start log watcher", e); }

      const { listen } = await import('@tauri-apps/api/event');

      const unlistenReady = await listen('watcher_ready', async () => {
        console.debug('[event:watcher_ready] Watcher is ready, fetching initial data.');
        unlistenReady();
        
        try {
          const initialActive: any[] = await invoke('get_active_join_logs');
          activeUsers = initialActive.map(l => ({ ...l, type: 'user' }));
          console.debug('Loaded initial active users from DB', { count: activeUsers.length });
        } catch(e) {
          console.error("Failed to get active join logs", e);
        }
      });
      unsubs.push(unlistenReady);

      unsubs.push(await listen('db_row_inserted', (e: any) => {
        console.debug('[event:db_row_inserted]', e);
        const p = e.payload || {};
        if (p.type === 'system') {
          const sys: SystemLog = { type: 'system', ts: p.ts, message: p.message || 'System', worldId: p.worldId, instanceId: p.instanceId, region: p.region };
          if (logPage === 0) {
            pagedLogs = [sys, ...pagedLogs].slice(0, LOGS_PER_PAGE);
          }
          return;
        }
        const newEntry: UserJoinLog = { ...p, type: 'user' };
        activeUsers = [newEntry, ...activeUsers];
        if (logPage === 0) {
          pagedLogs = [newEntry, ...pagedLogs].slice(0, LOGS_PER_PAGE);
        }
      }));

      unsubs.push(await listen('db_row_updated', (e: any) => {
        console.debug('[event:db_row_updated]', e);
        const { id, userId, leftAt } = e.payload;

        activeUsers = activeUsers.filter(u => u.id !== id && u.userId !== userId);
        
        const pagedIdx = pagedLogs.findIndex(l => (l as UserJoinLog).id === id);
        if (pagedIdx !== -1) {
          const updatedLog = { ...(pagedLogs[pagedIdx] as UserJoinLog), leftAt } as UserJoinLog;
          pagedLogs[pagedIdx] = updatedLog;
          pagedLogs = [...pagedLogs];
        }
      }));

      unsubs.push(await listen('db_purged', (e: any) => {
        console.debug('[event:db_purged]', e);
        const { ts } = e.payload;
        activeUsers = [];
        pagedLogs = pagedLogs.map(l => {
          if (l.type === 'user' && !(l as UserJoinLog).leftAt) {
            return { ...(l as UserJoinLog), leftAt: ts } as UserJoinLog;
          }
          return l;
        });
      }));

      // instance_changed kept for UX; DB row will also arrive
      unsubs.push(await listen('instance_changed', (e: any) => {
        console.debug('[event:instance_changed]', e);
      }));

    })();

    return () => { unsubs.forEach((u) => typeof u === 'function' && u()); };
  });
</script>

<div class="panel">
  <div class="header">
    <div class="tabs">
      <button aria-label="Live View" class:active={tab==='monitor'} title="Live View" onclick={() => tab='monitor'}>Live View</button>
      <button aria-label="Join Logs" class:active={tab==='logs'} title="Join Logs" onclick={showJoinLogs}>Join Logs</button>
    </div>
    <div class="meta">{activeUsers.length} user{s(activeUsers.length)} in instance</div>
  </div>
  {#if tab === 'logs'}
    <div class="tools">
      <input placeholder="Search logs... (user, ID, time)" bind:value={logsQuery} oninput={() => {}} />
      <div class="actions">
        <button class="delete" onclick={handleDeleteClick}>
          {#if confirmingDelete}Confirm{:else}Delete Logs{/if}
        </button>
        <div class="pagination">
          <button disabled={logPage === 0} onclick={() => fetchPage(logPage - 1)}>Prev</button>
          <span>Page {logPage + 1}</span>
          <button onclick={() => fetchPage(logPage + 1)}>Next</button>
        </div>
      </div>
    </div>
  {/if}

  <div class="list" role="list">
    {#if tab === 'monitor'}
      {#if activeUsers.length === 0}
        <div class="empty">Waiting for players…</div>
      {:else}
        {#each activeUsers as ul (ul.userId + (ul.joinedAt || ''))}
          <div class="row" role="listitem">
            <div class="avatar" aria-hidden="true">{firstGrapheme(ul.username) || '?'}</div>
            <div class="col">
              <div class="name">{ul.username || 'Unknown'}</div>
              <div class="sub">{ul.userId}</div>
            </div>
            <div class="actions stats">Warns: 0&nbsp;|&nbsp;Kicks: 0&nbsp;|&nbsp;Bans: 0
              {ensureWatchLoaded(ul.userId)}
              <button class="watch" class:active={watch.get(ul.userId)} title="Watchlist" aria-label="Watchlist" onclick={async () => { const newVal = !watch.get(ul.userId); try { await invoke('set_watch', { userId: ul.userId, watch: newVal }); watch.set(ul.userId, newVal); watch = new Map(watch); } catch {} }}>
                {#if watch.get(ul.userId)}
                  <!-- open eye -->
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                    <path d="M2 12c4-7 16-7 20 0" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
                    <path d="M2 12c4 7 16 7 20 0" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
                    <circle cx="12" cy="12" r="3" fill="currentColor"/>
                  </svg>
                {:else}
                  <!-- closed eye -->
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                    <path d="M2 12c4-6 16-6 20 0" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
                  </svg>
                {/if}
              </button>
              {ensureNoteLoaded(ul.userId)}
              <button class="note" class:has-note={noted.get(ul.userId)} title="Edit note" aria-label="Edit note" onclick={async () => { showNoteFor = ul.userId; try { const res: any = await invoke('get_note', { userId: ul.userId }); noteText = (res && res.text) ? res.text : ''; } catch { noteText = ''; } }}>
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                  <rect x="5" y="3" width="14" height="18" rx="2" stroke="currentColor" stroke-width="1.5"/>
                  <path d="M8 8h8M8 12h8M8 16h6" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
                </svg>
              </button>
            </div>
          </div>
          {#if showNoteFor === ul.userId}
            <div class="note-editor">
              <textarea placeholder="Write a note..." bind:value={noteText}></textarea>
              <div class="note-actions">
                <button onclick={() => { showNoteFor = null; noteText = ''; }}>Cancel</button>
                <button onclick={async () => { try { await invoke('add_note', { userId: ul.userId, text: noteText }); noted.set(ul.userId, noteText.trim().length > 0); noted = new Map(noted); } catch {}; showNoteFor = null; noteText = ''; }}>Save</button>
              </div>
            </div>
          {/if}
        {/each}
      {/if}
    {:else}
      {#if filteredLogs().length === 0}
        <div class="empty">No joins recorded yet.</div>
      {:else}
        {#each filteredLogs() as l, i (l.type === 'system' ? ((l as any).ts || `sys-${i}`) : (((l as any).id || (l as any).userId) + ((l as any).joinedAt || `-${i}`)))}
          {#if l.type === 'system'}
            <div class="row system" role="listitem">
              <div class="avatar" aria-hidden="true">!</div>
              <div class="col">
                <div class="name">{(l as SystemLog).message}</div>
                <div class="sub">{fmt((l as SystemLog).ts)}</div>
              </div>
            </div>
          {:else}
            {@const ul = l as UserJoinLog}
            <div class="row" role="listitem">
              <div class="avatar" aria-hidden="true">{firstGrapheme(ul.username) || '?'}</div>
              <div class="col">
                <div class="name">{ul.username || 'Unknown'}</div>
                <div class="sub">{ul.userId}</div>
                <div class="sub time">
                  <span class="pill">Joined: {fmt(ul.joinedAt)}</span>
                  {#if ul.leftAt}
                    <span class="pill">Left: {fmt(ul.leftAt)}</span>
                  {/if}
                </div>
              </div>
              <div class="actions stats">
                {ensureWatchLoaded(ul.userId)}
                <button class="watch" class:active={watch.get(ul.userId)} title="Watchlist" aria-label="Watchlist" onclick={async () => { const newVal = !watch.get(ul.userId); try { await invoke('set_watch', { userId: ul.userId, watch: newVal }); watch.set(ul.userId, newVal); watch = new Map(watch); } catch {} }}>
                  {#if watch.get(ul.userId)}
                    <!-- open eye -->
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                      <path d="M2 12c4-7 16-7 20 0" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
                      <path d="M2 12c4 7 16 7 20 0" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
                      <circle cx="12" cy="12" r="3" fill="currentColor"/>
                    </svg>
                  {:else}
                    <!-- closed eye -->
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                      <path d="M2 12c4-6 16-6 20 0" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
                    </svg>
                  {/if}
                </button>
                {ensureNoteLoaded(ul.userId)}
                <button class="note" class:has-note={noted.get(ul.userId)} title="Edit note" aria-label="Edit note" onclick={async () => { showNoteFor = ul.userId; try { const res: any = await invoke('get_note', { userId: ul.userId }); noteText = (res && res.text) ? res.text : ''; } catch { noteText = ''; } }}>
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                    <rect x="5" y="3" width="14" height="18" rx="2" stroke="currentColor" stroke-width="1.5"/>
                    <path d="M8 8h8M8 12h8M8 16h6" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
                  </svg>
                </button>
              </div>
            </div>
            {#if showNoteFor === ul.userId}
              <div class="note-editor">
                <textarea placeholder="Write a note..." bind:value={noteText}></textarea>
                <div class="note-actions">
                  <button onclick={() => { showNoteFor = null; noteText = ''; }}>Cancel</button>
                  <button onclick={async () => { try { await invoke('add_note', { userId: ul.userId, text: noteText }); noted.set(ul.userId, noteText.trim().length > 0); noted = new Map(noted); } catch {}; showNoteFor = null; noteText = ''; }}>Save</button>
                </div>
              </div>
            {/if}
          {/if}
        {/each}
      {/if}
    {/if}
  </div>
</div>


<style>
  .panel { display: flex; flex-direction: column; gap: 12px; }
  .header { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
  .tabs { display: inline-flex; gap: 8px; }
  .tabs button { height: 36px; padding: 0 12px; border: 1px solid var(--border); background: var(--bg); color: var(--fg); border-radius: 8px; cursor: pointer; display: inline-flex; align-items: center; justify-content: center; font-weight: 600; }
  .tabs button.active { background: var(--bg-elev); border-color: var(--fg-muted); }
  .meta { color: var(--fg-muted); font-size: 12px; }

  .tools { display: flex; gap: 8px; align-items: center; margin-top: 6px; justify-content: space-between; }
  .tools input { flex: 1; min-width: 160px; border-radius: 8px; border: 1px solid var(--border); background: var(--bg); color: var(--fg); padding: 6px 10px; }
  .actions { display: inline-flex; gap: 8px; align-items: center; }
  .pagination { display: inline-flex; gap: 8px; align-items: center; }
  .pagination button { border: 1px solid var(--border); background: var(--bg); color: var(--fg); border-radius: 8px; padding: 4px 10px; cursor: pointer; }
  .pagination button:disabled { opacity: 0.5; cursor: not-allowed; }
  .delete { border: 1px solid var(--border-err); background: var(--bg-err); color: var(--fg-err); border-radius: 8px; padding: 4px 10px; cursor: pointer; font-weight: 600; }

  .list { border: 1px solid var(--border); border-radius: 12px; background: var(--bg-elev); min-height: 160px; max-height: calc(100vh - 220px); overflow: auto; }
  .empty { padding: 16px; color: var(--fg-muted); }

  .row { display: grid; grid-template-columns: 36px 1fr auto; gap: 12px; align-items: center; padding: 10px 12px; border-bottom: 1px solid var(--border); }
  .row:last-child { border-bottom: none; }
  .avatar { width: 36px; height: 36px; border-radius: 8px; background: var(--bg); color: var(--fg-muted); display: inline-flex; align-items: center; justify-content: center; font-weight: 600; }
  .name { color: var(--fg); font-weight: 600; }
  .sub { color: var(--fg-muted); font-size: 12px; }
  .actions.stats { color: var(--fg-muted); font-size: 12px; }
  .actions.stats .note { margin-left: 8px; border: 1px solid var(--border); background: var(--bg); color: var(--fg); border-radius: 6px; padding: 2px; cursor: pointer; display: inline-flex; align-items: center; justify-content: center; vertical-align: middle; }
  .actions.stats .note svg { display: block; }
  .actions.stats .note.has-note { background: rgba(255, 182, 193, 0.35); border-color: #ffb6c1; }
  .actions.stats .watch { margin-left: 8px; border: 1px solid var(--border); background: var(--bg); color: var(--fg); border-radius: 6px; padding: 2px; cursor: pointer; display: inline-flex; align-items: center; justify-content: center; vertical-align: middle; }
  .actions.stats .watch.active { background: rgba(255, 182, 193, 0.35); border-color: #ffb6c1; }
  .actions.stats .watch svg { display: block; }
  .note-editor { padding: 8px 12px; background: var(--bg-elev); border-top: 1px dashed var(--border); display: grid; gap: 8px; }
  .note-editor textarea { width: 100%; min-height: 72px; border: 1px solid var(--border); border-radius: 8px; background: var(--bg); color: var(--fg); padding: 8px; resize: vertical; }
  .note-actions { display: inline-flex; gap: 8px; }
  .note-actions button { border: 1px solid var(--border); background: var(--bg); color: var(--fg); border-radius: 8px; padding: 4px 10px; cursor: pointer; }
  .sub.time { margin-top: 4px; display: flex; gap: 6px; flex-wrap: wrap; }
  .pill { border: 1px solid var(--border); background: var(--bg); color: var(--fg); border-radius: 999px; padding: 2px 8px; }
  .row.system .avatar { background: var(--bg); color: var(--fg); }
</style>
