<script lang="ts">
  import {
    debugLog,
    type DebugEntry,
    clearDebugLog,
    avatarSwitchCount,
    apiCallSentCount,
    apiCallReturnCount,
    apiQueueLength,
    liveViewCounts
  } from '$lib/stores/debugLog';
  import { derived } from 'svelte/store';
  import { writable } from 'svelte/store';
  import { invoke } from '@tauri-apps/api/core';

  type AvatarDetail = {
    avatarName: string;
    ownerId: string;
    fileId: string;
    version: number;
    file: unknown;
    security: unknown;
    updatedAt: string;
  };

  type AvatarLog = {
    timestamp: string;
    username: string;
    avatarName: string;
  };

let showModal = $state<null | { type: 'details' | 'logs'; items: AvatarDetail[] | AvatarLog[] }>(null);
  let modalTitle = $state('');
  let loading = $state(false);
  let errorMsg = $state<string | null>(null);
let modalRequestId = 0;

  async function openModal(type: 'details' | 'logs') {
  const requestId = ++modalRequestId;
  loading = true;
  errorMsg = null;
  modalTitle = type === 'details' ? 'Recent Avatar Details' : 'Recent Avatar Logs';
  showModal = { type, items: [] };
    try {
      if (type === 'details') {
      const res: AvatarDetail[] = await invoke('list_recent_avatar_details', { limit: 10 });
      if (requestId === modalRequestId) {
        showModal = { type, items: Array.isArray(res) ? res : [] };
      }
      } else {
      const res: AvatarLog[] = await invoke('list_recent_avatar_logs', { limit: 10 });
      if (requestId === modalRequestId) {
        showModal = { type, items: Array.isArray(res) ? res : [] };
      }
      }
    } catch (err: any) {
    if (requestId === modalRequestId) {
      errorMsg = err?.message ?? String(err ?? 'Unknown error');
    }
    } finally {
    if (requestId === modalRequestId) {
      loading = false;
    }
    }
  }

  function closeModal() {
  modalRequestId++;
  showModal = null;
  errorMsg = null;
  loading = false;
  }

  const entries = derived(debugLog, ($debugLog): DebugEntry[] => $debugLog.slice().reverse());
  const filterText = writable('');
  const includeGroup = writable(true);
  const includeSound = writable(true);
  const includeJoin = writable(true);
  const includeVrcapi = writable(true);
  const includeAvatar = writable(false);
  const includePerf = writable(false);
  const includeWatcher = writable(true);
  const includeOther = writable(true);

  const filters = derived(
    [
      filterText,
      includeGroup,
      includeSound,
      includeJoin,
      includeVrcapi,
      includeAvatar,
      includePerf,
      includeWatcher,
      includeOther,
      entries
    ],
    ([
      $filterText,
      $includeGroup,
      $includeSound,
      $includeJoin,
      $includeVrcapi,
      $includeAvatar,
      $includePerf,
      $includeWatcher,
      $includeOther,
      $entries
    ]) => {
      const text = $filterText.trim().toLowerCase();
      return $entries.filter((entry) => {
        const message = entry.message.toLowerCase();
        if (text && !message.includes(text)) return false;
        const tag = classify(entry.message);
        if (tag === 'group' && !$includeGroup) return false;
        if (tag === 'sound' && !$includeSound) return false;
        if (tag === 'join' && !$includeJoin) return false;
        if (tag === 'vrcapi' && !$includeVrcapi) return false;
        if (tag === 'avatar' && !$includeAvatar) return false;
        if (tag === 'perf' && !$includePerf) return false;
        if (tag === 'watcher' && !$includeWatcher) return false;
        if (tag === 'other' && !$includeOther) return false;
        return true;
      });
    }
  );

  function classify(message: string): 'group' | 'sound' | 'join' | 'vrcapi' | 'avatar' | 'perf' | 'watcher' | 'other' {
    const msg = message.toLowerCase();
    if (msg.includes('[group-watch]')) return 'group';
    if (msg.includes('[sound]')) return 'sound';
    if (msg.includes('join inserted') || msg.includes('join ignored')) return 'join';
    if (msg.includes('[avatardata]')) return 'avatar';
    if (msg.includes('[avatarperf]')) return 'perf';
    if (msg.includes('[vrcapi]')) return 'vrcapi';
    if (msg.includes('[watcher]')) return 'watcher';
    return 'other';
  }

  function resetFilters() {
    filterText.set('');
    includeGroup.set(true);
    includeSound.set(true);
    includeJoin.set(true);
    includeVrcapi.set(true);
    includeAvatar.set(false);
    includeWatcher.set(true);
    includeOther.set(true);
  }
