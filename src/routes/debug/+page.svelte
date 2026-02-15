<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { derived, writable, get } from 'svelte/store';
  import { debugLog, pushDebug, clearDebugLog, type DebugEntry } from '$lib/stores/debugLog';
  import { listen } from '@tauri-apps/api/event';

  const entries = derived(debugLog, ($debugLog) => $debugLog.slice().reverse() as DebugEntry[]);
  const filterText = writable('');
  const filterLevel = writable('all' as 'all' | 'log' | 'debug' | 'info' | 'warn' | 'error');
  const filterSource = writable('all' as 'all' | 'frontend' | 'backend');
  const autoScroll = writable(true);
  let logContainer;
  let unlistenDebug = null;

  const filters = derived(
    [filterText, filterLevel, filterSource, entries],
    ([$filterText, $filterLevel, $filterSource, $entries]) => {
      const text = $filterText.trim().toLowerCase();
      return $entries.filter((entry) => {
        if ($filterLevel !== 'all' && entry.level !== $filterLevel) return false;
        if ($filterSource !== 'all' && entry.source !== $filterSource) return false;
        if (text && !entry.message.toLowerCase().includes(text)) return false;
        return true;
      });
    }
  );

  function resetFilters() {
    filterText.set('');
    filterLevel.set('all');
    filterSource.set('all');
  }

  function scrollToBottom() {
    const container = logContainer as HTMLDivElement;
    if (!container) return;
    const shouldScroll = get(autoScroll);
    if (shouldScroll) {
      requestAnimationFrame(() => {
        container.scrollTop = container.scrollHeight;
      });
    }
  }

  // Auto-scroll when new entries are added
  let unsubscribeFilters = null;
  let unsubscribeAutoScroll = null;

  onMount(async () => {
    pushDebug('[Debug] Debug panel initialized', undefined, 'info', 'frontend');
    
    // Listen for backend debug events if available
    try {
      unlistenDebug = await listen('debug_log', (e) => {
        const eventData = e as any;
        const { message, ts } = eventData?.payload || {};
        if (typeof message === 'string') {
          pushDebug(message, typeof ts === 'string' ? ts : undefined, 'log', 'backend');
        }
      });
      pushDebug('[Debug] Backend debug listener attached', undefined, 'info', 'frontend');
    } catch (err) {
      pushDebug(`[Debug] Backend debug listener not available: ${err}`, undefined, 'warn', 'frontend');
    }

    // Auto-scroll when filters change
    unsubscribeFilters = filters.subscribe(() => {
      if (get(autoScroll)) {
        scrollToBottom();
      }
    });

    // Auto-scroll when autoScroll setting changes
    unsubscribeAutoScroll = autoScroll.subscribe(value => {
      if (value) {
        scrollToBottom();
      }
    });
  });

  onDestroy(() => {
    if (unlistenDebug) {
      unlistenDebug();
    }
    if (unsubscribeFilters) {
      unsubscribeFilters();
    }
    if (unsubscribeAutoScroll) {
      unsubscribeAutoScroll();
    }
  });
</script>

