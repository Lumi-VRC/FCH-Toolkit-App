<script lang="ts">
  import Sidebar from '$lib/components/Sidebar.svelte';
  import InstanceMonitor from './instance-monitor/+page.svelte';
  import LogExplorer from './log-explorer/+page.svelte';
  import DatabasePage from './database/+page.svelte';
  import SettingsPanel from './settings/+page.svelte';
  import { onMount } from 'svelte';
  let collapsed = $state(true);
  let activeIndex = $state(0);

  function toggleSidebar() { collapsed = !collapsed; }
  function selectTab(i: number) { activeIndex = i; }

  const tabTitles = [
    'Dashboard', 'Instance Monitor', 'Database', 'Log Explorer', 'World Moderation', 'Settings', 'About'
  ];

  onMount(async () => {
    // watcher no longer auto-starts; starts when Instance Monitor mounts
  });
</script>

<div class="app">
  <Sidebar {collapsed} {activeIndex} onToggle={toggleSidebar} onSelect={selectTab} />

  <main>
    <header>
      <h1>{tabTitles[activeIndex]}</h1>
    </header>
    <section class="content">
      {#if activeIndex === 1}
        {#key activeIndex}
          <InstanceMonitor/>
        {/key}
      {:else if activeIndex === 2}
        {#key activeIndex}
          <DatabasePage/>
        {/key}
      {:else if activeIndex === 3}
        {#key activeIndex}
          <LogExplorer/>
        {/key}
      {:else if activeIndex === 5}
        {#key activeIndex}
          <SettingsPanel/>
        {/key}
      {:else}
        <div class="placeholder">
          <p>Content for <strong>{tabTitles[activeIndex]}</strong> will appear here.</p>
        </div>
      {/if}
    </section>
  </main>
  
  <div style="display:none"><slot /></div>
</div>

<style>
  :global(html, body, #app) { height: 100%; }
  :global(body) { margin: 0; background: var(--bg); color: var(--fg); font-family: system-ui, Segoe UI, Roboto, Arial, sans-serif; }
  :global(*), :global(*::before), :global(*::after) { box-sizing: border-box; }

  .app {
    display: grid;
    grid-template-columns: auto 1fr;
    grid-template-rows: 100vh;
    width: 100vw;
    height: 100vh;
    overflow: hidden;
    background: var(--bg);
  }

  main { display: flex; flex-direction: column; min-width: 0; }
  header {
    display: flex; align-items: center; justify-content: space-between;
    padding: 14px 18px; border-bottom: 1px solid var(--border); background: var(--bg-elev);
  }
  header h1 { margin: 0; font-size: 15px; font-weight: 600; color: var(--fg); }

  .content { flex: 1; overflow: auto; padding: 16px; }

  .placeholder {
    margin-top: 16px;
    padding: 16px;
    border: 1px dashed var(--border);
    border-radius: 12px;
    background: linear-gradient(180deg, rgba(255,255,255,0.02), rgba(0,0,0,0.02));
    color: var(--fg-muted);
  }

  @media (max-width: 900px) {
    .app { grid-template-columns: auto 1fr; }
  }
</style>