</script>

<div class="debug-panel">
  <h2>Debug Stream</h2>
  <p class="hint">Latest 100 events in reverse chronological order. Session only.</p>

  <div class="controls">
    <div class="filter-row">
      <label for="filter-text">Contains</label>
      <input id="filter-text" placeholder="Search debug text" bind:value={$filterText} />
      <button class="ghost" onclick={resetFilters}>Reset</button>
      <button class="ghost" onclick={clearDebugLog}>Clear Log</button>
    </div>
    <div class="channels">
      <label><input type="checkbox" bind:checked={$includeGroup} /> Group</label>
      <label><input type="checkbox" bind:checked={$includeSound} /> Sound</label>
      <label><input type="checkbox" bind:checked={$includeJoin} /> Join</label>
      <label><input type="checkbox" bind:checked={$includeVrcapi} /> VRCAPI</label>
      <label><input type="checkbox" bind:checked={$includeAvatar} /> AvatarData</label>
      <label><input type="checkbox" bind:checked={$includePerf} /> AvatarPerf</label>
      <label><input type="checkbox" bind:checked={$includeWatcher} /> Watcher</label>
      <label><input type="checkbox" bind:checked={$includeOther} /> Other</label>
    </div>
    <div class="counters">
      <div class="counter">Avatar Switch Events: {$avatarSwitchCount}</div>
      <div class="counter">Avatar API Calls Sent: {$apiCallSentCount}</div>
      <div class="counter">Avatar API Calls Returned: {$apiCallReturnCount}</div>
      <div class="counter">Current API Queue: {$apiQueueLength}</div>
      <div class="counter">Live View Avatars: {$liveViewCounts.loaded}/{$liveViewCounts.total}</div>
    </div>
  </div>

  <div class="log" role="log">
    {#if $filters.length === 0}
      <div class="empty">No debug events for the current filter.</div>
    {:else}
      <ul>
        {#each $filters as entry (entry.ts + entry.message)}
          <li>
            <span class="ts">{new Date(entry.ts).toLocaleTimeString()}</span>
            <span class="msg">{entry.message}</span>
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  {#if showModal}
    <div class="modal-backdrop" role="presentation" onclick={closeModal}>
      <div class="modal" role="dialog" aria-modal="true" onclick={(e) => e.stopPropagation()}>
        <header>
          <h3>{modalTitle}</h3>
          <button class="ghost" onclick={closeModal}>Close</button>
        </header>
        <div class="body">
          {#if loading}
            <div class="empty">Loading…</div>
          {:else if errorMsg}
            <div class="empty error">{errorMsg}</div>
          {:else if showModal.items.length === 0}
            <div class="empty">No records.</div>
          {:else if showModal.type === 'details'}
            <ul class="results">
              {#each showModal.items as item (item.avatarName + item.ownerId + item.updatedAt)}
                <li>
                  <div><strong>Avatar:</strong> {item.avatarName}</div>
                  <div><strong>Owner:</strong> {item.ownerId || 'Unknown'}</div>
                  <div><strong>Version:</strong> {item.version}</div>
                  <div><strong>File ID:</strong> {item.fileId || '—'}</div>
                  <div><strong>Updated:</strong> {item.updatedAt}</div>
                  {#if item.file && Object.keys(item.file as Record<string, unknown>).length}
                    <details>
                      <summary>File JSON</summary>
                      <pre>{JSON.stringify(item.file, null, 2)}</pre>
                    </details>
                  {/if}
                  {#if item.security && Object.keys(item.security as Record<string, unknown>).length}
                    <details>
                      <summary>Security JSON</summary>
                      <pre>{JSON.stringify(item.security, null, 2)}</pre>
                    </details>
                  {/if}
                </li>
              {/each}
            </ul>
          {:else}
            <ul class="results">
              {#each showModal.items as item (item.timestamp + item.username + item.avatarName)}
                <li>
                  <div><strong>When:</strong> {item.timestamp}</div>
                  <div><strong>User:</strong> {item.username}</div>
                  <div><strong>Avatar:</strong> {item.avatarName}</div>
                </li>
              {/each}
            </ul>
          {/if}
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .debug-panel {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  h2 {
    margin: 0;
    font-size: 18px;
  }

  .hint {
    margin: 0;
    color: var(--fg-muted);
    font-size: 12px;
  }

  .controls {
    display: grid;
    gap: 8px;
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 10px;
    background: rgba(0,0,0,0.12);
  }

  .modal-actions {
    display: none;
  }

  .counters {
    display: grid;
    gap: 6px;
    align-items: center;
    grid-auto-flow: row;
  }

  .counter {
    font-weight: 600;
    color: var(--fg);
  }

  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0,0,0,0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }

  .modal {
    width: min(640px, 90vw);
    background: var(--bg-elev);
    color: var(--fg);
    border: 1px solid var(--border);
    border-radius: 12px;
    box-shadow: 0 20px 50px rgba(0,0,0,0.4);
    display: grid;
    grid-template-rows: auto 1fr;
  }

  .modal header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
  }

  .modal header h3 {
    margin: 0;
    font-size: 16px;
  }

  .modal .body {
    padding: 16px;
    max-height: 60vh;
    overflow: auto;
    display: grid;
    gap: 12px;
  }

  .results {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 12px;
  }

  .results li {
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 12px;
    background: rgba(0,0,0,0.1);
    display: grid;
    gap: 6px;
  }

  .results details {
    background: rgba(255,255,255,0.04);
    border-radius: 6px;
    padding: 6px 8px;
  }

  .results pre {
    margin: 6px 0 0 0;
    padding: 8px;
    background: rgba(0,0,0,0.3);
    border-radius: 6px;
    font-size: 12px;
    max-height: 240px;
    overflow: auto;
  }

  .empty.error {
    color: var(--fg-err);
  }

  .filter-row {
    display: grid;
    grid-template-columns: auto 1fr auto auto;
    gap: 8px;
    align-items: center;
  }

  .filter-row label {
    color: var(--fg-muted);
    font-size: 12px;
  }

  .filter-row input {
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--fg);
    border-radius: 8px;
    padding: 6px 10px;
  }

  .channels {
    display: flex;
    gap: 12px;
    flex-wrap: wrap;
    font-size: 12px;
    color: var(--fg-muted);
  }

  .channels label {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .ghost {
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--fg);
    border-radius: 8px;
    padding: 6px 12px;
    cursor: pointer;
  }

  .log {
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--bg-elev);
    padding: 12px;
    min-height: 160px;
    max-height: calc(100vh - 260px);
    overflow: auto;
  }

  .empty {
    color: var(--fg-muted);
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 8px;
  }

  li {
    display: flex;
    gap: 8px;
    font-size: 13px;
    line-height: 1.4;
  }

  .ts {
    color: var(--fg-muted);
    min-width: 80px;
    font-family: 'Consolas', 'SFMono-Regular', 'Menlo', monospace;
  }

  .msg {
    color: var(--fg);
    flex: 1;
  }
</style>

