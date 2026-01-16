<script lang="ts">
  import { onMount } from 'svelte';

  let currentVersion = $state('');

  async function loadVersion() {
    try {
      const res = await fetch('/version.json', { cache: 'no-store' });
      if (res.ok) {
        const data = await res.json();
        if (data && typeof data.version === 'string') {
          currentVersion = data.version;
        }
      }
    } catch (err) {
      console.error('Failed to load version:', err);
    }
  }

  onMount(() => {
    loadVersion();
  });
</script>

<div class="panel">
  <div class="about-header">
    <img src="/IconNOBG.png" alt="FCH Toolkit" class="about-icon" />
    <div class="about-title-section">
      <div class="title-row">
        <h2>About FCH App</h2>
        {#if currentVersion}
          <span class="version-pill">v{currentVersion}</span>
        {/if}
      </div>
      <p class="intro">
        FCH App is a desktop application for monitoring and moderating VRChat instances. 
        This page provides an overview of each feature and how it works.
      </p>
    </div>
  </div>

  <div class="tabs-grid">
    <!-- Login Tab -->
    <div class="tab-panel">
      <h3>Login</h3>
      <div class="panel-content">
        <p class="description">Group authentication token management for accessing group watchlist features.</p>
        <div class="features">
          <h4>Features:</h4>
          <ul>
            <li>Add group access tokens (64-character hexadecimal)</li>
            <li>Validate tokens against the backend API</li>
            <li>View all stored group tokens with names</li>
            <li>Remove tokens for groups you no longer manage</li>
          </ul>
        </div>
        <div class="how-it-works">
          <h4>How to use:</h4>
          <p>
            Enter your 64-character group access token in the input field and click "Add Token". 
            The app will verify the token and display the group name if valid. You can add multiple 
            tokens for different groups you manage. Once added, these tokens allow the app to check 
            users against your group watchlists automatically. To remove a token, click the "Remove" 
            button next to the group name.
          </p>
        </div>
      </div>
    </div>

    <!-- Instance Monitor Tab -->
    <div class="tab-panel">
      <h3>Instance Monitor</h3>
      <div class="panel-content">
        <p class="description">Real-time monitoring of players joining and leaving your VRChat instance.</p>
        <div class="features">
          <h4>Features:</h4>
          <ul>
            <li>Live player list with avatars and user IDs</li>
            <li>Group watchlist integration with colored indicators</li>
            <li>Local watchlist notifications</li>
            <li>User notes and custom sounds</li>
            <li>Batched user lookups for performance</li>
            <li>Moderation history display (warns, kicks, bans)</li>
          </ul>
        </div>
        <div class="how-it-works">
          <h4>How to use:</h4>
          <p>
            When you join a VRChat instance, the app automatically detects players as they join. 
            Users on your group watchlists will appear with colored pills next to their names - 
            red means notifications are enabled, yellow means notifications are off. Their avatar 
            will also glow with the same color. Click the pill to see which groups have them 
            watchlisted, view any notes, and see their moderation history (warns, kicks, bans). 
            You can also click the eye icon to enable local notifications for specific users, or 
            the note icon to add personal notes about them.
          </p>
        </div>
      </div>
    </div>

    <!-- Database Tab -->
    <div class="tab-panel">
      <h3>Database</h3>
      <div class="panel-content">
        <p class="description">Manage user notes, watchlist entries, and custom notification sounds.</p>
        <div class="features">
          <h4>Features:</h4>
          <ul>
            <li>Add/edit notes for specific users</li>
            <li>Toggle watchlist status (local notifications)</li>
            <li>Set custom notification sounds per user</li>
            <li>Search and filter users</li>
            <li>View watchlist-only entries</li>
            <li>Delete user entries</li>
          </ul>
        </div>
        <div class="how-it-works">
          <h4>How to use:</h4>
          <p>
            Search for users by their user ID or username, or filter to show only watchlisted users. 
            Click on a user entry to expand it and view their notes. Click "Edit Note" to add or 
            modify notes about the user. Toggle the watchlist checkbox to enable/disable local 
            notifications when they join. Set a custom sound path to play a specific sound file 
            when this user joins (overrides default sounds). Click "Delete User" to remove all 
            stored data for a user. Use the "Add User" button at the top to manually add a new 
            entry before they join your instance.
          </p>
        </div>
      </div>
    </div>

    <!-- Log Explorer Tab -->
    <div class="tab-panel">
      <h3>Log Explorer</h3>
      <div class="panel-content">
        <p class="description">Real-time VRChat log file viewer with search capabilities.</p>
        <div class="features">
          <h4>Features:</h4>
          <ul>
            <li>Live log streaming from VRChat log files</li>
            <li>Full-text search with highlighting</li>
            <li>Navigate between search matches</li>
            <li>Auto-scroll to latest entries</li>
            <li>Manual log file refresh</li>
          </ul>
        </div>
        <div class="how-it-works">
          <h4>How to use:</h4>
          <p>
            The log viewer automatically displays VRChat log entries as they're written. Scroll 
            through the log to see recent activity. Use the search box to find specific text - 
            matching lines will be highlighted and you can navigate between matches using the 
            arrow buttons. Toggle "Auto-scroll" to automatically jump to the latest log entries. 
            The app automatically switches to new log files when VRChat creates them, so you'll 
            always see the most recent logs.
          </p>
        </div>
      </div>
    </div>

    <!-- World Moderation Tab -->
    <div class="tab-panel">
      <h3>World Moderation</h3>
      <div class="panel-content">
        <p class="description">View and search moderation actions (bans, warns, kicks) from your groups.</p>
        <div class="features">
          <h4>Features:</h4>
          <ul>
            <li>View all ban/warn log entries</li>
            <li>Search by admin, target, or reason</li>
            <li>Filter by action type (ban/warn)</li>
            <li>View timestamps and details</li>
          </ul>
        </div>
        <div class="how-it-works">
          <h4>How to use:</h4>
          <p>
            View all ban and warn actions from your groups in chronological order. Each entry shows 
            who performed the action, who it was performed on, the reason, and when it happened. 
            Use the search box to find specific entries by admin name, target user, or reason text. 
            The search updates automatically as you type, filtering the displayed entries. This 
            helps you review moderation history, check if someone has been warned before, or find 
            specific ban reasons.
          </p>
        </div>
      </div>
    </div>

    <!-- Settings Tab -->
    <div class="tab-panel">
      <h3>Settings</h3>
      <div class="panel-content">
        <p class="description">Configure notification sounds and volume levels.</p>
        <div class="features">
          <h4>Features:</h4>
          <ul>
            <li>Master volume control (affects all sounds)</li>
            <li>Group notification sound and volume</li>
            <li>Local notification sound and volume</li>
            <li>Browse for custom sound files</li>
            <li>Preview sounds before saving</li>
          </ul>
        </div>
        <div class="how-it-works">
          <h4>How to use:</h4>
          <p>
            Adjust the master volume slider to control overall sound levels. For group and local 
            notifications, set a default sound file by clicking "Browse" and selecting an audio 
            file, then adjust the volume slider for that notification type. Click "Play" to test 
            how the sound will sound at the current volume settings. If you don't set a custom 
            sound, Windows system sounds will be used instead. Master volume affects all sounds, 
            while individual notification volumes let you fine-tune each type. Custom sounds set 
            for specific users in the Database tab will override these defaults.
          </p>
        </div>
      </div>
    </div>

    <!-- Debug Tab -->
    <div class="tab-panel">
      <h3>Debug</h3>
      <div class="panel-content">
        <p class="description">View application debug logs and system information.</p>
        <div class="features">
          <h4>Features:</h4>
          <ul>
            <li>Real-time debug log display</li>
            <li>Filter by log level (info, warn, error, debug)</li>
            <li>Filter by source (frontend, backend, etc.)</li>
            <li>Text search in logs</li>
            <li>Auto-scroll to latest entries</li>
            <li>Clear log history</li>
          </ul>
        </div>
        <div class="how-it-works">
          <h4>How to use:</h4>
          <p>
            View all debug messages from the current session in real-time. Use the filter dropdowns 
            to show only specific log levels (info, warn, error, debug) or sources (frontend, backend). 
            Type in the search box to find specific text in the logs. The log automatically scrolls 
            to show the latest entries, but you can scroll up to review older messages. Click "Clear 
            Log" to start fresh. This is useful for troubleshooting issues or understanding what 
            the app is doing behind the scenes.
          </p>
        </div>
      </div>
    </div>
  </div>
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 24px;
    padding: 20px;
  }

  .about-header {
    display: flex;
    align-items: flex-start;
    gap: 24px;
    margin-bottom: 8px;
  }

  .about-icon {
    width: 120px;
    height: 120px;
    object-fit: contain;
    flex-shrink: 0;
  }

  .about-title-section {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .title-row {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }

  h2 {
    margin: 0;
    font-size: 28px;
    font-weight: 600;
    color: var(--fg);
  }

  .version-pill {
    display: inline-flex;
    align-items: center;
    padding: 4px 12px;
    background: var(--bg-elev);
    border: 1px solid var(--border);
    border-radius: 12px;
    font-size: 12px;
    font-weight: 600;
    color: var(--fg-muted);
    white-space: nowrap;
  }

  .intro {
    color: var(--fg-muted);
    font-size: 15px;
    line-height: 1.6;
    margin: 0;
  }

  .tabs-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(350px, 1fr));
    gap: 20px;
  }

  .tab-panel {
    background: var(--bg-elev);
    border: 1px solid var(--border);
    border-radius: 12px;
    padding: 20px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .tab-panel h3 {
    margin: 0;
    font-size: 20px;
    font-weight: 600;
    color: var(--fg);
    border-bottom: 2px solid var(--accent);
    padding-bottom: 8px;
  }

  .panel-content {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .description {
    color: var(--fg);
    font-size: 14px;
    line-height: 1.6;
    margin: 0;
    font-weight: 500;
  }

  .features,
  .how-it-works {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .features h4,
  .how-it-works h4 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    color: var(--fg);
  }

  .features ul {
    margin: 0;
    padding-left: 20px;
    color: var(--fg-muted);
    font-size: 13px;
    line-height: 1.8;
  }

  .features li {
    margin-bottom: 4px;
  }

  .how-it-works p {
    margin: 0;
    color: var(--fg-muted);
    font-size: 13px;
    line-height: 1.7;
  }
</style>
