<script lang="ts">
  import Sidebar from '$lib/components/Sidebar.svelte';
  import LoginPage from './+page.svelte';
  import InstanceMonitor from './instance-monitor/+page.svelte';
  import LogExplorer from './log-explorer/+page.svelte';
  import DatabasePage from './database/+page.svelte';
  import SettingsPanel from './settings/+page.svelte';
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  let collapsed = $state(true);
  let activeIndex = $state(0);

  function toggleSidebar() { collapsed = !collapsed; }
  function selectTab(i: number) { activeIndex = i; }

  const tabTitles = [
    'Login', 'Instance Monitor', 'Database', 'Log Explorer', 'World Moderation', 'Settings', 'About'
  ];

  // Global join-driven group watch batching (runs regardless of active tab)
  let joinBatch = $state(new Set<string>());
  const JOIN_BATCH_DELAY = 400;
  const JOIN_BATCH_MAX = 100;
  let batchTimer: any = null;

  function scheduleBatchWatchCheck() {
    if (joinBatch.size >= JOIN_BATCH_MAX) { void flushJoinBatch(); return; }
    if (batchTimer) return;
    batchTimer = setTimeout(() => { void flushJoinBatch(); }, JOIN_BATCH_DELAY);
  }

  async function flushJoinBatch() {
    batchTimer = null;
    const userIds = Array.from(joinBatch);
    joinBatch = new Set();
    if (userIds.length === 0) return;

    let tokens: string[] = [];
    try {
      const res: any = await invoke('list_group_access_tokens');
      tokens = (Array.isArray(res) ? res : []).map((g: any) => String(g.token || '')).filter((t: string) => t.length >= 32);
    } catch {}
    if (tokens.length === 0) return;

    const API_BASE: string = (import.meta as any)?.env?.VITE_API_BASE || 'https://fch-toolkit.com';
    try {
      console.debug('[group-batch] sending', { userIdsCount: userIds.length, tokensCount: tokens.length });
      const resp = await fetch(`${API_BASE}/check-user`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ userIds, tokens }) });
      if (!resp.ok) return;
      const data: any = await resp.json();
      if (data && (Array.isArray(data.matches) || Array.isArray(data.aggregates))) {
        // Merge into global cache for hydration when tabs mount later
        try {
          const w: any = window as any;
          const cache = w.__FCH_GROUP_WATCH_CACHE__ || { flags: {}, matchesByUser: {}, aggregates: {} };
          for (const m of (data.matches || [])) {
            const uid = String(m.user_id);
            const watchlist = Boolean(m.watchlist);
            const notes = m.notes ? String(m.notes) : undefined;
            const prevFlag = cache.flags[uid] || {};
            cache.flags[uid] = { watchlist: Boolean(prevFlag.watchlist) || watchlist, notes: prevFlag.notes || notes };
            const arr = cache.matchesByUser[uid] || [];
            if (!arr.find((x: any) => String(x.group_id) === String(m.group_id))) {
              arr.push({ group_id: String(m.group_id), groupName: m.groupName, notes, watchlist });
            }
            cache.matchesByUser[uid] = arr;
          }
          for (const a of (data.aggregates || [])) {
            const uid = String(a.user_id);
            cache.aggregates[uid] = { warns: Number(a.warns)||0, kicks: Number(a.kicks)||0, bans: Number(a.bans)||0 };
          }
          w.__FCH_GROUP_WATCH_CACHE__ = cache;
        } catch {}
        // Play one sound per batch if any matched watchlisted users
        try {
          const matchedUserIds = Array.from(new Set((data.matches || []).filter((m: any) => Boolean(m.watchlist)).map((m: any) => String(m.user_id))));
          if (matchedUserIds.length > 0) {
            console.debug('[sound] group_watchlist', { matchedCount: matchedUserIds.length, userIds: matchedUserIds });
            try { await invoke('preview_sound'); } catch {}
          }
        } catch {}
        // Persist group flags in DB for join logs backfill (no sound here)
        try {
          const matchedUserIds = Array.from(new Set((data.matches || []).filter((m: any) => Boolean(m.watchlist)).map((m: any) => String(m.user_id))));
          if (matchedUserIds.length > 0) {
            await invoke('set_group_watchlisted_for_users', { userIds: matchedUserIds });
          }
        } catch {}
        // Broadcast results so UI (Instance Monitor) can update flags if mounted
        try {
          const evt = new CustomEvent('group_watch_results', { detail: { matches: data.matches || [], aggregates: data.aggregates || [] } });
          window.dispatchEvent(evt);
        } catch {}
      }
    } catch {}
  }

  onMount(() => {
    let unlistenInserted: undefined | (() => void);
    let unlistenSound: undefined | (() => void);
    (async () => {
      try { await invoke('start_log_watcher'); } catch {}
      try {
        const { listen } = await import('@tauri-apps/api/event');
        // On startup, dedupe open joins and proactively check current live users
        try { await invoke('dedupe_open_joins'); } catch {}
        try {
          const initial: any[] = await invoke('get_active_join_logs');
          const userIds = Array.from(new Set((Array.isArray(initial)?initial:[]).map((r:any)=>String(r.userId)).filter(Boolean)));
          if (userIds.length > 0) {
            // Reuse batch flow by inserting into the batch and flushing immediately
            userIds.forEach(uid => joinBatch.add(uid));
            await flushJoinBatch();
          }
        } catch {}
        unlistenInserted = await listen('db_row_inserted', (e: any) => {
          const p = e?.payload || {};
          if (p && !p.type && p.userId) {
            joinBatch.add(String(p.userId));
            scheduleBatchWatchCheck();
          }
        });
        unlistenSound = await listen('sound_triggered', (e: any) => {
          console.debug('[sound] local_watchlist', e?.payload || {});
        });
      } catch {}
    })();
    return () => { try { unlistenInserted && unlistenInserted(); unlistenSound && unlistenSound(); } catch {} };
  });
