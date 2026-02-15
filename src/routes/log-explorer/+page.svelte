<script lang="ts">
  import { onMount, onDestroy, tick } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';

  let logLines = $state([]);
  let unlisten = null;
  let textareaElement;
  let autoScroll = $state(true);
  let searchQuery = $state('');
  let matchIndices = $state([]);
  let currentMatchIndex = $state(-1);
  let isSearching = $state(false);
  let searchProgress = $state(0);
  let searchTimer = null;
  let mostRecentLogFile = $state(null);
  let currentLogFileName = $state(null); // Track current log file to avoid redundant updates

  // Auto-scroll to bottom when new log lines are added (if enabled)
  $effect(() => {
    if (autoScroll && textareaElement && logLines.length > 0 && currentMatchIndex === -1 && !searchQuery.trim()) {
      // Only auto-scroll if not navigating search results and no search is active
      setTimeout(() => {
        if (textareaElement) {
          textareaElement.scrollTop = textareaElement.scrollHeight;
        }
      }, 0);
    }
  });

  // Search function with batch processing
  function performSearch() {
    if (!searchQuery.trim()) {
      matchIndices = [];
      currentMatchIndex = -1;
      isSearching = false;
      searchProgress = 0;
      return;
    }

    // Disable auto-scroll when searching
    autoScroll = false;

    isSearching = true;
    searchProgress = 0;
    matchIndices = [];
    currentMatchIndex = -1;

    const query = searchQuery.toLowerCase();
    const batchSize = 100;
    let processed = 0;

    function processBatch() {
      const start = processed;
      const end = Math.min(processed + batchSize, logLines.length);
      
      for (let i = start; i < end; i++) {
        if (logLines[i].toLowerCase().includes(query)) {
          matchIndices.push(i);
        }
      }

      processed = end;
      searchProgress = (processed / logLines.length) * 100;

      if (processed < logLines.length) {
        // Process next batch asynchronously
        setTimeout(processBatch, 0);
      } else {
        // Search complete
        isSearching = false;
        if (matchIndices.length > 0) {
          currentMatchIndex = 0;
          scrollToMatch();
        }
      }
    }

    processBatch();
  }

  // Debounced search
  function onSearchInput() {
    if (searchTimer) {
      clearTimeout(searchTimer);
    }
    searchTimer = setTimeout(() => {
      performSearch();
    }, 500);
  }

  // Get filtered lines to display - only show matches when searching
  // Use $derived for reactivity
  let displayedLines = $derived.by(() => {
    if (!searchQuery.trim() || matchIndices.length === 0) {
      // No search or no matches - show all lines
      return logLines.map((line, i) => ({ line, index: i }));
    }
    // Search active - only show matching lines
    return matchIndices.map(i => ({ line: logLines[i], index: i }));
  });

  function jumpToMatch(delta) {
    if (matchIndices.length === 0) return;
    currentMatchIndex = (currentMatchIndex + delta + matchIndices.length) % matchIndices.length;
    scrollToMatch();
  }

  async function scrollToMatch() {
    if (currentMatchIndex === -1 || matchIndices.length === 0 || !textareaElement) return;
    
    // Wait for DOM to update with the new current match highlighting
    await tick();
    
    const lineIndex = matchIndices[currentMatchIndex];
    const lineElement = document.getElementById(`line-${lineIndex}`);
    
    if (lineElement && textareaElement) {
      // Calculate the position to scroll to (center the element in the container)
      const elementOffsetTop = lineElement.offsetTop;
      const containerHeight = textareaElement.clientHeight;
      const elementHeight = lineElement.offsetHeight;
      
      const targetScrollTop = elementOffsetTop - (containerHeight / 2) + (elementHeight / 2);
      
      // Smooth scroll to the target position
      textareaElement.scrollTo({
        top: targetScrollTop,
        behavior: 'smooth'
      });
    }
  }

  async function updateMostRecentLogFile() {
    const startTime = performance.now();
    try {
      const filePath = await invoke('get_most_recent_log_file');
      const duration = performance.now() - startTime;
      // Only log if the file actually changed
      if (filePath !== mostRecentLogFile) {
        console.log(`[PERF] updateMostRecentLogFile: ${duration.toFixed(2)}ms - ${filePath}`);
      if (filePath) {
        mostRecentLogFile = filePath;
      } else {
        mostRecentLogFile = null;
        }
      }
    } catch (err) {
      console.error('Failed to get most recent log file:', err);
      mostRecentLogFile = null;
    }
  }

  async function openMostRecentLogFile() {
    if (!mostRecentLogFile) return;
    try {
      console.log('Attempting to open most recent log file...');
      const result = await invoke('open_most_recent_log_file');
      console.log('Open result:', result);
    } catch (err) {
      console.error('Failed to open log file:', err);
      alert(`Failed to open log file: ${err}`);
    }
  }

  async function openMostRecentLogFolder() {
    try {
      const result = await invoke('open_most_recent_log_folder');
      console.log('Open folder result:', result);
    } catch (err) {
      console.error('Failed to open log folder:', err);
      alert(`Failed to open log folder: ${err}`);
    }
  }

  onMount(async () => {
    const mountStartTime = performance.now();
    console.log('[PERF] log-explorer onMount START');
    // Start log reader automatically
    try {
      const readerStartTime = performance.now();
      await invoke('start_log_reader');
      const readerDuration = performance.now() - readerStartTime;
      console.log(`[PERF] log-explorer start_log_reader: ${readerDuration.toFixed(2)}ms`);
    } catch (err) {
      console.error('Failed to start log reader:', err);
    }
    
    // If loading screen is still showing, don't dismiss it here
    // Let instance monitor handle the dismissal after backfill completes

    // Listen for log line events
    try {
      const listenerStartTime = performance.now();
      unlisten = await listen('log_line', async (event) => {
        const payload = event.payload;
        if (payload && typeof payload === 'object' && 'line' in payload) {
          const p = payload as { line?: string; timestamp?: string; file?: string };
          const logEntry = `[${p.timestamp || ''}] [${p.file || ''}] ${p.line || ''}`;
          
          // Preserve scroll position if auto-scroll is disabled
          let shouldPreserveScroll = !autoScroll && textareaElement;
          let anchorLineId = null;
          let anchorOffset = 0;
          
          if (shouldPreserveScroll && logLines.length > 0) {
            // Find the first visible line element to use as an anchor
            const containerTop = textareaElement.scrollTop;
            
            // Find which line is currently at the top of the viewport
            for (let i = 0; i < logLines.length; i++) {
              const lineEl = document.getElementById(`line-${i}`);
              if (lineEl) {
                const lineTop = lineEl.offsetTop;
                const lineHeight = lineEl.offsetHeight;
                
                // Check if this line is at or near the top of the viewport
                if (lineTop <= containerTop + 20 && lineTop + lineHeight > containerTop) {
                  anchorLineId = `line-${i}`;
                  anchorOffset = containerTop - lineTop;
                  break;
                }
              }
            }
            
            // Fallback: if no line found, use the first line
            if (!anchorLineId && logLines.length > 0) {
              anchorLineId = `line-0`;
              anchorOffset = containerTop;
            }
          }
          
          // Track if we need to adjust match indices (if lines were removed from front)
          const oldLength = logLines.length;
          const hadActiveSearch = searchQuery.trim() && matchIndices.length > 0;
          const currentMatchLineIndex = hadActiveSearch && currentMatchIndex >= 0 
            ? matchIndices[currentMatchIndex] 
            : -1;
          
          logLines = [...logLines, logEntry];
          
          // Keep only last 1000 lines to prevent memory issues
          let linesRemoved = 0;
          if (logLines.length > 1000) {
            const removedCount = logLines.length - 1000;
            logLines = logLines.slice(-1000);
            linesRemoved = removedCount;
            
            // Adjust anchor line index if lines were removed
            if (anchorLineId) {
              const oldIndex = parseInt(anchorLineId.replace('line-', ''));
              const newIndex = Math.max(0, oldIndex - linesRemoved);
              anchorLineId = `line-${newIndex}`;
            }
          }
          
          // Restore scroll position using the anchor line
          if (shouldPreserveScroll && textareaElement && anchorLineId) {
            await tick(); // Wait for Svelte to update DOM
            
            // Use double requestAnimationFrame to ensure layout is complete
            requestAnimationFrame(() => {
              requestAnimationFrame(() => {
                if (textareaElement) {
                  const anchorEl = document.getElementById(anchorLineId);
                  if (anchorEl) {
                    // Scroll the anchor line back to the same position
                    const targetScrollTop = anchorEl.offsetTop + anchorOffset;
                    textareaElement.scrollTop = targetScrollTop;
                  } else {
                    // Fallback: if anchor element not found, try to maintain scroll position
                    // This handles edge cases where the anchor was removed
                    const currentScrollTop = textareaElement.scrollTop;
                    const scrollHeight = textareaElement.scrollHeight;
                    const clientHeight = textareaElement.clientHeight;
                    
                    // If we're near the bottom, don't adjust
                    const distanceFromBottom = scrollHeight - currentScrollTop - clientHeight;
                    if (distanceFromBottom > 50) {
                      // We're scrolled up, try to maintain position
                      textareaElement.scrollTop = currentScrollTop;
                    }
                  }
                }
              });
            });
          }
          
          // Adjust match indices if lines were removed from the front
          if (linesRemoved > 0 && hadActiveSearch) {
            // Adjust all match indices by subtracting removed lines
            matchIndices = matchIndices
              .map(idx => idx - linesRemoved)
              .filter(idx => idx >= 0); // Remove indices that are now out of bounds
            
            // Update current match index if the current match was removed
            if (currentMatchLineIndex >= 0) {
              const newIndex = currentMatchLineIndex - linesRemoved;
              if (newIndex >= 0) {
                // Find the new index in matchIndices
                const newMatchIndex = matchIndices.indexOf(newIndex);
                if (newMatchIndex >= 0) {
                  currentMatchIndex = newMatchIndex;
                } else {
                  // Current match was removed, go to nearest match
                  if (matchIndices.length > 0) {
                    currentMatchIndex = 0;
                  } else {
                    currentMatchIndex = -1;
                  }
                }
              } else {
                // Current match was removed, go to nearest match
                if (matchIndices.length > 0) {
                  currentMatchIndex = 0;
                } else {
                  currentMatchIndex = -1;
                }
              }
            }
          }
          
          // If search is active and new lines were added (not removed), check if new lines match
          if (hadActiveSearch && linesRemoved === 0 && searchQuery.trim()) {
            // Check if the new line matches the search query
            const query = searchQuery.toLowerCase();
            if (logEntry.toLowerCase().includes(query)) {
              // Add the new match index (it's at the end of the array)
              matchIndices = [...matchIndices, logLines.length - 1];
            }
          }
          
          // Update most recent log file only when the file changes
          if (p.file && p.file !== currentLogFileName) {
            currentLogFileName = p.file;
            await updateMostRecentLogFile();
          }
        }
      });
      const listenerDuration = performance.now() - listenerStartTime;
      console.log(`[PERF] log-explorer event listener setup: ${listenerDuration.toFixed(2)}ms`);
    } catch (err) {
      console.error('Failed to set up log listener:', err);
    }
    
    // Initial check for most recent log file
    await updateMostRecentLogFile();
    
    const mountDuration = performance.now() - mountStartTime;
    console.log(`[PERF] log-explorer onMount END: ${mountDuration.toFixed(2)}ms`);
  });

  onDestroy(() => {
    if (unlisten) {
      unlisten();
    }
  });
