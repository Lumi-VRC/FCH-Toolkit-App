<script lang="ts">
  import SidebarButton from './SidebarButton.svelte';
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { open } from '@tauri-apps/plugin-shell';
  import { listen } from '@tauri-apps/api/event';

  let { collapsed = false, activeIndex = 0, onToggle = () => {}, onSelect = (_) => {} } = $props();

  const labels = [
    'Login', 'Instance Monitor', 'Database', 'Log Explorer', 'World Moderation', 'Settings', 'Debug', 'About'
  ];

  const iconForIndex = (i) =>
    i === 1 ? 'list'
    : i === 2 ? 'vault'
    : i === 3 ? 'search'
    : i === 4 ? 'globe'
    : i === 5 ? 'gear'
    : i === 6 ? 'bug'
    : 'dot';

  let updateAvailable = $state(false);
  let checking = $state(false);
  let showUpdateModal = $state(false);
  let downloading = $state(false);
  let updateInfo = $state(null);
  let updateCheckInterval = null;

  async function getLocalVersion() {
    try {
      const res = await fetch('/version.json', { cache: 'no-store' });
      if (!res.ok) return null;
      const data = await res.json();
      return (data && typeof data.version === 'string') ? data.version : null;
    } catch { 
      return null; 
    }
  }

  async function runUpdateCheck() {
    if (checking) return null; 
    checking = true;
    try {
      const localV = await getLocalVersion();
      if (!localV) {
        console.log('[Updater] No local version found');
        checking = false;
        return null;
      }
      
      console.log(`[Updater] Checking for updates. Local version: ${localV}`);
      const update = await invoke('check_for_update', { localVersion: localV });
      if (update) {
        console.log(`[Updater] Update available: v${update.version}`);
        updateInfo = update;
        updateAvailable = true;
        checking = false;
        return update;
      } else {
        console.log('[Updater] No update available');
        updateAvailable = false;
        updateInfo = null;
        checking = false;
        return null;
      }
    } catch (err) {
      const errMsg = String(err);
      console.error('[Updater] Update check failed:', errMsg);
      // Only suppress "no releases found" errors, log everything else
      if (!errMsg.includes('No releases found') && !errMsg.includes('404')) {
        console.error('Failed to check for updates:', err);
      }
      updateAvailable = false;
      updateInfo = null;
      checking = false;
      return null;
    }
  }

  async function downloadAndInstall() {
    if (!updateInfo || downloading) return;
    
    downloading = true;
    try {
      await invoke('download_and_install_update', {
        downloadUrl: updateInfo.download_url,
        filename: updateInfo.filename
      });
      showUpdateModal = false;
      // The installer will handle closing the app
    } catch (err) {
      console.error('Failed to download/install update:', err);
      alert('Failed to download or install update. Please try downloading manually from the releases page.');
    } finally {
      downloading = false;
    }
  }

  // on startup (client), check once, then every 30 minutes
  onMount(() => { 
    // Initial check
    runUpdateCheck();
    
    // Set up periodic checks every 30 minutes (1800000 ms)
    updateCheckInterval = setInterval(() => {
      runUpdateCheck();
    }, 30 * 60 * 1000);
    
    // Listen for installer started event
    listen('updater:installer-started', () => {
      console.log('Installer started, app will close soon');
    });
  });

  onDestroy(() => {
    // Clean up interval when component is destroyed
    if (updateCheckInterval) {
      clearInterval(updateCheckInterval);
      updateCheckInterval = null;
    }
  });

  async function onUpdateButtonClick(_e) {
    // Run update check and wait for it to complete, get the result directly
    const update = await runUpdateCheck();
    
    // Use the returned value directly instead of relying on reactive state
    if (update) { 
      console.log('[Updater] Showing update modal for v' + update.version);
      updateInfo = update;
      updateAvailable = true;
      showUpdateModal = true; 
    } else {
      console.log('[Updater] No update available, opening releases page');
      try {
        await open('https://github.com/Lumi-VRC/FCH-Toolkit-App/releases/latest');
      } catch (err) {
        console.error('Failed to open releases page:', err);
      }
    }
  }
</script>