</script>

<div class="app">
  <Sidebar {collapsed} {activeIndex} onToggle={toggleSidebar} onSelect={selectTab} />

  <main>
    <header>
      <h1>{tabTitles[activeIndex]}</h1>
    </header>
    <section class="content">
      {#if activeIndex === 0}
        {#key activeIndex}
          <LoginPage/>
        {/key}
      {:else if activeIndex === 1}
        {#key activeIndex}
          <InstanceMonitor/>
        {/key}
      {:else if activeIndex === 2}
        {#key activeIndex}
          <DatabasePage/>
        {/key}
      {:else if activeIndex === 3}
        {#key activeIndex}
          <LogExplorer/>
        {/key}
      {:else if activeIndex === 5}
        {#key activeIndex}
          <SettingsPanel/>
        {/key}
      {:else}
        <div class="placeholder">
          <p>Content for <strong>{tabTitles[activeIndex]}</strong> will appear here.</p>
        </div>
      {/if}
    </section>
  </main>
</div>

<style>
  :global(html, body, #app) { height: 100%; }
  :global(body) { margin: 0; background: var(--bg); color: var(--fg); font-family: system-ui, Segoe UI, Roboto, Arial, sans-serif; }
  :global(*), :global(*::before), :global(*::after) { box-sizing: border-box; }

  .app {
    display: grid;
    grid-template-columns: auto 1fr;
    grid-template-rows: 100vh;
    width: 100vw;
    height: 100vh;
    overflow: hidden;
    background: var(--bg);
  }

  main { display: flex; flex-direction: column; min-width: 0; }
  header {
    display: flex; align-items: center; justify-content: space-between;
    padding: 14px 18px; border-bottom: 1px solid var(--border); background: var(--bg-elev);
  }
  header h1 { margin: 0; font-size: 15px; font-weight: 600; color: var(--fg); }

  .content { flex: 1; overflow: auto; padding: 16px; }

  .placeholder {
    margin-top: 16px;
    padding: 16px;
    border: 1px dashed var(--border);
    border-radius: 12px;
    background: linear-gradient(180deg, rgba(255,255,255,0.02), rgba(0,0,0,0.02));
    color: var(--fg-muted);
  }

  @media (max-width: 900px) {
    .app { grid-template-columns: auto 1fr; }
  }
</style>

