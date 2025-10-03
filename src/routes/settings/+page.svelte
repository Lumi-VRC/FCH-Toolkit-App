<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  let soundPath = $state<string | null>(null);
  let volume = $state(1.0);
  let groupSoundPath = $state<string | null>(null);
  let groupVolume = $state(1.0);
  let saveTimer: any;

  onMount(async () => {
    try {
      const res: any = await invoke('get_config');
      soundPath = (res && res.soundPath) || null;
      volume = (res && typeof res.soundVolume === 'number') ? res.soundVolume : 1.0;
      groupSoundPath = (res && res.groupSoundPath) || null;
      groupVolume = (res && typeof res.groupSoundVolume === 'number') ? res.groupSoundVolume : 1.0;
    } catch {}
  });

  async function saveNow() {
    try {
      await invoke('set_config', {
        soundPath,
        soundVolume: volume,
        groupSoundPath,
        groupSoundVolume: groupVolume
      });
    } catch {}
  }
  function scheduleSave() {
    clearTimeout(saveTimer);
    saveTimer = setTimeout(saveNow, 300);
  }
  async function browse() {
    try { const res: any = await invoke('browse_sound'); const p = (res && res.path) || null; if (p) { soundPath = p; scheduleSave(); } } catch {}
  }
  async function preview() {
    try { await invoke('preview_watch_sound'); } catch {}
  }

  async function previewGroup() {
    try { await invoke('preview_group_sound'); } catch {}
  }
</script>

<div class="panel">
  <fieldset class="capsule">
    <legend>Watchlist Notification Sound</legend>
    <div class="row">
      <label for="sound">File (WAV/MP3, blank for system sound)</label>
      <div class="row-in">
        <input id="sound" placeholder="C:\\path\\to\\sound.(wav|mp3)" bind:value={soundPath} oninput={scheduleSave} />
        <button onclick={browse}>Browse</button>
        <button onclick={preview}>Preview</button>
      </div>
    </div>
    <div class="row">
      <label for="vol">Volume</label>
      <input id="vol" type="range" min="0" max="1" step="0.01" bind:value={volume} oninput={scheduleSave} />
    </div>
  </fieldset>

  <fieldset class="capsule">
    <legend>Group Watchlist Notification Sound</legend>
    <div class="row">
      <label for="group-sound">File (WAV/MP3, blank for system sound)</label>
      <div class="row-in">
        <input id="group-sound" placeholder="C:\\path\\to\\group.(wav|mp3)" bind:value={groupSoundPath} oninput={scheduleSave} />
        <button onclick={async () => { try { const res: any = await invoke('browse_sound'); const p = (res && res.path) || null; if (p) { groupSoundPath = p; scheduleSave(); } } catch {} }}>Browse</button>
        <button onclick={previewGroup}>Preview</button>
      </div>
    </div>
    <div class="row">
      <label for="group-vol">Volume</label>
      <input id="group-vol" type="range" min="0" max="1" step="0.01" bind:value={groupVolume} oninput={scheduleSave} />
    </div>
  </fieldset>
</div>

<style>
  .panel { display: grid; gap: 12px; }
  .capsule { border: 1px solid var(--border); border-radius: 12px; padding: 12px; }
  .capsule legend { color: var(--fg); padding: 0 6px; }
  .row { display: grid; gap: 6px; }
  .row-in { display: grid; grid-template-columns: 1fr auto; gap: 8px; }
  label { color: var(--fg-muted); font-size: 12px; }
  input { border: 1px solid var(--border); background: var(--bg); color: var(--fg); border-radius: 8px; padding: 8px 10px; }
  input[type="range"] { accent-color: #ffb6c1; }
  button { border: 1px solid var(--border); background: var(--bg-elev); color: var(--fg); border-radius: 8px; padding: 6px 12px; cursor: pointer; }
</style>