<aside class:collapsed>
  <div class="top">
    <button class="collapse" onclick={() => onToggle()} aria-label="Toggle sidebar">
      <span aria-hidden="true">{#if collapsed}›{/if}{#if !collapsed}‹{/if}</span>
    </button>
  </div>
  <div class="buttons">
    {#each labels as title, i}
      <SidebarButton
        {title}
        selected={activeIndex === i}
        onClick={() => onSelect(i)}
        showLabel={!collapsed}
        label={title}
        icon={iconForIndex(i)}
      />
    {/each}
  </div>
  <div class="bottom">
    <button 
      class="update" 
      class:pulse={updateAvailable} 
      onclick={onUpdateButtonClick} 
      title={updateAvailable ? 'Update Available' : 'Check for updates'} 
      aria-label="Check for updates"
    >
      <span class="icon" aria-hidden="true">↑</span>
      {#if !collapsed}<span class="text">{updateAvailable ? 'Update Available' : 'Check for updates'}</span>{/if}
    </button>
  </div>
</aside>

{#if showUpdateModal && updateInfo}
  <div class="modal-backdrop" role="dialog" aria-modal="true" aria-label="Update Available" onclick={(e) => { if (e.target === e.currentTarget) showUpdateModal = false; }}>
    <div class="modal">
      <div class="modal-title">Update Available</div>
      <div class="modal-content">
        <p>A new version is available: <strong>v{updateInfo.version}</strong></p>
        <p class="modal-subtitle">Would you like to download and install it now?</p>
        {#if downloading}
          <p class="modal-status">Downloading and installing update...</p>
        {/if}
      </div>
      <div class="modal-actions">
        <button 
          class="link primary" 
          onclick={downloadAndInstall}
          disabled={downloading}
        >
          {downloading ? 'Installing...' : 'Download & Install'}
        </button>
        <a 
          class="link" 
          href="https://github.com/Lumi-VRC/FCH-Toolkit-App/releases/latest" 
          target="_blank" 
          rel="noopener noreferrer"
          onclick={async (e) => { 
            e.preventDefault(); 
            try { 
              await open('https://github.com/Lumi-VRC/FCH-Toolkit-App/releases/latest'); 
            } catch (err) { 
              console.error('Failed to open releases page:', err); 
            } 
          }}
        >
          Open Releases
        </a>
        <button class="close" onclick={() => showUpdateModal = false} disabled={downloading}>Close</button>
      </div>
    </div>
  </div>
{/if}

<style>
  :global(:root) {
    --bg: #121212;
    --bg-elev: #1a1a1a;
    --bg-hover: #222222;
    --fg: #e6e6e6;
    --fg-muted: #9a9a9a;
    --accent: #4d4d4d;
    --border: #2a2a2a;
  }

  aside {
    width: 220px;
    min-width: 220px;
    background: rgba(26, 26, 26, 0.85);
    backdrop-filter: blur(8px);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 12px;
    transition: width 150ms ease, min-width 150ms ease;
    position: relative;
    z-index: 2;
  }

  aside.collapsed { width: 72px; min-width: 72px; }

  .top { display: flex; justify-content: center; }
  .collapse {
    width: 36px; height: 36px;
    display: inline-flex; align-items: center; justify-content: center;
    background: var(--bg);
    color: var(--fg);
    border: 1px solid var(--border);
    border-radius: 8px;
    cursor: pointer;
  }

  .buttons { display: grid; grid-template-columns: 1fr; gap: 12px; }

  .bottom { margin-top: auto; }
  .update {
    width: 100%;
    height: 40px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    background: var(--bg);
    color: var(--fg);
    border: 1px solid var(--border);
    border-radius: 8px;
    cursor: pointer;
  }
  .update:hover { 
    background: var(--bg-hover); 
    border-color: var(--accent); 
  }
  .update.pulse {
    animation: pulse 1.6s ease-in-out infinite;
    --pulse-color: rgba(255, 182, 193, 0.5);
  }
  @keyframes pulse {
    0%   { box-shadow: 0 0 0 0 var(--pulse-color); background: var(--bg); }
    50%  { box-shadow: 0 0 0 6px transparent; background: #2a1f24; }
    100% { box-shadow: 0 0 0 0 transparent; background: var(--bg); }
  }
  .icon { display: inline-flex; }
  .text { font-size: 12px; font-weight: 600; }

  .modal-backdrop { 
    position: fixed; 
    inset: 0; 
    background: rgba(0,0,0,0.6); 
    display: grid; 
    place-items: center; 
    z-index: 9999; 
  }
  .modal { 
    background: var(--bg-elev); 
    border: 1px solid var(--border); 
    border-radius: 12px; 
    padding: 16px; 
    min-width: 280px; 
    max-width: 90vw; 
  }
  .modal-title { 
    font-weight: 600; 
    margin-bottom: 12px; 
    color: var(--fg);
    font-size: 18px;
  }
  .modal-content {
    margin-bottom: 16px;
    color: var(--fg-muted);
  }
  .modal-content p {
    margin: 8px 0;
  }
  .modal-subtitle {
    font-size: 13px;
  }
  .modal-status {
    font-size: 12px;
    color: var(--accent);
    font-style: italic;
  }
  .modal-actions { 
    display: inline-flex; 
    gap: 8px; 
    flex-wrap: wrap;
  }
  .modal .link.primary {
    background: var(--accent);
    color: var(--fg);
    border-color: var(--accent);
  }
  .modal .link.primary:hover:not(:disabled) {
    background: var(--bg-hover);
    opacity: 0.9;
  }
  .modal .link:disabled,
  .modal .close:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .modal .link { 
    border: 1px solid var(--border); 
    background: var(--bg); 
    color: var(--fg); 
    border-radius: 8px; 
    padding: 6px 10px; 
    text-decoration: none; 
    display: inline-flex; 
    align-items: center; 
    cursor: pointer;
  }
  .modal .link:hover {
    background: var(--bg-hover);
    border-color: var(--accent);
  }
  .modal .close { 
    border: 1px solid var(--border); 
    background: var(--bg); 
    color: var(--fg); 
    border-radius: 8px; 
    padding: 6px 10px; 
    cursor: pointer; 
  }
  .modal .close:hover {
    background: var(--bg-hover);
    border-color: var(--accent);
  }
</style>
