<script lang="ts">
import Sidebar from '$lib/components/Sidebar.svelte';
import LoginPage from './+page.svelte';
import InstanceMonitor from './instance-monitor/+page.svelte';
import LogExplorer from './log-explorer/+page.svelte';
import DatabasePage from './database/+page.svelte';
import SettingsPanel from './settings/+page.svelte';
import DebugPanel from './debug/+page.svelte';
import WorldModeration from './world-moderation/+page.svelte';
import { onMount } from 'svelte';
import { invoke } from '@tauri-apps/api/core';
import { pushDebug, setApiQueueLength } from '$lib/stores/debugLog';
import { getResultsStore, pushResult } from '$lib/stores/apiChecks';
  let collapsed = $state(true);
  let activeIndex = $state(0);

  function toggleSidebar() { collapsed = !collapsed; }
  function selectTab(i: number) { activeIndex = i; }

const tabs = [
  { title: 'Login', component: LoginPage },
  { title: 'Instance Monitor', component: InstanceMonitor },
  { title: 'Database', component: DatabasePage },
  { title: 'Log Explorer', component: LogExplorer },
  { title: 'World Moderation', component: WorldModeration },
  { title: 'Settings', component: SettingsPanel },
  { title: 'Debug', component: DebugPanel },
  { title: 'About', component: null }
];

const tabTitles = tabs.map((tab) => tab.title);

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
    if (tokens.length === 0) {
      pushDebug('[group-watch] abort :: no tokens available');
      return;
    }

    const API_BASE: string = (import.meta as any)?.env?.VITE_API_BASE || 'https://fch-toolkit.com';
    try {
      console.debug('[group-batch] sending', { userIdsCount: userIds.length, tokensCount: tokens.length });
      pushDebug(`[group-watch] batch start :: users=${userIds.length} tokens=${tokens.length}`);
      const resp = await fetch(`${API_BASE}/check-user`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ userIds, tokens }) });
      if (!resp.ok) {
        pushDebug(`[group-watch] server responded ${resp.status} ${resp.statusText}`);
        return;
      }
      const data: any = await resp.json();
      if (data && (Array.isArray(data.matches) || Array.isArray(data.aggregates))) {
        // Merge into global cache for hydration when tabs mount later
        try {
          pushDebug(`[group-watch] received matches=${(data.matches || []).length} aggregates=${(data.aggregates || []).length}`);
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
            pushDebug(`[group-watch] playing group sound for ${matchedUserIds.length} user(s)`);
            try { await invoke('preview_group_sound'); } catch (err) { pushDebug(`[group-watch] group sound failed :: ${err}`); }
          }
        } catch {}
        // Persist group flags in DB for join logs backfill (no sound here)
        try {
          const matchedUserIds = Array.from(new Set((data.matches || []).filter((m: any) => Boolean(m.watchlist)).map((m: any) => String(m.user_id))));
          if (matchedUserIds.length > 0) {
            try {
              const changed = await invoke<number>('set_group_watchlisted_for_users', { userIds: matchedUserIds });
              pushDebug(`[group-watch] persisted watch flags for ${matchedUserIds.length} user(s) :: rowsUpdated=${changed}`);
            } catch (err) {
              pushDebug(`[group-watch] failed to persist watch flags :: ${err}`);
            }
          }
        } catch {}
        // Broadcast results so UI (Instance Monitor) can update flags if mounted
        try {
          const evt = new CustomEvent('group_watch_results', { detail: { matches: data.matches || [], aggregates: data.aggregates || [] } });
          window.dispatchEvent(evt);
          pushDebug(`[group-watch] dispatched results event`);
        } catch {}
      }
    } catch (err) {
      pushDebug(`[group-watch] batch error :: ${err}`);
    }
  }

  onMount(() => {
    let unlistenInserted: undefined | (() => void);
    let unlistenSound: undefined | (() => void);
    let unlistenDebug: undefined | (() => void);
    let unlistenApiQueue: undefined | (() => void);
    let unsubscribeApiResults: undefined | (() => void);
    (async () => {
      try { await invoke('start_log_watcher'); } catch {}
      pushDebug('[VRCAPI] apiChecks ready (HTTP mode)');
      unsubscribeApiResults = getResultsStore().subscribe((entries) => {
        const latest = entries[entries.length - 1];
        if (!latest) return;
        pushDebug(
          `[VRCAPI] apiChecks completed :: ${latest.file_id} v${latest.version} :: success=${latest.success}`
        );
      });
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
        unlistenApiQueue = await listen('api_queue_length', (event: any) => {
          const len = Number(event?.payload ?? 0);
          setApiQueueLength(Number.isFinite(len) ? len : 0);
        });
        unlistenSound = await listen('sound_triggered', (e: any) => {
          const payload = e?.payload || {};
          console.debug('[sound] local_watchlist', payload);
          pushDebug(`[sound] local watch triggered :: ${JSON.stringify(payload)}`);
        });
        unlistenDebug = await listen('debug_log', (e: any) => {
          const { message, ts } = e?.payload || {};
          if (typeof message === 'string') {
            pushDebug(message, typeof ts === 'string' ? ts : undefined);
          }
        });
        await listen('api_checks_result', async (e: any) => {
          const payload = e?.payload;
          if (!payload || typeof payload !== 'object') return;

          const normalized = Object.entries(payload).reduce((acc, [key, value]) => {
            const camelKey = key.replace(/_([a-z])/g, (_, c) => c.toUpperCase());
            acc[camelKey] = value;
            return acc;
          }, {} as Record<string, unknown>);

          pushDebug(
            `[avatarData] file=${String((normalized as any).fileId ?? '')} v=${String((normalized as any).version ?? '')} :: file=${JSON.stringify((normalized as any).file ?? null)} :: security=${JSON.stringify((normalized as any).security ?? null)}`
          );

          pushResult({
            file_id: String((normalized as any).fileId ?? ''),
            version: Number((normalized as any).version ?? 0),
            success: Boolean((normalized as any).success !== false),
            errors: Array.isArray((normalized as any).errors)
              ? ((normalized as any).errors as string[])
              : (normalized as any).error
              ? [String((normalized as any).error)]
              : undefined,
            timestamp:
              typeof (normalized as any).timestamp === 'string'
                ? (normalized as any).timestamp
                : undefined,
            file: (normalized as any).file,
            security: (normalized as any).security,
            raw: payload,
          });
        });
      } catch {}
    })();
    return () => { try {
      if (batchTimer) clearInterval(batchTimer);
      if (unlistenApiQueue) unlistenApiQueue();
      unsubscribeApiResults?.();
      unlistenInserted?.();
      unlistenSound?.();
      unlistenDebug?.();
    } catch {} };
  });
</script>

<div class="app">
  <Sidebar {collapsed} {activeIndex} onToggle={toggleSidebar} onSelect={selectTab} />

  <main>
    <header>
      <h1>{tabTitles[activeIndex]}</h1>
    </header>
    <section class="content">
      {#each tabs as tab, index}
        <div
          class="tab"
          class:active={activeIndex === index}
          aria-hidden={activeIndex === index ? undefined : 'true'}
        >
          {#if tab.component}
            <svelte:component this={tab.component} />
          {:else}
            <div class="placeholder">
              <p>Content for <strong>{tab.title}</strong> will appear here.</p>
            </div>
          {/if}
        </div>
      {/each}
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

  .content { flex: 1; overflow: hidden; padding: 16px; position: relative; }

  .tab { display: none; height: 100%; overflow: auto; }
  .tab.active { display: block; }

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

