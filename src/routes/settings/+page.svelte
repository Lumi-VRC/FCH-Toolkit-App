<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';

  let masterVolume = $state(1.0);
  let groupNotificationVolume = $state(0.8);
  let groupNotificationSound = $state(null as string | null);
  let localNotificationVolume = $state(0.8);
  let localNotificationSound = $state(null as string | null);
  let loading = $state(true);

  async function loadSettings() {
    try {
      const settings = await invoke('get_settings') as any;
      masterVolume = settings.master_volume || 1.0;
      groupNotificationVolume = settings.group_notifications?.volume || 0.8;
      groupNotificationSound = settings.group_notifications?.default_sound_path || null;
      localNotificationVolume = settings.local_notifications?.volume || 0.8;
      localNotificationSound = settings.local_notifications?.default_sound_path || null;
      loading = false;
    } catch (err) {
      console.error('Failed to load settings:', err);
      loading = false;
    }
  }

  async function saveMasterVolume() {
    try {
      await invoke('set_master_volume', { volume: masterVolume });
    } catch (err) {
      console.error('Failed to save master volume:', err);
    }
  }

  async function saveGroupNotificationSettings() {
    try {
      await invoke('set_group_notification_settings', {
        defaultSoundPath: groupNotificationSound || null,
        volume: groupNotificationVolume
      });
    } catch (err) {
      console.error('Failed to save group notification settings:', err);
    }
  }

  async function saveLocalNotificationSettings() {
    try {
      await invoke('set_local_notification_settings', {
        defaultSoundPath: localNotificationSound || null,
        volume: localNotificationVolume
      });
    } catch (err) {
      console.error('Failed to save local notification settings:', err);
    }
  }

  async function browseGroupSound() {
    try {
      const res = await invoke('browse_sound') as any;
      if (res && res.path) {
        groupNotificationSound = res.path;
        await saveGroupNotificationSettings();
      }
    } catch (err) {
      console.error('Failed to browse sound:', err);
    }
  }

  async function browseLocalSound() {
    try {
      const res = await invoke('browse_sound') as any;
      if (res && res.path) {
        localNotificationSound = res.path;
        await saveLocalNotificationSettings();
      }
    } catch (err) {
      console.error('Failed to browse sound:', err);
    }
  }

  function clearGroupSound() {
    groupNotificationSound = null;
    saveGroupNotificationSettings();
  }

  function clearLocalSound() {
    localNotificationSound = null;
    saveLocalNotificationSettings();
  }

  async function previewGroupSound() {
    try {
      await invoke('preview_group_notification_sound');
    } catch (err) {
      console.error('Failed to preview group sound:', err);
    }
  }

  async function previewLocalSound() {
    try {
      await invoke('preview_local_notification_sound');
    } catch (err) {
      console.error('Failed to preview local sound:', err);
    }
  }

  onMount(() => {
    loadSettings();
  });
</script>

