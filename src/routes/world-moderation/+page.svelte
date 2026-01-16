<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';

  type BanLogEntry = {
    id: number;
    admin: string;
    target: string;
    reason: string;
    timestamp: string;
    action_type: string; // "ban" or "warn"
  };

  let banLogs = $state<BanLogEntry[]>([]);
  let searchQuery = $state('');
  let unlisten = null;
  let containerElement;

  async function loadBanLogs() {
    const startTime = performance.now();
    const isSearch = searchQuery.trim();
    console.log(`[PERF] world-moderation loadBanLogs() START (search: ${isSearch})`);
    try {
      if (isSearch) {
        const dbStartTime = performance.now();
        const results = await invoke<BanLogEntry[]>('search_ban_log_entries', { query: searchQuery });
        const dbDuration = performance.now() - dbStartTime;
        console.log(`[PERF] world-moderation search_ban_log_entries DB call: ${dbDuration.toFixed(2)}ms`);
        banLogs = results || [];
      } else {
        const dbStartTime = performance.now();
        const results = await invoke<BanLogEntry[]>('get_all_ban_log_entries');
        const dbDuration = performance.now() - dbStartTime;
        console.log(`[PERF] world-moderation get_all_ban_log_entries DB call: ${dbDuration.toFixed(2)}ms`);
        banLogs = results || [];
      }
      const totalDuration = performance.now() - startTime;
      console.log(`[PERF] world-moderation loadBanLogs() END: ${totalDuration.toFixed(2)}ms (${banLogs.length} entries)`);
    } catch (err) {
      console.error('Failed to load ban logs:', err);
      banLogs = [];
    }
  }

  // Debounced search
  let searchTimer = null;
  function onSearchInput() {
    if (searchTimer) {
      clearTimeout(searchTimer);
    }
    searchTimer = setTimeout(() => {
      loadBanLogs();
    }, 300);
  }

  // Check if the tab is currently visible
  function isTabVisible() {
    if (!containerElement) return false;
    const parent = containerElement.closest('.tab');
    if (!parent) return false;
    return parent.getAttribute('aria-hidden') === null;
  }

  let wasVisible = false;
  let observer = null;

  function checkVisibility() {
    const isVisible = isTabVisible();
    if (isVisible && !wasVisible) {
      loadBanLogs();
    }
    wasVisible = isVisible;
  }

  onMount(async () => {
    const mountStartTime = performance.now();
    console.log('[PERF] world-moderation onMount START');
    // Initial load
    await loadBanLogs();

    // Listen for new ban events
    try {
      const listenerStartTime = performance.now();
      unlisten = await listen('ban_event', async () => {
        // Reload logs when a new ban event is detected
        if (isTabVisible()) {
          console.log('[PERF] world-moderation ban_event received, reloading logs');
          await loadBanLogs();
        }
      });
      const listenerDuration = performance.now() - listenerStartTime;
      console.log(`[PERF] world-moderation event listener setup: ${listenerDuration.toFixed(2)}ms`);
    } catch (err) {
      console.error('Failed to set up ban event listener:', err);
    }

    // Set up MutationObserver to watch for tab visibility changes
    if (containerElement) {
      const parent = containerElement.closest('.tab');
      if (parent) {
        wasVisible = isTabVisible();

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
    console.log(`[PERF] world-moderation onMount END: ${mountDuration.toFixed(2)}ms`);
  });

  onDestroy(() => {
    if (unlisten) {
      unlisten();
    }
    if (observer) {
      observer.disconnect();
    }
  });
</script>

<div class="panel" bind:this={containerElement}>
  <div class="header">
    <h2>Moderation Logs - World Staff (Not Group Moderation)</h2>
    <div class="search-container">
      <input
        type="text"
        placeholder="Search by admin or target..."
        bind:value={searchQuery}
        oninput={onSearchInput}
        class="search-input"
      />
    </div>
  </div>

  <div class="logs-container">
    {#if banLogs.length === 0}
      <div class="empty">
        {#if searchQuery.trim()}
          No moderation logs found matching "{searchQuery}"
        {:else}
          No moderation logs yet. Ban/warn events will appear here when detected in VRChat logs.
        {/if}
      </div>
    {:else}
      <div class="logs-list">
        {#each banLogs as entry (entry.id)}
          <div class="log-entry" class:ban={entry.action_type === 'ban'} class:warn={entry.action_type === 'warn'}>
            <div class="action-badge" class:ban={entry.action_type === 'ban'} class:warn={entry.action_type === 'warn'}>
              {entry.action_type === 'ban' ? 'Ban' : 'Warn'}
            </div>
            <div class="log-header">
              <div class="log-meta">
                <span class="admin">Admin: <strong>{entry.admin}</strong></span>
                <span class="target">Target: <strong>{entry.target}</strong></span>
                <span class="timestamp">{entry.timestamp}</span>
              </div>
            </div>
            <div class="log-reason">
              <span class="reason-label">Reason:</span>
              <span class="reason-text">{entry.reason}</span>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 16px;
    height: 100%;
  }

  .header {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  h2 {
    margin: 0;
    font-size: 18px;
    font-weight: 600;
    color: var(--fg);
  }

  .search-container {
    display: flex;
    gap: 8px;
  }

  .search-input {
    flex: 1;
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--fg);
    border-radius: 8px;
    padding: 8px 12px;
    font-size: 13px;
  }

  .search-input:focus {
    outline: none;
    border-color: var(--accent);
  }

  .logs-container {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--bg-elev);
    padding: 12px;
  }

  .empty {
    padding: 32px;
    text-align: center;
    color: var(--fg-muted);
    font-size: 14px;
  }

  .logs-list {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .log-entry {
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 0;
    background: var(--bg);
    display: flex;
    flex-direction: column;
    gap: 0;
    position: relative;
    overflow: hidden;
  }

  .log-entry.ban {
    border-color: rgba(255, 80, 80, 0.5);
    box-shadow: 0 0 12px rgba(255, 80, 80, 0.3), inset 0 0 20px rgba(255, 80, 80, 0.1);
  }

  .log-entry.warn {
    border-color: rgba(255, 180, 0, 0.5);
    box-shadow: 0 0 12px rgba(255, 180, 0, 0.3), inset 0 0 20px rgba(255, 180, 0, 0.1);
  }

  .action-badge {
    padding: 6px 12px;
    font-size: 12px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    text-align: center;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }

  .action-badge.ban {
    background: rgba(255, 80, 80, 0.2);
    color: #ff5050;
    text-shadow: 0 0 8px rgba(255, 80, 80, 0.8);
  }

  .action-badge.warn {
    background: rgba(255, 180, 0, 0.2);
    color: #ffb400;
    text-shadow: 0 0 8px rgba(255, 180, 0, 0.8);
  }

  .log-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px;
  }

  .log-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    align-items: center;
    font-size: 13px;
  }

  .admin,
  .target {
    color: var(--fg-muted);
  }

  .admin strong,
  .target strong {
    color: var(--fg);
    font-weight: 600;
  }

  .timestamp {
    color: var(--fg-muted);
    font-size: 12px;
    margin-left: auto;
  }

  .log-reason {
    display: flex;
    gap: 8px;
    padding: 12px;
    padding-top: 8px;
    border-top: 1px solid var(--border);
  }

  .reason-label {
    color: var(--fg-muted);
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
  }

  .reason-text {
    color: var(--fg);
    font-size: 13px;
    flex: 1;
  }
</style>