<div class="debug-panel">
  <div class="header">
    <h2>Debug Log</h2>
    <p class="hint">Session logs in reverse chronological order. Logs persist until cleared or app restart.</p>
  </div>

  <div class="controls">
    <div class="filter-row">
      <label for="filter-text">Search</label>
      <input 
        id="filter-text" 
        type="text" 
        placeholder="Filter by message content..." 
        bind:value={$filterText} 
      />
      <select bind:value={$filterLevel}>
        <option value="all">All Levels</option>
        <option value="log">Log</option>
        <option value="debug">Debug</option>
        <option value="info">Info</option>
        <option value="warn">Warn</option>
        <option value="error">Error</option>
      </select>
      <select bind:value={$filterSource}>
        <option value="all">All Sources</option>
        <option value="frontend">Frontend</option>
        <option value="backend">Backend</option>
      </select>
      <label class="checkbox-label">
        <input type="checkbox" bind:checked={$autoScroll} />
        Auto-scroll
      </label>
      <button class="ghost" onclick={resetFilters}>Reset Filters</button>
      <button class="ghost" onclick={clearDebugLog}>Clear Log</button>
    </div>
    <div class="stats">
      <span>Total: {$entries.length}</span>
      <span>Filtered: {$filters.length}</span>
    </div>
  </div>

  <div 
    class="log" 
    role="log" 
    aria-live="polite"
    bind:this={logContainer}
  >
    {#if $filters.length === 0}
      <div class="empty">No debug entries match the current filters.</div>
    {:else}
      <ul>
        {#each $filters as entry, i (entry.ts + entry.message + entry.level + entry.source + i)}
          <li class="entry entry-{entry.level} entry-{entry.source}">
            <span class="timestamp">{new Date(entry.ts).toLocaleTimeString()}</span>
            <span class="level-badge level-{entry.level}">{entry.level}</span>
            <span class="source-badge source-{entry.source}">{entry.source}</span>
            <span class="message">{entry.message}</span>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</div>

<style>
  .debug-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    gap: 16px;
  }

  .header {
    flex-shrink: 0;
  }

  h2 {
    margin: 0 0 4px 0;
    font-size: 20px;
    font-weight: 600;
  }

  .hint {
    margin: 0;
    color: var(--fg-muted);
    font-size: 13px;
  }

  .controls {
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 12px;
    background: var(--bg-elev);
    border: 1px solid var(--border);
    border-radius: 8px;
  }

  .filter-row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  label {
    font-size: 13px;
    color: var(--fg-muted);
    white-space: nowrap;
  }

  input[type="text"] {
    flex: 1;
    min-width: 200px;
    height: 32px;
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg);
    color: var(--fg);
    font-size: 13px;
  }

  input[type="text"]:focus {
    outline: none;
    border-color: var(--accent);
    box-shadow: 0 0 0 2px rgba(255, 20, 147, 0.1);
  }

  select {
    height: 32px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg);
    color: var(--fg);
    font-size: 13px;
    cursor: pointer;
  }

  select:focus {
    outline: none;
    border-color: var(--accent);
  }

  .checkbox-label {
    display: flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
    font-size: 13px;
  }

  .checkbox-label input[type="checkbox"] {
    margin: 0;
    cursor: pointer;
  }

  button.ghost {
    height: 32px;
    padding: 0 12px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg);
    color: var(--fg);
    font-size: 13px;
    cursor: pointer;
    transition: all 0.2s;
  }

  button.ghost:hover {
    background: var(--bg-hover);
    border-color: var(--accent);
    box-shadow: 0 0 0 2px rgba(255, 20, 147, 0.1);
  }

  .stats {
    display: flex;
    gap: 16px;
    font-size: 12px;
    color: var(--fg-muted);
  }

  .log {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 8px;
    font-family: 'Courier New', 'Consolas', monospace;
    font-size: 12px;
    line-height: 1.5;
  }

  .empty {
    padding: 24px;
    text-align: center;
    color: var(--fg-muted);
    font-style: italic;
  }

  ul {
    list-style: none;
    padding: 0;
    margin: 0;
  }

  .entry {
    display: grid;
    grid-template-columns: auto auto auto 1fr;
    gap: 8px;
    padding: 4px 8px;
    margin-bottom: 2px;
    border-radius: 4px;
    transition: background 0.1s;
    align-items: start;
  }

  .entry:hover {
    background: var(--bg-elev);
  }

  .entry-error {
    border-left: 3px solid #ff6b6b;
  }

  .entry-warn {
    border-left: 3px solid #ffa94d;
  }

  .entry-info {
    border-left: 3px solid #4dabf7;
  }

  .entry-debug {
    border-left: 3px solid #868e96;
  }

  .timestamp {
    color: var(--fg-muted);
    font-size: 11px;
    white-space: nowrap;
    min-width: 80px;
  }

  .level-badge {
    padding: 2px 6px;
    border-radius: 3px;
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    white-space: nowrap;
    min-width: 50px;
    text-align: center;
  }

  .level-log {
    background: rgba(134, 142, 150, 0.2);
    color: #868e96;
  }

  .level-debug {
    background: rgba(134, 142, 150, 0.2);
    color: #868e96;
  }

  .level-info {
    background: rgba(77, 171, 247, 0.2);
    color: #4dabf7;
  }

  .level-warn {
    background: rgba(255, 169, 77, 0.2);
    color: #ffa94d;
  }

  .level-error {
    background: rgba(255, 107, 107, 0.2);
    color: #ff6b6b;
  }

  .source-badge {
    padding: 2px 6px;
    border-radius: 3px;
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    white-space: nowrap;
    min-width: 70px;
    text-align: center;
  }

  .source-frontend {
    background: rgba(255, 20, 147, 0.2);
    color: #ff1493;
  }

  .source-backend {
    background: rgba(138, 43, 226, 0.2);
    color: #8a2be2;
  }

  .message {
    color: var(--fg);
    word-break: break-word;
    white-space: pre-wrap;
  }

  /* Scrollbar styling */
  .log::-webkit-scrollbar {
    width: 8px;
  }

  .log::-webkit-scrollbar-track {
    background: var(--bg-elev);
    border-radius: 4px;
  }

  .log::-webkit-scrollbar-thumb {
    background: #ff1493;
    border-radius: 4px;
    border: 1px solid rgba(255, 20, 147, 0.3);
  }

  .log::-webkit-scrollbar-thumb:hover {
    background: #ff69b4;
  }
</style>
