<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { invoke } from '@tauri-apps/api/core';
  import { get } from 'svelte/store';
  import { isLoading, loadingMessage } from '$lib/stores/loading';

  let activeUsers = $state([]);
  let unlisten = null;
  let currentLogFile = $state(null);
  let watch = $state(new Map());
  let noted = $state(new Map());
  let showNoteFor = $state(null);
  let noteText = $state('');
  let isInitializing = $state(false);
  let hasInitialized = $state(false);
  
  // Event queue for sequential processing (prevents race conditions)
  // Using closure variables (not reactive) since queue is internal implementation detail
  let eventQueue = [];
  let isProcessingQueue = false;
  
  // Batch note lookups to avoid repeated get_all_notes calls
  let pendingNoteLookups = $state(new Set());
  let noteBatchTimer = null;
  const NOTE_BATCH_DELAY = 500; // 500ms delay
  
  // Group watchlist data
  // Map of userId -> array of group matches
  let groupMatches = $state(new Map());
  // Map of userId -> aggregate stats
  let groupAggregates = $state(new Map());
  // Modal state for showing group data
  let showGroupModal = $state(null);

  // Current location (from [Behaviour] Joining / Joining or Creating Room log lines)
  let location = $state({ roomName: null, worldId: null, instanceId: null });

  // Time-in-instance: when we entered the current instance (ms since epoch), null if not in one
  let instanceJoinedAt = $state(null);
  let timeInInstanceDisplay = $state('0:00');
  let timeInInstanceInterval = null;

  // Instance history modal (stopwatch)
  let showHistoryModal = $state(false);
  let instanceHistory = $state([]);
  
  const API_BASE = ((import.meta as any)?.env?.VITE_API_BASE || 'https://fch-toolkit.com');

  // Format elapsed seconds as m:ss or h mm:ss
  function formatElapsed(sec) {
    if (sec < 0 || !Number.isFinite(sec)) return '0:00';
    const h = Math.floor(sec / 3600);
    const m = Math.floor((sec % 3600) / 60);
    const s = Math.floor(sec % 60);
    if (h > 0) return `${h}h ${m}m`;
    if (m > 0) return `${m}:${s.toString().padStart(2, '0')}`;
    return `0:${s.toString().padStart(2, '0')}`;
  }

  // Safely get first grapheme for avatar initial (handles emojis/combining marks)
  function firstGrapheme(s) {
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

  // Get the best group match for a user (prioritizes notifications on)
  function getBestGroupMatch(userId) {
    const matches = groupMatches.get(userId) || [];
    if (matches.length === 0) return null;
    
    // Prioritize matches with notifications on
    const withNotifs = matches.filter((m) => {
      const match = m;
      return match.notifications;
    });
    if (withNotifs.length > 0) {
      return withNotifs[0]; // Return first match with notifications
    }
    
    // Otherwise return first match
    return matches[0];
  }

  // Check if user has any group watchlist matches
  function hasGroupWatchlist(userId) {
    return groupMatches.has(userId) && (groupMatches.get(userId) || []).length > 0;
  }

  // Get sorted active users (group watchlist first, then local watchlist, then others)
  // Includes safety deduplication to prevent duplicate key errors
  function getSortedActiveUsers() {
    // Safety: Deduplicate by userId (keep first occurrence)
    const seen = new Set();
    const unique = activeUsers.filter(user => {
      if (seen.has(user.userId)) {
        return false; // Skip duplicate
      }
      seen.add(user.userId);
      return true;
    });
    
    // If duplicates were found, log a warning (helps diagnose the root cause)
    if (unique.length !== activeUsers.length) {
      console.warn(`[DUPLICATE DETECTED] Found ${activeUsers.length - unique.length} duplicate(s) in activeUsers. This should not happen with the queue system.`);
    }
    
    return unique.sort((a, b) => {
      const aHasGroup = hasGroupWatchlist(a.userId);
      const bHasGroup = hasGroupWatchlist(b.userId);
      const aHasLocal = watch.get(a.userId) || false;
      const bHasLocal = watch.get(b.userId) || false;
      
      // Group watchlist users first
      if (aHasGroup && !bHasGroup) return -1;
      if (!aHasGroup && bHasGroup) return 1;
      
      // If both or neither have group watchlist, sort by local watchlist
      if (aHasLocal && !bHasLocal) return -1;
      if (!aHasLocal && bHasLocal) return 1;
      
      // Otherwise maintain original order (or sort by username)
      return (a.username || '').localeCompare(b.username || '');
    });
  }

  // Open group watchlist modal for a user
  function openGroupModal(userId) {
    const matches = groupMatches.get(userId) || [];
    const aggregate = groupAggregates.get(userId);
    
    if (matches.length > 0 || aggregate) {
      showGroupModal = {
        userId,
        matches,
        aggregate
      };
    }
  }

  // Schedule batched note lookup for a user
  function scheduleNoteLookup(userId) {
    if (!userId || watch.has(userId) && noted.has(userId)) return;
    
    // Add to pending batch
    pendingNoteLookups.add(userId);
    pendingNoteLookups = new Set(pendingNoteLookups);
    
    // Clear existing timer
    if (noteBatchTimer) {
      clearTimeout(noteBatchTimer);
    }
    
    // Schedule batch lookup after delay
    noteBatchTimer = setTimeout(() => {
      flushNoteBatch();
    }, NOTE_BATCH_DELAY);
  }
  
  // Flush pending note lookups batch
  async function flushNoteBatch() {
    if (pendingNoteLookups.size === 0) {
      noteBatchTimer = null;
      return;
    }
    
    const userIds = Array.from(pendingNoteLookups);
    pendingNoteLookups.clear();
    pendingNoteLookups = new Set();
    noteBatchTimer = null;
    
    const startTime = performance.now();
    try {
      // Load all notes once for all pending users
      const res = await invoke('get_all_notes');
      const allNotesData = res as any;
      const watchlistObj = (allNotesData?.watchlist || {}) as Record<string, boolean>;
      const notesObj = (allNotesData?.notes || {}) as Record<string, any[]>;
      
      // Update watch and note status for all batched users
      for (const userId of userIds) {
        const userIdStr = String(userId);
        // Update watch status
        if (!watch.has(userIdStr)) {
          watch.set(userIdStr, !!(watchlistObj[userIdStr]));
        }
        
        // Update note status
        if (!noted.has(userIdStr)) {
          const userNotes = notesObj[userIdStr];
          const hasNote = userNotes && Array.isArray(userNotes) && userNotes.length > 0;
          const lastNote = hasNote ? userNotes[userNotes.length - 1] : null;
          const noteText = lastNote?.text || null;
          noted.set(userIdStr, !!(noteText && String(noteText).trim().length > 0));
        }
      }
      
      // Trigger reactivity
      watch = new Map(watch);
      noted = new Map(noted);
      
      const duration = performance.now() - startTime;
      console.log(`[PERF] flushNoteBatch(${userIds.length} users): ${duration.toFixed(2)}ms`);
    } catch (err) {
      console.error('Failed to flush note batch:', err);
    }
  }

  // Load watch status for a user from the database (legacy - use scheduleNoteLookup instead)
  async function loadWatchStatus(userId) {
    if (!userId || watch.has(userId)) return;
    scheduleNoteLookup(userId);
  }

  // Load note status for a user from the database (legacy - use scheduleNoteLookup instead)
  async function loadNoteStatus(userId) {
    if (!userId || noted.has(userId)) return;
    scheduleNoteLookup(userId);
  }

  // Refresh watch/note status for all active users
  async function refreshAllUserStatus() {
    if (isInitializing) return; // Prevent concurrent calls
    const startTime = performance.now();
    console.log(`[PERF] refreshAllUserStatus START (${activeUsers.length} users)`);
    
    // Load all notes data once instead of per-user file reads
    const loadStart = performance.now();
    let allNotesData = null;
    try {
      const res = await invoke('get_all_notes');
      allNotesData = res;
      const loadDuration = performance.now() - loadStart;
      console.log(`[PERF] refreshAllUserStatus get_all_notes: ${loadDuration.toFixed(2)}ms`);
    } catch (err) {
      console.error('Failed to load all notes:', err);
      const totalDuration = performance.now() - startTime;
      console.log(`[PERF] refreshAllUserStatus END (error): ${totalDuration.toFixed(2)}ms`);
      return;
    }
    
    // Process users and populate watch/note status from loaded data
    const processStart = performance.now();
    const watchlist = allNotesData?.watchlist || {};
    const notes = allNotesData?.notes || {};
    
    for (const user of activeUsers) {
      const userId = user.userId;
      // Set watch status from loaded data
      watch.set(userId, !!(watchlist[userId]));
      // Set note status from loaded data (check if user has any notes)
      const userNotes = notes[userId];
      const hasNote = userNotes && Array.isArray(userNotes) && userNotes.length > 0;
      const lastNote = hasNote ? userNotes[userNotes.length - 1] : null;
      const noteText = lastNote?.text || null;
      noted.set(userId, !!(noteText && String(noteText).trim().length > 0));
    }
    
    // Update reactive maps
    watch = new Map(watch);
    noted = new Map(noted);
    
    const processDuration = performance.now() - processStart;
    console.log(`[PERF] refreshAllUserStatus data processing: ${processDuration.toFixed(2)}ms`);
    
    const totalDuration = performance.now() - startTime;
    console.log(`[PERF] refreshAllUserStatus END: ${totalDuration.toFixed(2)}ms`);
  }

  // Enqueue event for sequential processing
  function enqueuePlayerEvent(event) {
    const payload = event.payload;
    if (!payload || typeof payload !== 'object') return;
    
    eventQueue.push(payload);
    processEventQueue();
  }
  
  // Process events sequentially (one at a time) to prevent race conditions
  async function processEventQueue() {
    // If already processing, just return (events will be processed by the active processor)
    if (isProcessingQueue) {
      return;
    }
    
    // Set flag immediately to prevent concurrent processing
    isProcessingQueue = true;
    
    try {
      // Process all queued events sequentially
      while (eventQueue.length > 0) {
        const p = eventQueue.shift(); // Remove and get first event
        // Process synchronously - state updates are synchronous
        processPlayerEvent(p);
      }
    } finally {
      // Always reset flag, even if an error occurs
      isProcessingQueue = false;
      
      // If more events arrived while processing, process them now
      if (eventQueue.length > 0) {
        // Use setTimeout to avoid stack overflow and allow UI to update
        setTimeout(() => processEventQueue(), 0);
      }
    }
  }
  
  // Process a single player event (synchronous state updates)
  function processPlayerEvent(p) {
    // Check if log file changed - if so, purge all users
    if (p.file && p.file !== currentLogFile) {
      if (currentLogFile !== null) {
        // New log file detected, clear all users (they all "left" the old instance)
        activeUsers = [];
        watch.clear();
        noted.clear();
        watch = new Map(watch);
        noted = new Map(noted);
      }
      currentLogFile = p.file;
    }

    if (p.event === 'player_joined') {
      // Add user to active list if not already present
      const userId = p.user_id || '';
      const username = p.username || 'Unknown';
      const timestamp = p.timestamp || '';

      // Check if user already exists (for logging/debugging)
      const existingCount = activeUsers.filter(u => u.userId === userId).length;
      if (existingCount > 0) {
        console.warn(`[DUPLICATE EVENT] Received player_joined for ${userId} (${username}), but ${existingCount} entry/entries already exist. This suggests duplicate events from backend.`);
      }

      // Atomic: filter out any existing entry first, then add
      // This ensures only one entry per userId even if duplicates arrive
      activeUsers = [
        ...activeUsers.filter(u => u.userId !== userId),
        {
          userId,
          username,
          joinedAt: timestamp
        }
      ];
      
      // Add user to batch for group watchlist check
      invoke('add_user_to_batch_command', { userId }).catch((err) => {
        console.error('Failed to add user to batch:', err);
      });
      // Only load watch/note status if not initializing (during retroactive scan)
      // During initialization, refreshAllUserStatus will bulk-load all statuses
      if (!isInitializing) {
        scheduleNoteLookup(userId);
      }
    } else if (p.event === 'player_left') {
      // Remove user from active list
      const userId = p.user_id || '';
      activeUsers = activeUsers.filter(u => u.userId !== userId);
      // Note: We keep watch/note status in memory even after they leave
      // so it's available if they rejoin
    }
  }
  
  // Legacy handler (now just enqueues)
  function handlePlayerEvent(event) {
    enqueuePlayerEvent(event);
  }

  onMount(async () => {
    const mountStartTime = performance.now();
    console.log('[PERF] instance-monitor onMount START');
    try {
      const { listen } = await import('@tauri-apps/api/event');
      
      const listenerSetupStart = performance.now();
      // Listen for player events
      const playerUnlisten = await listen('player_event', handlePlayerEvent);
      
      // Listen for instance cleared events (when successfully joined room or OnLeftRoom)
      const clearUnlisten = await listen('instance_cleared', async (e) => {
        const payload = (e?.payload || {}) as any;
        const left = payload?.left === true; // true when OnLeftRoom, false when Successfully joined room
        
        // Wait for any pending events to finish processing before clearing
        // This ensures instance_cleared happens after all queued player events
        while (isProcessingQueue || eventQueue.length > 0) {
          await new Promise(resolve => setTimeout(resolve, 10));
        }
        
        activeUsers = [];
        // Only clear timer/location when we actually left; keep them when Successfully joined room (new instance)
        if (left) {
          instanceJoinedAt = null;
          location = { roomName: null, worldId: null, instanceId: null };
        }
        watch.clear();
        noted.clear();
        groupMatches.clear();
        groupAggregates.clear();
        watch = new Map(watch);
        noted = new Map(noted);
        groupMatches = new Map(groupMatches);
        groupAggregates = new Map(groupAggregates);
      });

      // Listen for location updates ([Behaviour] Joining world:instance, Joining or Creating Room)
      const locationUnlisten = await listen('location_update', (e) => {
        const payload = (e?.payload || {}) as any;
        if (!payload || typeof payload !== 'object') return;
        location = {
          roomName: payload.room_name ?? payload.roomName ?? null,
          worldId: payload.world_id ?? payload.worldId ?? null,
          instanceId: payload.instance_id ?? payload.instanceId ?? null
        };
        // Start time-in-instance timer when we have a valid location
        if (location.roomName || location.worldId || location.instanceId) {
          instanceJoinedAt = Date.now();
        } else {
          instanceJoinedAt = null;
        }
      });
      
      // Listen for group watchlist results
      const groupWatchUnlisten = await listen('group_watch_results', (e) => {
        const payload = (e?.payload || {}) as any;
        if (!payload || typeof payload !== 'object') return;
        
        const matches = Array.isArray(payload.matches) ? payload.matches : [];
        const aggregates = Array.isArray(payload.aggregates) ? payload.aggregates : [];
        
        // Process matches - group by user_id
        for (const match of matches) {
          const userId = String(match.user_id || '');
          if (!userId) continue;
          
          const existingMatches = groupMatches.get(userId) || [];
          const newMatch = {
            user_id: userId,
            group_id: String(match.group_id || ''),
            group_name: match.groupName || match.group_name,
            watchlist: Boolean(match.watchlist),
            notes: match.notes ? String(match.notes) : undefined,
            notifications: Boolean(match.notifications !== undefined ? match.notifications : match.watchlist)
          };
          
          // Check if this group already exists for this user
          const exists = existingMatches.some((m) => {
            const matchItem = m;
            return matchItem.group_id === newMatch.group_id;
          });
          if (!exists) {
            existingMatches.push(newMatch);
            groupMatches.set(userId, existingMatches);
          }
        }
        
        // Process aggregates
        for (const agg of aggregates) {
          const userId = String(agg.user_id || '');
          if (!userId) continue;
          
          groupAggregates.set(userId, {
            user_id: userId,
            warns: Number(agg.warns || 0),
            kicks: Number(agg.kicks || 0),
            bans: Number(agg.bans || 0)
          });
        }
        
        // Trigger reactivity
        groupMatches = new Map(groupMatches);
        groupAggregates = new Map(groupAggregates);
        
        // Play notification sounds for users with notifications enabled
        for (const match of matches) {
          const userId = String(match.user_id || '');
          if (!userId) continue;
          
          const hasGroupNotifs = Boolean(match.notifications !== undefined ? match.notifications : match.watchlist);
          const hasLocalWatch = watch.get(userId) || false;
          
          if (hasGroupNotifs || hasLocalWatch) {
            // Play sound asynchronously
            invoke('play_user_notification_sound', {
              userId,
              hasGroupNotifications: hasGroupNotifs,
              hasLocalNotifications: hasLocalWatch && !hasGroupNotifs
            }).catch((err) => {
              console.error('Failed to play notification sound:', err);
            });
          }
        }
      });
      
      unlisten = () => {
        playerUnlisten();
        clearUnlisten();
        locationUnlisten();
        groupWatchUnlisten();
      };
      const listenerSetupDuration = performance.now() - listenerSetupStart;
      console.log(`[PERF] instance-monitor event listeners setup: ${listenerSetupDuration.toFixed(2)}ms`);
      
      // Run retroactive scan on startup (only if loading screen is still active)
      // This prevents duplicate scans if user switches tabs quickly
      const currentLoadingState = get(isLoading);
      if (currentLoadingState) {
        isInitializing = true;
        try {
          loadingMessage.set('Scanning log files...');
          const scanStartTime = performance.now();
          console.log('[PERF] manual_refresh_scan START');
          // Wait for scan to complete - this will emit all player events
          // but handlePlayerEvent will skip loading individual statuses during isInitializing
        const result = await invoke('manual_refresh_scan');
          const scanDuration = performance.now() - scanStartTime;
          console.log(`[PERF] manual_refresh_scan END: ${scanDuration.toFixed(2)}ms`);
        console.log('Startup retroactive scan result:', result);
          
          // Now that scan is complete, bulk-load all user statuses
          // This is much faster than loading individually
          loadingMessage.set('Loading user data...');
          await refreshAllUserStatus();
      } catch (err) {
        console.error('Startup retroactive scan failed:', err);
      }

        // Mark initialization as complete
        hasInitialized = true;
        isInitializing = false;
        
          loadingMessage.set('Ready');
        await new Promise(resolve => setTimeout(resolve, 300)); // Brief delay for smooth transition

        // Fallback: fetch current location in case location_update was missed
        try {
          const loc = await invoke('get_current_location') as any;
          if (loc && typeof loc === 'object') {
            location = {
              roomName: loc.room_name ?? loc.roomName ?? null,
              worldId: loc.world_id ?? loc.worldId ?? null,
              instanceId: loc.instance_id ?? loc.instanceId ?? null
            };
            if (location.roomName || location.worldId || location.instanceId) {
              instanceJoinedAt = Date.now();
            }
          }
        } catch (_) { /* ignore */ }

        // Only dismiss if we're still the one controlling the loading state
        const stillLoading = get(isLoading);
        if (stillLoading) {
          isLoading.set(false);
        }
      } else {
        // If loading screen already dismissed, mark as initialized
        hasInitialized = true;
      }
      const mountDuration = performance.now() - mountStartTime;
      console.log(`[PERF] instance-monitor onMount END: ${mountDuration.toFixed(2)}ms`);
    } catch (err) {
      console.error('Failed to set up event listeners:', err);
      // If there's an error, still try to dismiss loading screen
      isInitializing = false;
      hasInitialized = true;
      isLoading.set(false);
    }
  });

  // Refresh status when activeUsers changes (e.g., after retroactive scan)
  // Only run after initial initialization is complete to avoid blocking startup
  $effect(() => {
    if (hasInitialized && activeUsers.length > 0 && !isInitializing) {
      // Use setTimeout to avoid blocking the UI thread
      setTimeout(() => {
      refreshAllUserStatus();
      }, 0);
    }
  });

  // Time-in-instance: run a 1s ticker when we're in an instance
  $effect(() => {
    const joined = instanceJoinedAt;
    if (joined == null) {
      if (timeInInstanceInterval) {
        clearInterval(timeInInstanceInterval);
        timeInInstanceInterval = null;
      }
      timeInInstanceDisplay = '0:00';
      return;
    }
    const tick = () => {
      const elapsed = (Date.now() - joined) / 1000;
      timeInInstanceDisplay = formatElapsed(elapsed);
    };
    tick(); // immediate update
    timeInInstanceInterval = setInterval(tick, 1000);
    return () => {
      if (timeInInstanceInterval) {
        clearInterval(timeInInstanceInterval);
        timeInInstanceInterval = null;
      }
    };
  });

  onDestroy(() => {
    if (unlisten) {
      unlisten();
    }
    if (timeInInstanceInterval) {
      clearInterval(timeInInstanceInterval);
      timeInInstanceInterval = null;
    }
    // Clear note batch timer
    if (noteBatchTimer) {
      clearTimeout(noteBatchTimer);
    }
    // Flush any pending lookups
    if (pendingNoteLookups.size > 0) {
      flushNoteBatch();
    }
  });
</script>

<div class="panel">
  <!-- Location: always visible above player list -->
  <div class="location-card">
    <div class="location-header">
      <div class="location-title">{location.roomName || '—'}</div>
      {#if instanceJoinedAt != null}
        <span class="time-in-instance-capsule">{timeInInstanceDisplay}</span>
      {/if}
      <button
        class="stopwatch-btn"
        title="Instance history"
        aria-label="Instance history"
        onclick={async () => {
          showHistoryModal = true;
          try {
            const hist = await invoke('get_instance_history') as any[];
            instanceHistory = Array.isArray(hist) ? hist : [];
          } catch (err) {
            console.error('Failed to load instance history:', err);
            instanceHistory = [];
          }
        }}
      >
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
          <circle cx="12" cy="13" r="8" stroke="currentColor" stroke-width="1.5"/>
          <path d="M12 9v4l3 3" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
          <path d="M12 5V3" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
          <path d="M12 21v-2" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
          <path d="M16 5l1.5-1.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
          <path d="M6.5 15.5L5 14" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
          <circle cx="12" cy="7" r="1.5" fill="currentColor"/>
        </svg>
      </button>
    </div>
    <div class="location-subtext">
      {#if location.worldId || location.instanceId}
        World ID: {location.worldId || '—'}
        <br />
        Instance ID: {location.instanceId || '—'}
      {:else}
        —
      {/if}
    </div>
  </div>

  <div class="header">
    <div class="meta">{activeUsers.length} user{activeUsers.length === 1 ? '' : 's'} in instance</div>
  </div>

  <div class="list" role="list">
    {#if activeUsers.length === 0}
      <div class="empty">Waiting for players…</div>
    {:else}
      {#each getSortedActiveUsers() as user (user.userId)}
        {@const bestMatch = getBestGroupMatch(user.userId)}
        <div class="row" role="listitem">
          <div 
            class="avatar" 
            class:glow-notifications-on={bestMatch && bestMatch.notifications}
            class:glow-notifications-off={bestMatch && !bestMatch.notifications}
            aria-hidden="true"
          >
            {firstGrapheme(user.username)}
          </div>
          <div class="col">
            <div class="name-row">
              <div class="name">{user.username || 'Unknown'}</div>
              {#if hasGroupWatchlist(user.userId)}
                {@const bestMatch = getBestGroupMatch(user.userId)}
                {#if bestMatch}
                  <button
                    class="group-pill"
                    class:notifications-on={bestMatch.notifications}
                    class:notifications-off={!bestMatch.notifications}
                    onclick={() => openGroupModal(user.userId)}
                    title="Group Watchlisted - Click for details"
                  >
                    Group Watchlisted
                  </button>
                {/if}
              {/if}
            </div>
            <div class="sub">{user.userId}</div>
          </div>
          <div class="actions">
            <button 
              class="watch-btn" 
              class:active={watch.get(user.userId)}
              title="Notifies you when this user joins."
              aria-label="Notify"
              onclick={async () => {
                const newVal = !watch.get(user.userId);
                try {
                  await invoke('set_watch', { userId: user.userId, watch: newVal });
                  watch.set(user.userId, newVal);
                  watch = new Map(watch);
                } catch (err) {
                  console.error('Failed to set watch:', err);
                }
              }}
            >
              {#if watch.get(user.userId)}
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
              <span>Notify</span>
            </button>
            <button 
              class="note-btn" 
              class:has-note={noted.get(user.userId)}
              title="Edit the information attached to this players account."
              aria-label="Edit Note"
              onclick={async () => {
                showNoteFor = user.userId;
                try {
                  const res = await invoke('get_note', { userId: user.userId });
                  const resObj = (res || {}) as any;
                  noteText = (resObj && resObj.text) ? resObj.text : '';
                } catch (err) {
                  console.error('Failed to get note:', err);
                  noteText = '';
                }
              }}
            >
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                <rect x="5" y="3" width="14" height="18" rx="2" stroke="currentColor" stroke-width="1.5"/>
                <path d="M8 8h8M8 12h8M8 16h6" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
              </svg>
              <span>Edit Note</span>
            </button>
          </div>
        </div>
        {#if showNoteFor === user.userId}
          <div class="note-editor">
            <textarea placeholder="Write a note..." bind:value={noteText}></textarea>
            <div class="note-actions">
              <button onclick={() => { showNoteFor = null; noteText = ''; }}>Cancel</button>
              <button onclick={async () => {
                try {
                  await invoke('add_note', { userId: user.userId, text: noteText });
                  noted.set(user.userId, noteText.trim().length > 0);
                  noted = new Map(noted);
                } catch (err) {
                  console.error('Failed to save note:', err);
                }
                showNoteFor = null;
                noteText = '';
              }}>Save</button>
            </div>
          </div>
        {/if}
      {/each}
    {/if}
  </div>
</div>

{#if showGroupModal}
  <div 
    class="modal-backdrop" 
    role="button"
    tabindex="0"
    onclick={(e) => { 
      if (e.target === e.currentTarget) {
        showGroupModal = null;
      }
    }}
    onkeydown={(e) => { if (e.key === 'Escape' || e.key === 'Enter') showGroupModal = null; }}
  >
    <div class="modal" role="dialog">
      <header>
        <h3>Group Watchlist Details</h3>
        <button class="close-btn" onclick={() => { showGroupModal = null; }}>×</button>
      </header>
      <div class="modal-content">
        <div class="user-info">
          <strong>User ID:</strong> {showGroupModal.userId}
        </div>
        
        {#if showGroupModal.aggregate}
          <div class="aggregate-section">
            <h4>Moderation History</h4>
            <div class="aggregate-stats">
              <span class="stat warns">Warns: {showGroupModal.aggregate.warns}</span>
              <span class="stat kicks">Kicks: {showGroupModal.aggregate.kicks}</span>
              <span class="stat bans">Bans: {showGroupModal.aggregate.bans}</span>
            </div>
          </div>
        {/if}
        
        {#if showGroupModal.matches.length > 0}
          <div class="matches-section">
            <h4>Group Matches ({showGroupModal.matches.length})</h4>
            <div class="matches-list">
              {#each showGroupModal.matches as match}
                <div class="match-item">
                  <div class="match-header">
                    <span class="group-name">{match.group_name || 'Unknown Group'}</span>
                    <span class="match-badges">
                      {#if match.watchlist}
                        <span class="badge watchlist">Watchlisted</span>
                      {/if}
                      {#if match.notifications}
                        <span class="badge notifications">Notifications On</span>
                      {:else}
                        <span class="badge no-notifications">Notifications Off</span>
                      {/if}
                    </span>
                  </div>
                  {#if match.notes}
                    <div class="match-notes">
                      <strong>Notes:</strong> {match.notes}
                    </div>
                  {/if}
                  <div class="match-meta">
                    Group ID: {match.group_id}
                  </div>
                </div>
              {/each}
            </div>
          </div>
        {:else}
          <div class="no-matches">No group matches found.</div>
        {/if}
      </div>
    </div>
  </div>
{/if}

{#if showHistoryModal}
  <div
    class="modal-backdrop history-modal-backdrop"
    role="button"
    tabindex="0"
    onclick={(e) => { if (e.target === e.currentTarget) showHistoryModal = false; }}
    onkeydown={(e) => { if (e.key === 'Escape') showHistoryModal = false; }}
  >
    <div class="modal history-modal" role="dialog">
      <header>
        <h3>Instance History</h3>
        <button class="close-btn" onclick={() => showHistoryModal = false}>×</button>
      </header>
      <div class="history-content">
        {#if instanceHistory.length === 0}
          <div class="history-empty">No instance history yet.</div>
        {:else}
          <div class="history-list">
            {#each instanceHistory as entry}
              <div class="history-capsule" class:join={entry.kind === 'join'} class:leave={entry.kind === 'leave'}>
                <span class="history-timestamp">{entry.timestamp || '—'}</span>
                <span class="history-kind">{entry.kind === 'join' ? 'Join' : 'Leave'}</span>
                {#if entry.kind === 'join' && (entry.room_name || entry.world_id || entry.instance_id)}
                  <div class="history-details">
                    {#if entry.room_name}
                      <span class="history-room">{entry.room_name}</span>
                    {/if}
                    {#if entry.world_id}
                      <span class="history-id">World: {entry.world_id}</span>
                    {/if}
                    {#if entry.instance_id}
                      <span class="history-id">Instance: {entry.instance_id}</span>
                    {/if}
                  </div>
                {/if}
              </div>
            {/each}
          </div>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 12px;
    height: 100%;
  }

  .location-card {
    padding: 12px 16px;
    background: var(--bg-elev);
    border: 1px solid var(--border);
    border-radius: 8px;
    flex-shrink: 0;
  }

  .location-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .location-title {
    font-size: 16px;
    font-weight: 600;
    color: var(--fg);
    flex: 1;
    min-width: 0;
  }

  .time-in-instance-capsule {
    flex-shrink: 0;
    padding: 4px 10px;
    border-radius: 12px;
    font-size: 12px;
    font-weight: 600;
    font-family: 'Consolas', 'Monaco', 'Courier New', monospace;
    background: var(--bg);
    border: 1px solid var(--border);
    color: var(--fg-muted);
  }

  .stopwatch-btn {
    flex-shrink: 0;
    width: 32px;
    height: 32px;
    padding: 0;
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--fg-muted);
    border-radius: 6px;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }

  .stopwatch-btn:hover {
    background: var(--bg-hover);
    border-color: var(--accent);
    color: var(--accent);
  }

  .location-subtext {
    font-size: 12px;
    color: var(--fg-muted);
    margin-top: 4px;
    font-family: 'Consolas', 'Monaco', 'Courier New', monospace;
    word-break: break-all;
  }

  .header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .meta {
    color: var(--fg-muted);
    font-size: 12px;
  }

  .list {
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--bg-elev);
    min-height: 160px;
    max-height: calc(100vh - 220px);
    overflow-y: auto;
    flex: 1;
  }

  .empty {
    padding: 16px;
    color: var(--fg-muted);
    text-align: center;
  }

  .row {
    display: grid;
    grid-template-columns: 36px 1fr auto;
    gap: 12px;
    align-items: center;
    padding: 10px 12px;
    border-bottom: 1px solid var(--border);
  }

  .row:last-child {
    border-bottom: none;
  }

  .avatar {
    width: 36px;
    height: 36px;
    border-radius: 8px;
    background: var(--bg);
    color: var(--fg-muted);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-weight: 600;
    font-size: 16px;
    transition: box-shadow 0.3s ease;
  }

  .avatar.glow-notifications-on {
    box-shadow: 0 0 12px rgba(255, 107, 107, 0.6), 0 0 24px rgba(255, 107, 107, 0.4);
    animation: glow-pulse-red 2s ease-in-out infinite;
  }

  .avatar.glow-notifications-off {
    box-shadow: 0 0 12px rgba(255, 169, 77, 0.6), 0 0 24px rgba(255, 169, 77, 0.4);
    animation: glow-pulse-yellow 2s ease-in-out infinite;
  }

  @keyframes glow-pulse-red {
    0%, 100% {
      box-shadow: 0 0 12px rgba(255, 107, 107, 0.6), 0 0 24px rgba(255, 107, 107, 0.4);
    }
    50% {
      box-shadow: 0 0 16px rgba(255, 107, 107, 0.8), 0 0 32px rgba(255, 107, 107, 0.6);
    }
  }

  @keyframes glow-pulse-yellow {
    0%, 100% {
      box-shadow: 0 0 12px rgba(255, 169, 77, 0.6), 0 0 24px rgba(255, 169, 77, 0.4);
    }
    50% {
      box-shadow: 0 0 16px rgba(255, 169, 77, 0.8), 0 0 32px rgba(255, 169, 77, 0.6);
    }
  }

  .col {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .name-row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .name {
    color: var(--fg);
    font-weight: 600;
    font-size: 14px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .group-pill {
    padding: 4px 10px;
    border-radius: 12px;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    border: none;
    cursor: pointer;
    transition: all 0.2s;
    white-space: nowrap;
  }

  .group-pill.notifications-on {
    background: rgba(255, 107, 107, 0.2);
    color: #ff6b6b;
    border: 1px solid rgba(255, 107, 107, 0.4);
  }

  .group-pill.notifications-on:hover {
    background: rgba(255, 107, 107, 0.3);
    box-shadow: 0 0 0 2px rgba(255, 107, 107, 0.2);
  }

  .group-pill.notifications-off {
    background: rgba(255, 169, 77, 0.2);
    color: #ffa94d;
    border: 1px solid rgba(255, 169, 77, 0.4);
  }

  .group-pill.notifications-off:hover {
    background: rgba(255, 169, 77, 0.3);
    box-shadow: 0 0 0 2px rgba(255, 169, 77, 0.2);
  }

  .sub {
    color: var(--fg-muted);
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .watch-btn,
  .note-btn {
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--fg);
    border-radius: 6px;
    padding: 4px 8px;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    font-size: 13px;
  }

  .watch-btn:hover,
  .note-btn:hover {
    background: var(--bg-hover);
    border-color: var(--accent);
  }

  .watch-btn.active {
    background: rgba(255, 182, 193, 0.35);
    border-color: #ffb6c1;
  }

  .note-btn.has-note {
    background: rgba(255, 182, 193, 0.35);
    border-color: #ffb6c1;
  }

  .watch-btn svg,
  .note-btn svg {
    display: block;
    width: 16px;
    height: 16px;
  }

  .note-editor {
    grid-column: 1 / -1;
    padding: 8px 12px;
    background: var(--bg-elev);
    border-top: 1px dashed var(--border);
    display: grid;
    gap: 8px;
  }

  .note-editor textarea {
    width: 100%;
    min-height: 72px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg);
    color: var(--fg);
    padding: 8px;
    resize: vertical;
    font-family: inherit;
    font-size: 13px;
  }

  .note-editor textarea:focus {
    outline: none;
    border-color: var(--accent);
  }

  .note-actions {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
  }

  .note-actions button {
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--fg);
    border-radius: 8px;
    padding: 4px 10px;
    cursor: pointer;
    font-size: 13px;
  }

  .note-actions button:hover {
    background: var(--bg-hover);
    border-color: var(--accent);
  }

  /* Group Watchlist Modal */
  .modal-backdrop {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(0, 0, 0, 0.7);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
    padding: 20px;
  }

  .modal {
    background: var(--bg-elev);
    border: 1px solid var(--border);
    border-radius: 12px;
    max-width: 600px;
    width: 100%;
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.3);
  }

  .modal header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 20px;
    border-bottom: 1px solid var(--border);
  }

  .modal header h3 {
    margin: 0;
    font-size: 18px;
    font-weight: 600;
  }

  .modal .close-btn {
    width: 32px;
    height: 32px;
    border: none;
    background: transparent;
    color: var(--fg);
    font-size: 24px;
    cursor: pointer;
    border-radius: 6px;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: background 0.2s;
  }

  .modal .close-btn:hover {
    background: var(--bg-hover);
  }

  .modal-content {
    padding: 20px;
    overflow-y: auto;
    flex: 1;
  }

  .user-info {
    margin-bottom: 20px;
    padding: 12px;
    background: var(--bg);
    border-radius: 8px;
    font-size: 13px;
  }

  .aggregate-section {
    margin-bottom: 20px;
  }

  .aggregate-section h4 {
    margin: 0 0 12px 0;
    font-size: 16px;
    font-weight: 600;
  }

  .aggregate-stats {
    display: flex;
    gap: 12px;
    flex-wrap: wrap;
  }

  .stat {
    padding: 6px 12px;
    border-radius: 6px;
    font-size: 12px;
    font-weight: 600;
    background: var(--bg);
    border: 1px solid var(--border);
  }

  .stat.warns {
    color: #ffa94d;
    border-color: rgba(255, 169, 77, 0.3);
  }

  .stat.kicks {
    color: #ff6b6b;
    border-color: rgba(255, 107, 107, 0.3);
  }

  .stat.bans {
    color: #ff4757;
    border-color: rgba(255, 71, 87, 0.3);
  }

  .matches-section h4 {
    margin: 0 0 12px 0;
    font-size: 16px;
    font-weight: 600;
  }

  .matches-list {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .match-item {
    padding: 12px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 8px;
  }

  .match-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 8px;
    flex-wrap: wrap;
  }

  .group-name {
    font-weight: 600;
    font-size: 14px;
  }

  .match-badges {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }

  .badge {
    padding: 3px 8px;
    border-radius: 4px;
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
  }

  .badge.watchlist {
    background: rgba(255, 20, 147, 0.2);
    color: #ff1493;
  }

  .badge.notifications {
    background: rgba(255, 107, 107, 0.2);
    color: #ff6b6b;
  }

  .badge.no-notifications {
    background: rgba(255, 169, 77, 0.2);
    color: #ffa94d;
  }

  .match-notes {
    margin-top: 8px;
    padding: 8px;
    background: var(--bg-elev);
    border-radius: 6px;
    font-size: 12px;
    color: var(--fg-muted);
    white-space: pre-wrap;
    word-break: break-word;
  }

  .match-meta {
    margin-top: 8px;
    font-size: 11px;
    color: var(--fg-muted);
    font-family: 'Courier New', monospace;
  }

  .no-matches {
    padding: 24px;
    text-align: center;
    color: var(--fg-muted);
    font-style: italic;
  }

  /* Instance History Modal */
  .history-content {
    padding: 16px;
    overflow-y: auto;
    max-height: 60vh;
  }

  .history-empty {
    padding: 32px;
    text-align: center;
    color: var(--fg-muted);
    font-style: italic;
  }

  .history-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .history-capsule {
    padding: 10px 14px;
    border-radius: 8px;
    border: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .history-capsule.join {
    border-color: rgba(107, 255, 107, 0.5);
    background: rgba(107, 255, 107, 0.08);
  }

  .history-capsule.leave {
    border-color: rgba(255, 107, 107, 0.5);
    background: rgba(255, 107, 107, 0.08);
  }

  .history-timestamp {
    font-size: 12px;
    color: var(--fg-muted);
    font-family: 'Consolas', 'Monaco', 'Courier New', monospace;
  }

  .history-kind {
    font-size: 13px;
    font-weight: 600;
  }

  .history-capsule.join .history-kind {
    color: #6bff6b;
  }

  .history-capsule.leave .history-kind {
    color: #ff6b6b;
  }

  .history-details {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-top: 4px;
    font-size: 12px;
    color: var(--fg-muted);
  }

  .history-room {
    font-weight: 500;
    color: var(--fg);
  }

  .history-id {
    font-family: 'Consolas', 'Monaco', 'Courier New', monospace;
    word-break: break-all;
  }
</style>
