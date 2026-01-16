<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import LoginPage from './+page.svelte';
  import InstanceMonitor from './instance-monitor/+page.svelte';
  import DatabasePage from './database/+page.svelte';
  import LogExplorer from './log-explorer/+page.svelte';
  import WorldModeration from './world-moderation/+page.svelte';
  import SettingsPanel from './settings/+page.svelte';
  import DebugPanel from './debug/+page.svelte';
  import AboutPage from './about/+page.svelte';
  import { isLoading, loadingMessage } from '$lib/stores/loading';
  import { pushDebug } from '$lib/stores/debugLog';

  let collapsed = $state(true);
  let activeIndex = $state(0);
  let mouseX = $state(0.5); // Normalized 0-1
  let mouseY = $state(0.5); // Normalized 0-1
  let appElement;
  let petalInterval = null;
  let isAppLoading = $state(true);
  let currentLoadingMessage = $state('Initializing...');
  let showWelcome = $state(false);
  let welcomeFading = $state(false);
  let previousLoadingState = true;

  // Subscribe to loading store
  const unsubscribeLoading = isLoading.subscribe(value => {
    const wasLoading = previousLoadingState;
    previousLoadingState = value;
    isAppLoading = value;
    
    // When loading ends, show welcome screen
    if (wasLoading && !value) {
      showWelcome = true;
      welcomeFading = false;
      
      // Start fade out after 2 seconds
      setTimeout(() => {
        welcomeFading = true;
        // Remove welcome screen after fade completes
        setTimeout(() => {
          showWelcome = false;
        }, 2000); // Match fade duration
      }, 2000);
    }
  });
  
  const unsubscribeMessage = loadingMessage.subscribe(value => {
    currentLoadingMessage = value;
  });

  const tabs = [
    { title: 'Login', component: LoginPage },
    { title: 'Instance Monitor', component: InstanceMonitor },
    { title: 'Database', component: DatabasePage },
    { title: 'Log Explorer', component: LogExplorer },
    { title: 'World Moderation', component: WorldModeration },
    { title: 'Settings', component: SettingsPanel },
    { title: 'Debug', component: DebugPanel },
    { title: 'About', component: AboutPage }
  ];

  const tabTitles = tabs.map((tab) => tab.title);

  function toggleSidebar() {
    collapsed = !collapsed;
  }

  function selectTab(i) {
    activeIndex = i;
  }

  function handleMouseMove(e) {
    if (!appElement) return;
    
    const rect = appElement.getBoundingClientRect();
    // Normalize mouse position to 0-1 range
    mouseX = (e.clientX - rect.left) / rect.width;
    mouseY = (e.clientY - rect.top) / rect.height;
    
    // Update CSS custom properties for parallax
    appElement.style.setProperty('--mouse-x', mouseX.toString());
    appElement.style.setProperty('--mouse-y', mouseY.toString());
  }

  function isOverUIElement(x, y) {
    // Check if point is over UI elements (sidebar, header, content areas)
    if (!appElement) return false;
    
    const sidebar = appElement.querySelector('aside, [class*="sidebar"]');
    const header = appElement.querySelector('header');
    const main = appElement.querySelector('main');
    
    // Check sidebar
    if (sidebar) {
      const rect = sidebar.getBoundingClientRect();
      if (x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom) {
        return true;
      }
    }
    
    // Check header
    if (header) {
      const rect = header.getBoundingClientRect();
      if (x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom) {
        return true;
      }
    }
    
    // Check main content area (but not the background)
    if (main) {
      const rect = main.getBoundingClientRect();
      if (x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom) {
        return true;
      }
    }
    
    return false;
  }

  function updatePetalOpacity(petal) {
    const rect = petal.getBoundingClientRect();
    const centerX = rect.left + rect.width / 2;
    const centerY = rect.top + rect.height / 2;
    
    if (isOverUIElement(centerX, centerY)) {
      petal.style.opacity = '0.1';
    } else {
      petal.style.opacity = '0.6';
    }
  }

  function createPetal() {
    const petal = document.createElement('div');
    petal.className = 'sakura-petal';

    const startX = Math.random() * window.innerWidth;
    petal.style.left = `${startX}px`;

    const initialRotate = Math.random() * 360;
    petal.style.setProperty('--initialRotate', `${initialRotate}deg`);
    petal.style.transform = `rotate(${initialRotate}deg) scaleX(0.5)`;

    const duration = 5 + Math.random() * 5; // 5 to 10 seconds
    const angleDegrees = (Math.random() * 20) - 10;
    const translateX = Math.tan(angleDegrees * (Math.PI / 180)) * window.innerHeight;
    const deltaRotate = (Math.random() * 180) - 90;

    petal.style.setProperty('--translateX', `${translateX}px`);
    petal.style.setProperty('--deltaRotate', `${deltaRotate}deg`);
    petal.style.animation = `fall ${duration}s linear forwards`;
    petal.style.opacity = '0.6'; // Default opacity when over background

    // Update opacity periodically as petal falls
    const opacityInterval = setInterval(() => {
      if (petal.parentNode) {
        updatePetalOpacity(petal);
      } else {
        clearInterval(opacityInterval);
      }
    }, 100); // Check every 100ms

    petal.addEventListener('animationend', () => {
      clearInterval(opacityInterval);
      petal.remove();
    });

    document.body.appendChild(petal);
    // Initial opacity check
    setTimeout(() => updatePetalOpacity(petal), 10);
  }

  onMount(async () => {
    // Initialize debug logging
    pushDebug('[App] Application starting...', undefined, 'info', 'frontend');
    
    // Initialize loading state
    isLoading.set(true);
    loadingMessage.set('Starting application...');
    
    // Small delay to ensure UI is rendered
    await new Promise(resolve => setTimeout(resolve, 100));
    
    if (appElement) {
      appElement.addEventListener('mousemove', handleMouseMove);
    }
    
    // Create a petal every 500ms
    petalInterval = setInterval(createPetal, 500);
    
    // Fallback: dismiss loading screen after maximum wait time (10 seconds)
    // This ensures the app doesn't stay frozen if something goes wrong
    // Increased from 3s to 10s to allow retroactive log scan to complete
    setTimeout(() => {
      if (isAppLoading) {
        console.warn('[Loading] Fallback timeout reached, dismissing loading screen');
        isLoading.set(false);
        // Force a re-render by updating state
        isAppLoading = false;
      }
    }, 10000);
    
    // Wait a bit for initial setup, then allow components to signal completion
    // The loading will be dismissed by individual components when they're ready
    setTimeout(() => {
      // Minimum loading time to prevent flash
      if (isAppLoading) {
        loadingMessage.set('Loading components...');
      }
    }, 300);
  });

  onDestroy(() => {
    if (appElement) {
      appElement.removeEventListener('mousemove', handleMouseMove);
    }
    if (petalInterval) {
      clearInterval(petalInterval);
    }
    unsubscribeLoading();
    unsubscribeMessage();
  });
