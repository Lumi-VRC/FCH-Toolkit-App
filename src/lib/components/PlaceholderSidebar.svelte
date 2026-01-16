<script>
  import SidebarButton from './SidebarButton.svelte';

  let {
    collapsed = false,
    activeIndex = 0,
    onToggle = () => {},
    onSelect = (_) => {}
  } = $props();

  const labels = [
    'Login',
    'Instance Monitor',
    'Database',
    'Log Explorer',
    'World Moderation',
    'Settings',
    'Debug',
    'About'
  ];

  const iconForIndex = (i) =>
    i === 1 ? 'list'
    : i === 2 ? 'vault'
    : i === 3 ? 'search'
    : i === 4 ? 'globe'
    : i === 5 ? 'gear'
    : i === 6 ? 'bug'
    : 'dot';
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
</style>
