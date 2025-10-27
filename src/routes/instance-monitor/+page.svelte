<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { pushDebug, setLiveViewCounts } from '$lib/stores/debugLog';

  type UserJoinLog = { id?: number; type: 'user'; userId: string; username?: string; joinedAt?: string; leftAt?: string; groupWatchlisted?: boolean };
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
  let watchFlags = $state(new Map<string, { watchlist: boolean; notes?: string }>());
  let modAgg = $state(new Map<string, { warns: number; kicks: number; bans: number }>());
  let avatarPerf = $state(new Map<string, { rating: string; updatedAt?: string }>());
  type GroupMatch = { group_id: string; groupName?: string; notes?: string; watchlist?: boolean };
  let showWatchModal: { userId: string; items: GroupMatch[] } | null = $state(null);
  type AvatarModalState = {
    userId: string;
    username?: string;
    avatarName?: string;
    timestamp?: string;
    details: any[];
    loading: boolean;
    error?: string;
  };
  let avatarModal: AvatarModalState | null = $state(null);
  let lastMatches = $state(new Map<string, GroupMatch[]>());

  const closeWatchModal = () => { showWatchModal = null; };
  const closeAvatarModal = () => { avatarModal = null; };

  const handleBackdropKey = (e: KeyboardEvent) => {
    const key = e.key;
    if (key === 'Escape' || key === 'Enter' || key === ' ') {
      e.preventDefault();
      showWatchModal = null;
    }
  };

  const stopModalClick = (event: MouseEvent) => {
    event.stopPropagation();
  };

  const handleModalKey = (event: KeyboardEvent) => {
    if (event.key === 'Escape') {
      event.stopPropagation();
      event.preventDefault();
      closeWatchModal();
    }
  };

  const handleAvatarBackdropKey = (e: KeyboardEvent) => {
    const key = e.key;
    if (key === 'Escape' || key === 'Enter' || key === ' ') {
      e.preventDefault();
      closeAvatarModal();
    }
  };

  const stopAvatarModalClick = (event: MouseEvent) => {
    event.stopPropagation();
  };

  const handleAvatarModalKey = (event: KeyboardEvent) => {
    if (event.key === 'Escape') {
      event.stopPropagation();
      event.preventDefault();
      closeAvatarModal();
    }
  };

  function hasData(obj: unknown): boolean {
    return !!(obj && typeof obj === 'object' && Object.keys(obj as Record<string, unknown>).length > 0);
  }

  function toPrettyJson(value: unknown): string {
    try {
      return JSON.stringify(value, null, 2);
    } catch {
      return String(value ?? '');
    }
  }

  async function openAvatarModal(userId: string, username?: string) {
    let state: AvatarModalState = { userId, username, details: [], loading: true };
    avatarModal = state;
    try {
      const latest: any = await invoke('get_latest_avatar_for_user', { userId, username });
      const rawName = latest?.avatarName ?? latest?.avatar_name ?? '';
      const avatarName = typeof rawName === 'string' ? rawName.trim() : '';
      const timestamp = typeof latest?.timestamp === 'string' ? latest.timestamp : undefined;
      state = { ...state, avatarName: avatarName || undefined, timestamp };
      avatarModal = state;

      let details: any[] = [];
      if (avatarName) {
        try {
          const res: any = await invoke('db_get_avatar_details', { avatarName });
          if (Array.isArray(res)) {
            details = res;
          }
        } catch (detailErr) {
          console.error('Failed to load avatar details', detailErr);
        }
      }

      state = { ...state, details, loading: false };
      avatarModal = state;
    } catch (err) {
      console.error('Failed to load avatar data', err);
      state = { ...state, loading: false, error: 'Failed to load avatar data.' };
      avatarModal = state;
    }
  }

  async function pollAvatarStatus() {
    pushDebug('[avatarPerf] poll start');
    const next = new Map<string, { rating: string; updatedAt?: string }>();
    if (activeUsers.length === 0) {
      pushDebug('[avatarPerf] skipped, no active users');
      avatarPerf = next;
      return;
    }
    await Promise.all(
      activeUsers.map(async (user) => {
        try {
          const latest: any = await invoke('get_latest_avatar_for_user', {
            userId: user.userId,
            username: user.username || undefined
          });
          const rawName = latest?.avatarName ?? latest?.avatar_name ?? '';
          const avatarName = typeof rawName === 'string' ? rawName.trim() : '';
          if (!avatarName) {
            return;
          }
          const details: any[] = await invoke('db_get_avatar_details', { avatarName });
          const rating = Array.isArray(details) && details.length > 0
            ? details[0]?.performanceRating ?? null
            : null;
          if (rating && typeof rating === 'string') {
            next.set(user.userId, { rating, updatedAt: latest?.timestamp });
            pushDebug(`[avatarPerf] rating updated :: user=${user.userId} username=${user.username} avatar=${avatarName} rating=${rating}`);
          } else {
            pushDebug(`[avatarPerf] rating missing :: user=${user.userId} username=${user.username} avatar=${avatarName}`);
          }
        } catch (err) {
          console.warn('avatar poll failed', user.userId, err);
          pushDebug(`[avatarPerf] poll failed :: user=${user.userId} username=${user.username} err=${String(err)}`);
        }
      })
    );
    pushDebug(`[avatarPerf] poll complete :: updated=${next.size}`);
    avatarPerf = next;
    setLiveViewCounts(avatarPerf.size, activeUsers.length);
  }

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
      pushDebug('Join log table purged from UI (Live View cleared)');
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
    return { type: 'user', id: l.id, userId: l.userId, username: l.username, joinedAt: l.joinedAt, leftAt: l.leftAt, groupWatchlisted: !!l.groupWatchlisted } as UserJoinLog;
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
        resortActiveUsers();
      } catch {}
      loadingNotes.delete(userId);
    })();
  }

  function ensureWatchLoaded(userId: string) {
    if (!userId || watch.has(userId)) return;
    (async () => {
      try { const res: any = await invoke('get_watch', { userId }); watch.set(userId, !!(res && res.watch)); watch = new Map(watch); resortActiveUsers(); } catch {}
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

  // Consume group-watch results from global batcher (in +layout)
  function applyGroupWatchResults(payload: { matches: any[]; aggregates: any[] }) {
    try {
      const w: any = window as any;
      const cache = w.__FCH_GROUP_WATCH_CACHE__;
      if (cache && cache.flags) {
        const map = new Map<string, { watchlist: boolean; notes?: string }>();
        for (const [uid, flag] of Object.entries<any>(cache.flags)) {
          map.set(uid, { watchlist: Boolean(flag.watchlist), notes: flag.notes });
        }
        watchFlags = map;
        // Also hydrate per-user matches for the modal
        const perUser = new Map<string, GroupMatch[]>();
        if (cache.matchesByUser) {
          for (const [uid, arr] of Object.entries<any>(cache.matchesByUser)) {
            const list: GroupMatch[] = Array.isArray(arr)
              ? (arr as any[]).map((m) => ({
                  group_id: String(m.group_id),
                  groupName: m.groupName,
                  notes: m.notes,
                  watchlist: Boolean(m.watchlist)
                }))
              : [];
            perUser.set(uid, list);
          }
        }
        lastMatches = perUser;
      } else {
        const map = new Map<string, { watchlist: boolean; notes?: string }>();
        const perUserMatches = new Map<string, GroupMatch[]>();
        for (const m of (payload.matches || [])) {
          const uid = String(m.user_id);
          const watchlist = Boolean(m.watchlist);
          const notifications = Boolean(m.notifications);
          const notes = m.notes ? String(m.notes) : undefined;
          const prev = map.get(uid);
          map.set(uid, { watchlist: (prev?.watchlist || watchlist), notes: prev?.notes || notes });
          const arr = perUserMatches.get(uid) || [];
          arr.push({ group_id: String(m.group_id), groupName: m.groupName, notes, watchlist, notifications });
          perUserMatches.set(uid, arr);
        }
        watchFlags = map;
        lastMatches = perUserMatches;
      }
      {
        const w: any = window as any;
        const cache = w.__FCH_GROUP_WATCH_CACHE__;
        const aggMap = new Map<string, { warns: number; kicks: number; bans: number }>();
        if (cache && cache.aggregates) {
          for (const [uid, v] of Object.entries<any>(cache.aggregates)) {
            aggMap.set(uid, { warns: Number(v.warns)||0, kicks: Number(v.kicks)||0, bans: Number(v.bans)||0 });
          }
        } else {
          for (const a of (payload.aggregates || [])) {
            aggMap.set(String(a.user_id), { warns: Number(a.warns)||0, kicks: Number(a.kicks)||0, bans: Number(a.bans)||0 });
          }
        }
        modAgg = aggMap;
      }
      resortActiveUsers();
    } catch {}
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

  function resortActiveUsers() {
    // Priority: any local note OR local watch OR group watchlisted => top
    const score = (u: UserJoinLog): number => {
      const hasLocalNote = noted.get(u.userId) === true;
      const hasLocalWatch = watch.get(u.userId) === true;
      const hasGroupFlag = watchFlags.has(u.userId);
      return (hasLocalNote || hasLocalWatch || hasGroupFlag) ? 1 : 0;
    };
    activeUsers = [...activeUsers].sort((a, b) => {
      const sb = score(b) - score(a);
      if (sb !== 0) return sb;
      // Stable-ish fallback: most recent join first if timestamps exist
      const at = a.joinedAt || '';
      const bt = b.joinedAt || '';
      return bt.localeCompare(at);
    });
  }

  onMount(() => {
    const unsubs: Array<() => void> = [];
    let pollTimer: number | undefined;
    let hydrateTimer: number | undefined;
    let hydrationInFlight = false;
    let initialHydrated = false;
    const handleGroupResults = (ev: any) => {
      const d = ev?.detail;
      if (!d) return;
      applyGroupWatchResults({ matches: d.matches || [], aggregates: d.aggregates || [] });
    };
    window.addEventListener('group_watch_results', handleGroupResults as any);
    (async () => {
      try {
        const { listen } = await import('@tauri-apps/api/event');
        unsubs.push(await listen('sound_triggered', (e: any) => {
          console.debug('[sound] local_watchlist(page)', e?.payload || {});
        }));
      } catch {}
    })();

    async function hydrateActiveUsers(source: string) {
      if (initialHydrated || hydrationInFlight) return;
      hydrationInFlight = true;
      let succeeded = false;
      try {
        const initialActive: any[] = await invoke('get_active_join_logs');
        const latestByUser = new Map<string, any>();
        for (const row of initialActive) {
          const uid = String(row.userId);
          const prev = latestByUser.get(uid);
          if (!prev || String(row.joinedAt || '') > String(prev.joinedAt || '')) {
            latestByUser.set(uid, row);
          }
        }
        const deduped = Array.from(latestByUser.values());
        activeUsers = deduped.map((l) => ({ ...l, type: 'user' }));
        setLiveViewCounts(avatarPerf.size, activeUsers.length);
        pushDebug(`Backfilled ${activeUsers.length} active user(s) via ${source}`);
        void pollAvatarStatus();
        const duplicates = initialActive.filter((r) => {
          const latest = latestByUser.get(String(r.userId));
          return !latest || String(r.joinedAt || '') < String(latest?.joinedAt || '');
        });
        if (duplicates.length > 0) {
          const ts = String((initialActive[initialActive.length - 1]?.joinedAt) || '');
          for (const d of duplicates) {
            try {
              await invoke('db_update_leave', { ts, userId: d.userId });
            } catch {}
          }
        }
        console.debug('Loaded initial active users from DB', { count: activeUsers.length, source });
        applyGroupWatchResults({ matches: [], aggregates: [] });
        initialHydrated = true;
        succeeded = true;
      } catch (e) {
        console.error('Failed to get active join logs', e);
        hydrationInFlight = false;
        return;
      } finally {
        if (!succeeded) {
          hydrationInFlight = false;
        }
      }
      hydrationInFlight = false;
    }

    (async () => {
      const { listen } = await import('@tauri-apps/api/event');

      const unlistenReady = await listen('watcher_ready', async () => {
        console.debug('[event:watcher_ready] Watcher is ready, fetching initial data.');
        unlistenReady();
        await hydrateActiveUsers('watcher_ready');
      });
      unsubs.push(unlistenReady);

      // Safety net: hydrate once shortly after mount in case watcher_ready fired before we mounted.
      hydrateTimer = window.setTimeout(() => { void hydrateActiveUsers('delayed_hydrate'); }, 250);

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
        resortActiveUsers();
        setLiveViewCounts(
          Math.min(avatarPerf.size, activeUsers.length),
          activeUsers.length
        );
        pollAvatarStatus();
        pushDebug(`User joined: ${newEntry.username || 'Unknown'} (${newEntry.userId})`);
        if (logPage === 0) {
          pagedLogs = [newEntry, ...pagedLogs].slice(0, LOGS_PER_PAGE);
        }
      }));

      unsubs.push(await listen('db_row_updated', (e: any) => {
        console.debug('[event:db_row_updated]', e);
        const { id, userId, leftAt } = e.payload;

        activeUsers = activeUsers.filter(u => u.id !== id && u.userId !== userId);
        resortActiveUsers();
        setLiveViewCounts(
          Math.min(avatarPerf.size, activeUsers.length),
          activeUsers.length
        );
        pollAvatarStatus();
        if (userId) {
          pushDebug(`User left: ${userId} at ${leftAt || 'unknown'}`);
        }
        
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
        avatarPerf.clear();
        avatarPerf = new Map(avatarPerf);
        pushDebug(`Live View cleared due to purge at ${ts}`);
        setLiveViewCounts(0, 0);
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

    const startPolling = () => {
      if (pollTimer) clearInterval(pollTimer);
      pollTimer = window.setInterval(() => {
        void pollAvatarStatus();
      }, 10_000);
      void pollAvatarStatus();
    };

    startPolling();

    return () => {
      unsubs.forEach((u) => typeof u === 'function' && u());
      window.removeEventListener('group_watch_results', handleGroupResults as any);
      if (pollTimer) clearInterval(pollTimer);
      if (hydrateTimer) clearTimeout(hydrateTimer);
    };
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
            <div class="avatar" aria-hidden="true" class:flag-red={watchFlags.get(ul.userId)?.watchlist} class:flag-yellow={!watchFlags.get(ul.userId)?.watchlist && watchFlags.has(ul.userId)} title={watchFlags.get(ul.userId)?.notes || ''}>{firstGrapheme(ul.username) || '?'}</div>
            <div class="col">
              <div class="name">{ul.username || 'Unknown'}
                {#if ((watchFlags.get(ul.userId)?.notes ?? '').includes('{BOS}'))}
                  <span class="pill bos small" title="BOS">BOS</span>
                {/if}
                {#if watchFlags.has(ul.userId)}
                  <button class="pill link small" onclick={() => { const items = lastMatches.get(ul.userId) || []; showWatchModal = { userId: ul.userId, items }; }}>Group Watchlisted [Click Me]</button>
                {/if}
              </div>
              <div class="sub">{ul.userId}</div>
            </div>
            <div class="actions stats">
              {#if avatarPerf.get(ul.userId)?.rating}
                <span class="pill perf" title={`Avatar performance rating: ${avatarPerf.get(ul.userId)?.rating}`}>
                  {avatarPerf.get(ul.userId)?.rating}
                </span>
              {/if}
              <button
                class="avatar-details"
                title="Avatar details"
                aria-label="Avatar details"
                onclick={() => openAvatarModal(ul.userId, ul.username)}
              >
                <svg width="18" height="18" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                  <circle cx="12" cy="7" r="4" stroke="currentColor" stroke-width="1.5" />
                  <path d="M5 20c0-3.314 3.134-6 7-6s7 2.686 7 6" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
                </svg>
              </button>
              <span class="stats-pill">Warns: {modAgg.get(ul.userId)?.warns ?? 0}&nbsp;|&nbsp;Kicks: {modAgg.get(ul.userId)?.kicks ?? 0}&nbsp;|&nbsp;Bans: {modAgg.get(ul.userId)?.bans ?? 0}</span>
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
              <div class="avatar" aria-hidden="true" class:flag-red={(watchFlags.get(ul.userId)?.watchlist) ?? false} class:flag-yellow={!((watchFlags.get(ul.userId)?.watchlist) ?? false) && watchFlags.has(ul.userId)} title={watchFlags.get(ul.userId)?.notes || ''}>{firstGrapheme(ul.username) || '?'}</div>
              <div class="col">
                <div class="name">{ul.username || 'Unknown'}
                  {#if ((watchFlags.get(ul.userId)?.notes ?? '').includes('{BOS}'))}
                    <span class="pill bos small" title="BOS">BOS</span>
                  {/if}
                  {#if watchFlags.has(ul.userId)}
                    <button class="pill link small" onclick={() => { const items = lastMatches.get(ul.userId) || []; showWatchModal = { userId: ul.userId, items }; }}>Group Watchlisted [Click Me]</button>
                  {/if}
                </div>
                <div class="sub">{ul.userId}</div>
                <div class="sub time">
                  <span class="pill">Joined: {fmt(ul.joinedAt)}</span>
                  {#if ul.leftAt}
                    <span class="pill">Left: {fmt(ul.leftAt)}</span>
                  {/if}
                </div>
              </div>
            <div class="actions stats">
              <button
                class="avatar-details"
                title="Avatar details"
                aria-label="Avatar details"
                onclick={() => openAvatarModal(ul.userId, ul.username)}
              >
                <svg width="18" height="18" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                  <circle cx="12" cy="7" r="4" stroke="currentColor" stroke-width="1.5" />
                  <path d="M5 20c0-3.314 3.134-6 7-6s7 2.686 7 6" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
                </svg>
              </button>
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

{#if showWatchModal}
  <div
    class="modal-backdrop"
    role="button"
    aria-label="Close dialog"
    tabindex="0"
    onkeydown={handleBackdropKey}
    onclick={closeWatchModal}
  >
    <div class="modal" role="dialog" aria-modal="true" tabindex="-1" onclick={stopModalClick} onkeydown={handleModalKey}>
      <header>Group Watchlisted</header>
      <div class="body">
        {#if showWatchModal.items && showWatchModal.items.length > 0}
          {#each showWatchModal.items as it}
            <div class="gw-item">
              <div class="gw-line"><strong>{it.groupName || it.group_id}</strong></div>
              <div class="gw-line">Notifications: - {it.notifications ? 'On' : 'Off'}</div>
              <div class="gw-note">{it.notes || '—'}</div>
              <hr />
            </div>
          {/each}
        {:else}
          No notes.
        {/if}
      </div>
      <footer>
        <button onclick={closeWatchModal}>Close</button>
      </footer>
    </div>
  </div>
{/if}

{#if avatarModal}
  <div
    class="modal-backdrop"
    role="button"
    aria-label="Close dialog"
    tabindex="0"
    onkeydown={handleAvatarBackdropKey}
    onclick={closeAvatarModal}
  >
    <div class="modal" role="dialog" aria-modal="true" tabindex="-1" onclick={stopAvatarModalClick} onkeydown={handleAvatarModalKey}>
      <header>{avatarModal.avatarName || 'Avatar Details'}</header>
      <div class="body">
        {#if avatarModal.loading}
          <p>Loading…</p>
        {:else if avatarModal.error}
          <p class="error">{avatarModal.error}</p>
        {:else}
          <div class="meta-block">
            <div><strong>User:</strong> {avatarModal.username || avatarModal.userId}</div>
            {#if avatarModal.timestamp}
              <div><strong>Last Seen:</strong> {fmt(avatarModal.timestamp)}</div>
            {/if}
            {#if avatarModal.avatarName}
              <div><strong>Avatar:</strong> {avatarModal.avatarName}</div>
            {/if}
          </div>
          {#if avatarModal.details.length === 0}
            <p>No avatar details stored yet.</p>
          {:else}
            {#each avatarModal.details as detail, idx}
              <div class="detail-card">
                <div class="detail-row"><strong>Owner:</strong> {detail.ownerId}</div>
                <div class="detail-row"><strong>Version:</strong> {detail.version}</div>
                <div class="detail-row"><strong>File ID:</strong> {detail.fileId || '—'}</div>
                <div class="detail-row"><strong>Updated:</strong> {detail.updatedAt}</div>
                {#if hasData(detail.file)}
                  <details>
                    <summary>File JSON</summary>
                    <pre>{toPrettyJson(detail.file)}</pre>
                  </details>
                {/if}
                {#if hasData(detail.security)}
                  <details>
                    <summary>Security JSON</summary>
                    <pre>{toPrettyJson(detail.security)}</pre>
                  </details>
                {/if}
              </div>
              {#if idx < avatarModal.details.length - 1}
                <hr />
              {/if}
            {/each}
          {/if}
        {/if}
      </div>
      <footer>
        <button onclick={closeAvatarModal}>Close</button>
      </footer>
    </div>
  </div>
{/if}


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
  .avatar.flag-red { background: rgba(255, 0, 0, 0.25); border: 1px solid rgba(255,0,0,0.35); color: var(--fg); }
  .avatar.flag-yellow { background: rgba(255, 230, 0, 0.25); border: 1px solid rgba(255,230,0,0.35); color: var(--fg); }
  .name { color: var(--fg); font-weight: 600; }
  .sub { color: var(--fg-muted); font-size: 12px; }
  .actions.stats { color: var(--fg-muted); font-size: 12px; display: inline-flex; align-items: center; gap: 6px; }
  .actions.stats .avatar-details { border: 1px solid var(--border); background: var(--bg); color: var(--fg); border-radius: 6px; padding: 2px; cursor: pointer; display: inline-flex; align-items: center; justify-content: center; }
  .actions.stats .avatar-details:hover { background: rgba(255, 182, 193, 0.25); border-color: #ffb6c1; }
  .actions.stats .stats-pill { display: inline-flex; }
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
  .pill.small { font-size: 11px; padding: 1px 6px; margin-left: 6px; }
  .row.system .avatar { background: var(--bg); color: var(--fg); }
  .pill.link { cursor: pointer; }
  .pill.bos { background: rgba(255,255,255,0.06); border-color: #ffb6c1; }
  .pill.perf { background: rgba(0, 128, 255, 0.15); border-color: rgba(0, 128, 255, 0.45); font-weight: 600; }

  /* Simple modal */
  .modal-backdrop { position: fixed; inset: 0; background: rgba(0,0,0,0.6); display: flex; align-items: center; justify-content: center; z-index: 999; }
  .modal { width: min(560px, 92vw); background: var(--bg-elev); color: var(--fg); border: 1px solid var(--border); border-radius: 12px; box-shadow: 0 20px 60px rgba(0,0,0,0.35); }
  .modal header { padding: 12px 14px; border-bottom: 1px solid var(--border); font-weight: 700; }
  .modal .body { padding: 14px; white-space: pre-wrap; display: grid; gap: 12px; }
  .modal .body .meta-block { display: grid; gap: 4px; font-size: 13px; color: var(--fg); }
  .modal .body .detail-card { border: 1px solid var(--border); border-radius: 8px; padding: 10px; display: grid; gap: 6px; background: var(--bg); color: var(--fg); }
  .modal .body .detail-row { font-size: 13px; color: var(--fg-muted); }
  .modal .body details { background: rgba(255,255,255,0.03); border-radius: 6px; padding: 6px 8px; }
  .modal .body details summary { cursor: pointer; color: var(--fg); font-weight: 600; }
  .modal .body pre { max-height: 220px; overflow: auto; font-size: 12px; background: rgba(0,0,0,0.25); padding: 8px; border-radius: 6px; color: var(--fg); }
  .modal .body .error { color: var(--fg-err); }
  .modal .gw-item { padding: 6px 0; }
  .modal .gw-line { margin: 2px 0; }
  .modal .gw-note { margin: 4px 0 2px; opacity: 0.9; }
  .modal footer { padding: 12px 14px; border-top: 1px solid var(--border); display: flex; justify-content: flex-end; }
  .modal footer button { border: 1px solid var(--border); background: var(--bg); color: var(--fg); border-radius: 8px; padding: 6px 12px; cursor: pointer; }
</style>