<div class="panel">
  <h2>Settings</h2>
  
  {#if loading}
    <div class="loading">Loading settings...</div>
  {:else}
    <div class="settings-grid">
      <!-- Master Volume Panel -->
      <div class="settings-panel master-volume-panel">
        <h3>Master Volume</h3>
        <div class="volume-control">
          <input
            type="range"
            min="0"
            max="1"
            step="0.01"
            bind:value={masterVolume}
            oninput={saveMasterVolume}
            class="volume-slider"
          />
          <span class="volume-value">{Math.round(masterVolume * 100)}%</span>
        </div>
      </div>

      <!-- Group Notifications Panel -->
      <div class="settings-panel">
        <div class="panel-header">
          <h3>Group Notifications</h3>
          <button onclick={previewGroupSound} class="play-btn" title="Preview sound">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
              <path d="M8 5v14l11-7z" fill="currentColor"/>
            </svg>
            Play
          </button>
        </div>
        <div class="notification-controls">
          <div class="sound-path-control">
            <label>Default Sound Path</label>
            <div class="sound-path-row">
              <input
                type="text"
                bind:value={groupNotificationSound}
                placeholder="No sound set"
                readonly
                class="sound-path-input"
              />
              <button onclick={browseGroupSound} class="browse-btn">Browse</button>
              {#if groupNotificationSound}
                <button onclick={clearGroupSound} class="clear-btn">Clear</button>
              {/if}
            </div>
          </div>
          <div class="volume-control">
            <label>Volume</label>
            <div class="volume-row">
              <input
                type="range"
                min="0"
                max="1"
                step="0.01"
                bind:value={groupNotificationVolume}
                oninput={saveGroupNotificationSettings}
                class="volume-slider"
              />
              <span class="volume-value">{Math.round(groupNotificationVolume * 100)}%</span>
            </div>
          </div>
  </div>
      </div>

      <!-- Local Notifications Panel -->
      <div class="settings-panel">
        <div class="panel-header">
          <h3>Local Notifications</h3>
          <button onclick={previewLocalSound} class="play-btn" title="Preview sound">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
              <path d="M8 5v14l11-7z" fill="currentColor"/>
            </svg>
            Play
          </button>
        </div>
        <div class="notification-controls">
          <div class="sound-path-control">
            <label>Default Sound Path</label>
            <div class="sound-path-row">
              <input
                type="text"
                bind:value={localNotificationSound}
                placeholder="No sound set"
                readonly
                class="sound-path-input"
              />
              <button onclick={browseLocalSound} class="browse-btn">Browse</button>
              {#if localNotificationSound}
                <button onclick={clearLocalSound} class="clear-btn">Clear</button>
              {/if}
            </div>
          </div>
          <div class="volume-control">
            <label>Volume</label>
            <div class="volume-row">
              <input
                type="range"
                min="0"
                max="1"
                step="0.01"
                bind:value={localNotificationVolume}
                oninput={saveLocalNotificationSettings}
                class="volume-slider"
              />
              <span class="volume-value">{Math.round(localNotificationVolume * 100)}%</span>
            </div>
          </div>
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 20px;
    padding: 20px;
  }

  h2 {
    margin: 0;
    font-size: 24px;
    font-weight: 600;
    color: var(--fg);
  }

  .loading {
    padding: 40px;
    text-align: center;
    color: var(--fg-muted);
  }

  .settings-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
    gap: 20px;
  }

  .master-volume-panel {
    grid-column: 1 / -1;
  }

  .settings-panel {
    background: var(--bg-elev);
    border: 1px solid var(--border);
    border-radius: 12px;
    padding: 20px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .settings-panel h3 {
    margin: 0;
    font-size: 18px;
    font-weight: 600;
    color: var(--fg);
    flex: 1;
  }

  .play-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg);
    color: var(--fg);
    font-size: 13px;
    cursor: pointer;
    transition: all 0.2s;
    white-space: nowrap;
  }

  .play-btn:hover {
    background: var(--bg-hover);
    border-color: var(--accent);
  }

  .play-btn svg {
    flex-shrink: 0;
  }

  .volume-control {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .volume-control label {
    font-size: 14px;
    color: var(--fg-muted);
    font-weight: 500;
  }

  .volume-row {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .volume-slider {
    flex: 1;
    height: 6px;
    border-radius: 3px;
    background: var(--bg);
    outline: none;
    -webkit-appearance: none;
  }

  .volume-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--accent);
    cursor: pointer;
  }

  .volume-slider::-moz-range-thumb {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--accent);
    cursor: pointer;
    border: none;
  }

  .volume-value {
    min-width: 45px;
    text-align: right;
    font-size: 14px;
    color: var(--fg);
    font-weight: 600;
  }

  .notification-controls {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .sound-path-control {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .sound-path-control label {
    font-size: 14px;
    color: var(--fg-muted);
    font-weight: 500;
  }

  .sound-path-row {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .sound-path-input {
    flex: 1;
    padding: 8px 12px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg);
    color: var(--fg);
    font-size: 13px;
  }

  .sound-path-input:focus {
    outline: none;
    border-color: var(--accent);
  }

  .browse-btn,
  .clear-btn {
    padding: 8px 16px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg);
    color: var(--fg);
    font-size: 13px;
    cursor: pointer;
    transition: all 0.2s;
  }

  .browse-btn:hover {
    background: var(--bg-hover);
    border-color: var(--accent);
  }

  .clear-btn {
    background: rgba(255, 107, 107, 0.1);
    border-color: rgba(255, 107, 107, 0.3);
    color: #ff6b6b;
  }

  .clear-btn:hover {
    background: rgba(255, 107, 107, 0.2);
  }
</style>
