<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  let content = $state<string>('');
  let query = $state<string>('');
  let lines = $state<string[]>([]);
  let matchLines = $state<number[]>([]);
  let matchLinesSet = $state(new Set<number>());
  let current = $state(0);
  let loading = $state(false);
  let loadedOnce = $state(false);
  let isSearching = $state(false);
  let progress = $state(0); // 0..100
  let searchToken = 0;
  const CHUNK = 256 * 1024; // 256KB

  async function load() {
    if (loading) return;
    loading = true; content = '';
    try {
      const info = await invoke<any>('read_log_info');
      let offset = 0;
      const chunks: string[] = [];
      while (true) {
        const res = await invoke<any>('read_log_chunk', { offset, maxBytes: CHUNK });
        if (res.data) chunks.push(res.data);
        offset = res.offset || offset;
        if (res.eof) break;
      }
      content = chunks.join('');
      lines = content.split('\n');
    } catch {
      content = '';
      lines = [];
    } finally {
      loading = false; loadedOnce = true; compute();
    }
  }
  let computeTimer: any;
  function compute() {
    clearTimeout(computeTimer);
    computeTimer = setTimeout(startSearch, 500); // Debounce search
  }

  async function startSearch() {
    const myToken = ++searchToken;
    matchLines = []; current = 0;
    progress = 0;
    
    if (!query) {
      isSearching = false;
      return;
    }
    isSearching = true;

    try {
      const result: number[] = await invoke('search_log_file', { query, searchToken: myToken });
      // If the token has changed, it means a new search has started, so we discard these results
      if (myToken === searchToken) {
        matchLines = result;
        matchLinesSet = new Set(result);
        if (matchLines.length > 0) {
          current = 0;
          scrollToCurrent();
        }
      }
    } catch (e) {
      console.error("Search failed or was cancelled", e);
    } finally {
      if (myToken === searchToken) {
        isSearching = false;
      }
    }
  }
  function jump(delta: number) {
    if (matchLines.length === 0) return;
    current = (current + delta + matchLines.length) % matchLines.length;
    scrollToCurrent();
  }
  function scrollToCurrent() {
    if (matchLines.length === 0) return;
    const lineIdx = matchLines[current];
    const pre = document.getElementById('logpre');
    const el = document.getElementById('line-' + lineIdx);
    if (pre && el) {
      const top = el.offsetTop - pre.clientHeight / 2;
      pre.scrollTo({ top: Math.max(0, top), behavior: 'smooth' });
    }
  }

  onMount(() => {
    load();
    let unlistenProgress: (() => void) | undefined;
    let unlistenCancel: (() => void) | undefined;

    (async () => {
      const { listen } = await import('@tauri-apps/api/event');
      unlistenProgress = await listen('search_progress', (e: any) => {
        if (e.payload.token === searchToken) {
          progress = e.payload.progress;
        }
      });
      unlistenCancel = await listen('cancel_search', () => {});
    })();

    return () => {
      unlistenProgress?.();
      unlistenCancel?.();
    };
  });
</script>

<div class="panel">
  <div class="tools">
    <button onclick={load} disabled={loading}>{loadedOnce ? 'Reload' : 'Load'}</button>
    <input
      placeholder="Search..."
      bind:value={query}
      oninput={compute}
      style={`background-image: linear-gradient(to right, rgba(255,182,193,0.35) ${isSearching ? progress : 0}%, rgba(0,0,0,0) ${isSearching ? progress : 0}%); background-repeat: no-repeat; background-size: 100% 100%;`}
    />
    <div class="count">{matchLines.length} match{matchLines.length===1?'':'es'}</div>
    <button onclick={() => jump(-1)} disabled={matchLines.length===0}>Prev</button>
    <button onclick={() => jump(1)} disabled={matchLines.length===0}>Next</button>
  </div>
  <div id="logpre">
    {#if loading}
      Loading…
    {:else}
      {#each lines as line, i}
        <div id={`line-${i}`} class:match={matchLinesSet.has(i)}>{line}</div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .panel { display: flex; flex-direction: column; gap: 8px; }
  .tools { display: flex; gap: 8px; align-items: center; }
  input { flex: 1; min-width: 120px; border-radius: 8px; border: 1px solid var(--border); background: var(--bg); color: var(--fg); padding: 6px 10px; }
  button { border: 1px solid var(--border); background: var(--bg-elev); color: var(--fg); border-radius: 8px; padding: 6px 10px; cursor: pointer; }
  #logpre { border: 1px solid var(--border); border-radius: 12px; background: var(--bg-elev); color: var(--fg); padding: 12px; max-height: calc(100vh - 220px); overflow: auto; white-space: pre-wrap; word-break: break-word; font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; }
  #logpre > div { padding: 1px 4px; border-radius: 6px; }
  #logpre > div.match { background: rgba(255, 182, 193, 0.18); outline: 1px solid #ffb6c1; }
</style>

