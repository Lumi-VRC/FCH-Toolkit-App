<script lang="ts">
  export let selected: boolean = false;
  export let title: string = "";
  export let onClick: () => void = () => {};
  export let showLabel: boolean = false;
  export let label: string = "";
export let icon: 'dot' | 'list' | 'search' | 'gear' | 'vault' | 'bug' | 'globe' = 'dot';
</script>

<button class:selected class:center={!showLabel} onclick={onClick} class="btn" aria-pressed={selected} {title}>
  <span class="icon" aria-hidden="true">
    {#if icon === 'list'}
      <!-- list icon -->
      <svg width="20" height="20" viewBox="0 0 20 20" fill="none" xmlns="http://www.w3.org/2000/svg">
        <path d="M4 6h12M4 10h12M4 14h12" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
      </svg>
    {:else if icon === 'search'}
      <!-- magnifying glass icon -->
      <svg width="20" height="20" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
        <path d="M21 21l-3.8-3.8m0 0A7.2 7.2 0 104.2 4.2a7.2 7.2 0 0013 13z" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
      </svg>
    {:else if icon === 'gear'}
      <!-- gear icon -->
      <svg width="20" height="20" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
        <path d="M12 8.5a3.5 3.5 0 110 7 3.5 3.5 0 010-7z" stroke="currentColor" stroke-width="1.5"/>
        <path d="M4 12h2m12 0h2M12 4v2m0 12v2M6.5 6.5l1.4 1.4m8.2 8.2l1.4 1.4M17.5 6.5l-1.4 1.4M8.1 16.1l-1.4 1.4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
      </svg>
    {:else if icon === 'vault'}
      <!-- vault (safe) icon -->
      <svg width="20" height="20" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
        <rect x="4" y="5" width="16" height="14" rx="2" stroke="currentColor" stroke-width="1.5"/>
        <circle cx="12" cy="12" r="3" stroke="currentColor" stroke-width="1.5"/>
        <path d="M12 9v2M12 12l1.5 1.5M12 12l-1.5 1.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
        <path d="M6 19v2M18 19v2" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
      </svg>
    {:else if icon === 'bug'}
      <svg width="20" height="20" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
        <path d="M12 6c-1.657 0-3 1.343-3 3v6a3 3 0 006 0V9c0-1.657-1.343-3-3-3z" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/>
        <path d="M9 9H7.5a1.5 1.5 0 010-3H9m6 3h1.5a1.5 1.5 0 100-3H15" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
        <path d="M6 12h2m8 0h2M5 16h3m8 0h3M6.5 8L5 6m13 2l1.5-2" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
        <path d="M12 6V4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
      </svg>
    {:else if icon === 'globe'}
      <svg width="20" height="20" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
        <circle cx="12" cy="12" r="9" stroke="currentColor" stroke-width="1.5" />
        <path d="M12 3v18" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
        <path d="M3 12h18" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
        <path d="M5.5 7c1.8 1.2 3.9 1.8 6.5 1.8S16.2 8.2 18 7" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
        <path d="M5.5 17c1.8-1.2 3.9-1.8 6.5-1.8S16.2 15.8 18 17" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
      </svg>
    {:else}
      <!-- default dot -->
      <svg width="20" height="20" viewBox="0 0 20 20" fill="none" xmlns="http://www.w3.org/2000/svg">
        <circle cx="10" cy="10" r="5" fill="currentColor" />
      </svg>
    {/if}
  </span>
  {#if showLabel}
    <span class="text">{label || title}</span>
  {/if}
</button>

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

  .btn {
    width: 100%;
    height: 48px;
    display: inline-flex;
    align-items: center;
    justify-content: flex-start;
    gap: 10px;
    background: var(--bg-elev);
    border: 1px solid var(--border);
    border-radius: 10px;
    color: var(--fg);
    cursor: pointer;
    transition: background 120ms ease, border-color 120ms ease, transform 80ms ease;
    padding: 0 10px;
  }

  .btn:hover { background: var(--bg-hover); border-color: var(--accent); }
  .btn:active { transform: scale(0.98); }

  .btn.selected { border-color: var(--fg-muted); }
  .btn.center { justify-content: center; padding: 0; gap: 0; }

  .icon { display: inline-flex; color: var(--fg-muted); align-items: center; justify-content: center; }
  .text { color: var(--fg); font-size: 12px; font-weight: 600; opacity: 0.95; }
</style>

