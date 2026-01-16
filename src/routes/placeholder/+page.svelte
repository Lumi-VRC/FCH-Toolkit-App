<script>
  import PlaceholderSidebar from '$lib/components/PlaceholderSidebar.svelte';

  const tabs = [
    { title: 'Login', description: 'Placeholder login panel with no authentication.' },
    { title: 'Instance Monitor', description: 'Placeholder instance monitor with static rows.' },
    { title: 'Database', description: 'Placeholder database view without data sources.' },
    { title: 'Log Explorer', description: 'Placeholder log explorer without files.' },
    { title: 'World Moderation', description: 'Placeholder world moderation actions.' },
    { title: 'Settings', description: 'Placeholder settings toggles.' },
    { title: 'Debug', description: 'Placeholder debug utilities.' },
    { title: 'About', description: 'Placeholder about page.' }
  ];

  const placeholderBlocks = [
    { title: 'Primary Actions', items: ['Action A', 'Action B', 'Action C'], body: 'Describe the primary steps or controls for this panel.' },
    { title: 'Secondary Actions', items: ['Option 1', 'Option 2', 'Option 3'], body: 'Use this area to outline secondary controls or notes.' },
    { title: 'Status & Notes', items: ['Status Light', 'Indicator'], body: 'Any status, note, or info can live here in production.' }
  ];

  let collapsed = false;
  let activeIndex = 0;
</script>

<div class="shell">
  <PlaceholderSidebar
    {collapsed}
    {activeIndex}
    onToggle={() => collapsed = !collapsed}
    onSelect={(i) => activeIndex = i}
  />

  <main class:collapsed={collapsed}>
    <header class="page-header">
      <div class="titles">
        <p class="eyebrow">Placeholder View</p>
        <h1>{tabs[activeIndex].title}</h1>
        <p class="muted">{tabs[activeIndex].description}</p>
      </div>
      <div class="actions">
        <button class="ghost">Primary Placeholder</button>
        <button class="ghost">Secondary Placeholder</button>
      </div>
    </header>

    <section class="grid">
      {#each placeholderBlocks as block}
        <article class="card">
          <div class="card-head">
            <div>
              <div class="eyebrow">{tabs[activeIndex].title}</div>
              <h2>{block.title}</h2>
            </div>
            <button class="ghost small">Placeholder Button</button>
          </div>
          <p class="muted">{block.body}</p>
          <div class="chips">
            {#each block.items as item}
              <span class="chip">{item}</span>
            {/each}
          </div>
        </article>
      {/each}
    </section>
  </main>
</div>

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

  .shell {
    display: flex;
    min-height: 100vh;
    background: var(--bg);
    color: var(--fg);
  }

  main {
    flex: 1;
    padding: 20px;
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  .page-header {
    display: flex;
    justify-content: space-between;
    gap: 16px;
    flex-wrap: wrap;
    align-items: center;
  }

  .titles h1 { margin: 4px 0; }
  .eyebrow { text-transform: uppercase; font-size: 11px; letter-spacing: 0.08em; color: var(--fg-muted); }
  .muted { color: var(--fg-muted); margin: 4px 0 0 0; }

  .actions { display: inline-flex; gap: 8px; }
  .ghost {
    height: 38px;
    padding: 0 12px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--bg-elev);
    color: var(--fg);
    cursor: pointer;
  }
  .ghost:hover { background: var(--bg-hover); border-color: var(--accent); }
  .ghost.small { height: 32px; font-size: 12px; }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
    gap: 16px;
  }

  .card {
    background: var(--bg-elev);
    border: 1px solid var(--border);
    border-radius: 12px;
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .card-head {
    display: flex;
    justify-content: space-between;
    gap: 10px;
    align-items: center;
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .chip {
    padding: 6px 10px;
    border-radius: 999px;
    background: var(--bg);
    border: 1px solid var(--border);
    color: var(--fg);
    font-size: 12px;
  }
</style>
