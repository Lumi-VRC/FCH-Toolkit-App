<script lang="ts">
  import SidebarButton from './SidebarButton.svelte';
  export let collapsed: boolean = false;
  export let activeIndex: number = 0;
  export let onToggle: () => void = () => {};
  export let onSelect: (index: number) => void = () => {};

  const labels = [
    'Dashboard', 'Instance Monitor', 'Database', 'Log Explorer', 'World Moderation', 'Settings', 'About'
  ];

  async function checkForUpdates() {
    try {
      const { check } = await import('@tauri-apps/plugin-updater');
      const update = await check();
      if (update?.available) {
        await update.download(() => {});
        await update.install();
      } else {
        // no-op; could show a toast
      }
    } catch (e) {
      console.error('Update check failed', e);
    }
  }
</script>

<aside class:collapsed>
  <div class="top">
    <button class="collapse" onclick={onToggle} aria-label="Toggle sidebar">
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
        icon={(i === 1) ? 'list' : (i === 2) ? 'vault' : (i === 3) ? 'search' : (i === 5) ? 'gear' : 'dot'}
      />
    {/each}
  </div>
  <div class="bottom">
    <button class="update" onclick={checkForUpdates} title="Check for updates" aria-label="Check for updates">
      <span class="icon" aria-hidden="true">↑</span>
      {#if !collapsed}<span class="text">Check for updates</span>{/if}
    </button>
  </div>
</aside>

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
  .icon { display: inline-flex; }
  .text { font-size: 12px; font-weight: 600; }
</style>

