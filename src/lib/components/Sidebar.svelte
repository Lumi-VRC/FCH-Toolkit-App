<script lang="ts">
  import SidebarButton from './SidebarButton.svelte';
  import { onMount } from 'svelte';
  let { collapsed = false, activeIndex = 0, onToggle = () => {}, onSelect = (_: number) => {} } = $props();

  const labels = [
    'Login', 'Instance Monitor', 'Database', 'Log Explorer', 'World Moderation', 'Settings', 'Debug', 'About'
  ];

  let updateAvailable = $state(false);
  let checking = $state(false);
  let showUpdateModal = $state(false);

  async function getLocalVersion(): Promise<string | null> {
    try {
      const res = await fetch('/version.json', { cache: 'no-store' });
      if (!res.ok) return null;
      const data = await res.json();
      return (data && typeof data.version === 'string') ? data.version : null;
    } catch { return null; }
  }

  async function getGithubLatestVersion(): Promise<string | null> {
    try {
      const res = await fetch('https://api.github.com/repos/Lumi-VRC/FCH-Toolkit-App/releases/latest', { headers: { 'Accept': 'application/vnd.github+json' } });
      if (!res.ok) return null;
      const data = await res.json();
      const tag = (data && (data.tag_name || data.name)) ? String(data.tag_name || data.name) : '';
      // normalize like v0.1.2 -> 0.1.2
      return tag.replace(/^v/i, '').trim() || null;
    } catch { return null; }
  }

  function cmpSemver(a: string, b: string): number {
    const pa = a.split('.').map(x => parseInt(x || '0', 10));
    const pb = b.split('.').map(x => parseInt(x || '0', 10));
    for (let i=0;i<Math.max(pa.length, pb.length);i++) {
      const da = pa[i] || 0, db = pb[i] || 0;
      if (da > db) return 1; if (da < db) return -1;
    }
    return 0;
  }

  async function runUpdateCheck(): Promise<void> {
    if (checking) return; checking = true;
    try {
      const [localV, remoteV] = await Promise.all([getLocalVersion(), getGithubLatestVersion()]);
      updateAvailable = !!(localV && remoteV && cmpSemver(remoteV, localV) > 0);
    } finally { checking = false; }
  }

  // on startup (client), check once
  onMount(() => { runUpdateCheck(); });

  async function onUpdateButtonClick(_e: MouseEvent) {
    await runUpdateCheck();
    if (updateAvailable) { showUpdateModal = true; }
    else {
      try {
        const plugin = await import('@tauri-apps/plugin-opener');
        const fn: any = (plugin as any).open || (plugin as any).default || (plugin as any);
        if (typeof fn === 'function') await fn('https://github.com/Lumi-VRC/FCH-Toolkit-App/releases/latest');
      } catch {}
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
        icon={(i === 1)
          ? 'list'
          : (i === 2)
          ? 'vault'
          : (i === 3)
          ? 'search'
          : (i === 4)
          ? 'globe'
          : (i === 5)
          ? 'gear'
          : (i === 6)
          ? 'bug'
          : 'dot'}
      />
    {/each}
  </div>
  <div class="bottom">
    <button class="update" class:pulse={updateAvailable} onclick={onUpdateButtonClick} title={updateAvailable ? 'Update Available' : 'Check for updates'} aria-label="Check for updates">
      <span class="icon" aria-hidden="true">↑</span>
      {#if !collapsed}<span class="text">{updateAvailable ? 'Update Available' : 'Check for updates'}</span>{/if}
    </button>
  </div>
</aside>

{#if showUpdateModal}
  <div class="modal-backdrop" role="dialog" aria-modal="true" aria-label="Update Available">
    <div class="modal">
      <div class="modal-title">Navigate to this link and download the latest release.</div>
      <div class="modal-actions">
        <a class="link" href="https://github.com/Lumi-VRC/FCH-Toolkit-App/releases/latest" target="_blank" rel="noopener noreferrer">Open Releases</a>
        <button class="close" onclick={() => showUpdateModal = false}>Close</button>
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
    background: var(--bg-elev);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 12px;
    transition: width 150ms ease, min-width 150ms ease;
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
    display: inline-flex; align-items: center; justify-content: center; gap: 8px;
    background: var(--bg);
    color: var(--fg);
    border: 1px solid var(--border);
    border-radius: 8px;
    cursor: pointer;
  }
  .update:hover { background: var(--bg-hover); border-color: var(--accent); }
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

  .modal-backdrop { position: fixed; inset: 0; background: rgba(0,0,0,0.6); display: grid; place-items: center; z-index: 9999; }
  .modal { background: var(--bg-elev); border: 1px solid var(--border); border-radius: 12px; padding: 16px; min-width: 280px; max-width: 90vw; }
  .modal-title { font-weight: 600; margin-bottom: 12px; }
  .modal-actions { display: inline-flex; gap: 8px; }
  .modal .link { border: 1px solid var(--border); background: var(--bg); color: var(--fg); border-radius: 8px; padding: 6px 10px; text-decoration: none; display: inline-flex; align-items: center; }
  .modal .close { border: 1px solid var(--border); background: var(--bg); color: var(--fg); border-radius: 8px; padding: 6px 10px; cursor: pointer; }
</style>