</script>

<div class="panel">
  <div class="controls">
    <button class="control-btn" onclick={() => logLines = []}>Clear</button>
    <label class="auto-scroll-label">
      <input type="checkbox" bind:checked={autoScroll} />
      <span>Auto-scroll</span>
    </label>
    <button 
      class="control-btn" 
      onclick={openMostRecentLogFile}
      title={mostRecentLogFile ? `Open: ${mostRecentLogFile.split(/[/\\]/).pop()}` : 'No log file available'}
      disabled={!mostRecentLogFile}
    >
      Open Log File
    </button>
    <button
      class="control-btn"
      onclick={openMostRecentLogFolder}
      title="Open VRChat log folder"
    >
      Open Log Folder
    </button>
    <span class="status">Running • {logLines.length} lines</span>
  </div>
  
  <div class="search-bar">
    <input
      type="text"
      class="search-input"
      placeholder="Search..."
      bind:value={searchQuery}
      oninput={onSearchInput}
      style={`background-image: linear-gradient(to right, rgba(255,182,193,0.35) ${isSearching ? searchProgress : 0}%, rgba(0,0,0,0) ${isSearching ? searchProgress : 0}%); background-repeat: no-repeat; background-size: 100% 100%;`}
    />
    {#if matchIndices.length > 0}
      <span class="match-count">{matchIndices.length} match{matchIndices.length === 1 ? '' : 'es'}</span>
      <button class="control-btn" onclick={() => jumpToMatch(-1)} disabled={matchIndices.length === 0}>
        Prev
      </button>
      <button class="control-btn" onclick={() => jumpToMatch(1)} disabled={matchIndices.length === 0}>
        Next
      </button>
    {/if}
  </div>
  
  <div class="log-container">
    <div 
      bind:this={textareaElement}
      class="log-output"
      role="textbox"
      tabindex="0"
    >
      {#if logLines.length === 0}
        <div class="placeholder-text">Waiting for log lines...</div>
      {:else if searchQuery.trim() && matchIndices.length === 0 && !isSearching}
        <div class="placeholder-text">No matches found</div>
      {:else}
        {#each displayedLines as { line, index: i }}
          <div 
            class="log-line"
            class:match={matchIndices.includes(i)}
            class:current={currentMatchIndex !== -1 && matchIndices[currentMatchIndex] === i}
            id="line-{i}"
          >{line}</div>
        {/each}
      {/if}
    </div>
  </div>
</div>

<style>
  .panel { display: flex; flex-direction: column; gap: 12px; height: 100%; }
  
  .controls {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px;
    background: var(--bg-elev);
    border: 1px solid var(--border);
    border-radius: 8px;
  }
  
  .control-btn {
    padding: 6px 12px;
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--fg);
    border-radius: 6px;
    cursor: pointer;
    font-size: 13px;
  }
  
  .control-btn:hover {
    background: var(--bg-hover);
    border-color: var(--accent);
  }

  .control-btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
    background: var(--bg);
    border-color: var(--border);
    color: var(--fg-muted);
  }
  
  .auto-scroll-label {
    display: flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
    font-size: 13px;
    color: var(--fg);
    user-select: none;
  }
  
  .auto-scroll-label input[type="checkbox"] {
    cursor: pointer;
    width: 16px;
    height: 16px;
    accent-color: var(--accent, #ffb6c1);
  }
  
  .status {
    margin-left: auto;
    color: var(--fg-muted);
    font-size: 12px;
  }
  
  .search-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px;
    background: var(--bg-elev);
    border: 1px solid var(--border);
    border-radius: 8px;
  }
  
  .search-input {
    flex: 1;
    min-width: 120px;
    padding: 6px 10px;
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--fg);
    border-radius: 6px;
    font-size: 13px;
  }
  
  .search-input:focus {
    outline: none;
    border-color: var(--accent);
  }
  
  .match-count {
    color: var(--fg-muted);
    font-size: 12px;
    white-space: nowrap;
  }
  
  .log-container {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  
  .log-output {
    flex: 1;
    width: 100%;
    padding: 12px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 8px;
    color: var(--fg);
    font-family: 'Consolas', 'Monaco', 'Courier New', monospace;
    font-size: 12px;
    line-height: 1.5;
    overflow-y: auto;
    white-space: pre-wrap;
    word-break: break-word;
    scroll-behavior: auto;
    /* Prevent browser from automatically adjusting scroll when content changes */
    overflow-anchor: none;
  }
  
  .log-output:focus {
    outline: none;
    border-color: var(--accent);
  }
  
  .placeholder-text {
    color: var(--fg-muted);
    font-style: italic;
  }
  
  .log-line {
    padding: 1px 4px;
    border-radius: 2px;
  }
  
  .log-line.match {
    background: rgba(255, 182, 193, 0.18);
    outline: 1px solid rgba(255, 182, 193, 0.5);
  }
  
  .log-line.match.current {
    background: rgba(255, 182, 193, 0.35);
    outline: 2px solid #ffb6c1;
    font-weight: 600;
  }
</style>