</script>

<svelte:head>
  <style>
    @keyframes fall {
      to {
        transform: translateY(105vh) translateX(var(--translateX)) rotate(calc(var(--initialRotate) + var(--deltaRotate))) scaleX(0.5);
        opacity: 0;
      }
    }
  </style>
</svelte:head>

<div class="app" bind:this={appElement}>
  {#if isAppLoading}
    <div class="loading-overlay">
      <div class="loading-content">
        <div class="loading-spinner"></div>
        <div class="loading-text">{currentLoadingMessage}</div>
      </div>
    </div>
  {/if}

  {#if showWelcome}
    <div class="welcome-overlay" class:fading={welcomeFading}>
      <div class="welcome-content">
        <img src="/IconNOBG.png" alt="FCH Toolkit" class="welcome-icon" />
        <div class="welcome-text">Welcome to FCH Toolkit</div>
      </div>
    </div>
  {/if}

  <Sidebar {collapsed} {activeIndex} onToggle={toggleSidebar} onSelect={selectTab} />

  <main>
    <header>
      <h1>{tabTitles[activeIndex]}</h1>
    </header>
    <section class="content">
      {#each tabs as tab, index}
        <div
          class="tab"
          class:active={activeIndex === index}
          aria-hidden={activeIndex === index ? undefined : 'true'}
        >
          {#if tab.component}
            {@const C = tab.component}
            <C />
          {:else}
            <div class="placeholder">
              <p>Content for <strong>{tab.title}</strong> will appear here.</p>
            </div>
          {/if}
        </div>
      {/each}
    </section>
  </main>
</div>

<style>
  :global(html, body, #app) { height: 100%; }
  :global(body) { margin: 0; background: var(--bg); color: var(--fg); font-family: system-ui, Segoe UI, Roboto, Arial, sans-serif; }
  :global(*), :global(*::before), :global(*::after) { box-sizing: border-box; }

  /* CSS Variables - Dark gray theme */
  :global(:root) {
    --bg: #121212;
    --bg-elev: #1a1a1a;
    --bg-hover: #222222;
    --fg: #e0e0e0;
    --fg-muted: #999999;
    --accent: #ffb6c1;
    --border: #333333;
  }

  /* Custom scrollbar styling */
  :global(::-webkit-scrollbar) {
    width: 12px;
    height: 12px;
  }

  :global(::-webkit-scrollbar-track) {
    background: #1a1a1a;
    border: 1px solid #ffb6c1;
  }

  :global(::-webkit-scrollbar-thumb) {
    background: #ffb6c1;
    border-radius: 6px;
    border: 1px solid #ffb6c1;
  }

  :global(::-webkit-scrollbar-thumb:hover) {
    background: #ff9fb8;
  }

  /* Firefox scrollbar styling */
  :global(*) {
    scrollbar-width: thin;
    scrollbar-color: #ffb6c1 #1a1a1a;
  }

  /* Hoverable buttons and search bars - faint glow effect */
  :global(button:not(:disabled)),
  :global(input[type="text"]),
  :global(input[type="search"]),
  :global(textarea),
  :global(select),
  :global(.search-input),
  :global(.control-btn),
  :global(.watch-btn),
  :global(.note-btn),
  :global(.toggle),
  :global([role="button"]:not(:disabled)) {
    box-shadow: 0 0 2px rgba(255, 182, 193, 0.4);
    transition: box-shadow 0.2s ease;
  }

  /* Extended glow on focus/click */
  :global(button:not(:disabled):focus),
  :global(button:not(:disabled):active),
  :global(input[type="text"]:focus),
  :global(input[type="search"]:focus),
  :global(textarea:focus),
  :global(select:focus),
  :global(.search-input:focus),
  :global(.control-btn:focus),
  :global(.control-btn:active),
  :global(.watch-btn:focus),
  :global(.watch-btn:active),
  :global(.note-btn:focus),
  :global(.note-btn:active),
  :global(.toggle:focus),
  :global(.toggle:active),
  :global([role="button"]:not(:disabled):focus),
  :global([role="button"]:not(:disabled):active) {
    box-shadow: 0 0 6px rgba(255, 182, 193, 0.6);
    outline: none;
  }

  .app {
    display: grid;
    grid-template-columns: auto 1fr;
    grid-template-rows: 100vh;
    width: 100vw;
    height: 100vh;
    overflow: hidden;
    position: relative;
    background: var(--bg);
  }

  .app::before {
    content: '';
    position: absolute;
    inset: -10%; /* Extend beyond edges to allow movement */
    background-image: url('/sakurabg.png');
    background-size: 120%; /* Larger than container to allow parallax movement */
    background-position: center;
    background-repeat: no-repeat;
    filter: brightness(0.4);
    z-index: 0;
    /* Parallax effect based on mouse position (inverted) */
    transform: translate(
      calc((0.5 - var(--mouse-x, 0.5)) * 20px),
      calc((0.5 - var(--mouse-y, 0.5)) * 20px)
    );
    transition: transform 0.1s ease-out;
    will-change: transform;
  }

  .app::after {
    content: '';
    position: absolute;
    inset: 0;
    background: radial-gradient(ellipse at center, transparent 0%, rgba(0, 0, 0, 0.6) 100%);
    z-index: 1;
    pointer-events: none;
  }

  /* Falling sakura petals */
  :global(.sakura-petal) {
    position: fixed;
    top: -20px;
    width: 10px;
    height: 10px;
    background-color: rgba(255, 182, 193, 0.9);
    border-radius: 50%;
    pointer-events: none;
    z-index: 1; /* Above background/vignette (0-1), below content (2) */
    opacity: 0.6; /* Default opacity, dynamically adjusted by JS */
    transition: opacity 0.2s ease;
  }

  main { display: flex; flex-direction: column; min-width: 0; position: relative; z-index: 2; }
  header {
    display: flex; align-items: center; justify-content: space-between;
    padding: 14px 18px; border-bottom: 1px solid var(--border); 
    background: rgba(26, 26, 26, 0.85);
    backdrop-filter: blur(8px);
  }
  header h1 { margin: 0; font-size: 15px; font-weight: 600; color: var(--fg); }

  .content { flex: 1; overflow: hidden; padding: 16px; position: relative; }
  .tab { display: none; height: 100%; overflow: auto; }
  .tab.active { display: block; }
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

  /* Loading overlay */
  .loading-overlay {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(18, 18, 18, 0.95);
    backdrop-filter: blur(8px);
    z-index: 10000;
    display: flex;
    align-items: center;
    justify-content: center;
    pointer-events: all;
    user-select: none;
    -webkit-user-select: none;
  }

  .loading-content {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 24px;
  }

  .loading-spinner {
    width: 48px;
    height: 48px;
    border: 3px solid rgba(255, 182, 193, 0.2);
    border-top-color: #ffb6c1;
    border-radius: 50%;
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .loading-text {
    color: var(--fg);
    font-size: 14px;
    font-weight: 500;
    text-align: center;
  }

  /* Welcome overlay */
  .welcome-overlay {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(26, 26, 26, 0.85);
    backdrop-filter: blur(8px);
    z-index: 9999;
    display: flex;
    align-items: center;
    justify-content: center;
    pointer-events: all;
    user-select: none;
    -webkit-user-select: none;
    opacity: 1;
    transition: opacity 2s ease-out;
  }

  .welcome-overlay.fading {
    opacity: 0;
  }

  .welcome-content {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 24px;
  }

  .welcome-icon {
    width: 256px;
    height: 256px;
    object-fit: contain;
    animation: welcome-fade-in 0.5s ease-out;
  }

  .welcome-text {
    color: var(--fg);
    font-size: 28px;
    font-weight: 600;
    text-align: center;
    animation: welcome-fade-in 0.5s ease-out 0.2s both;
  }

  @keyframes welcome-fade-in {
    from {
      opacity: 0;
      transform: translateY(10px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
</style>
