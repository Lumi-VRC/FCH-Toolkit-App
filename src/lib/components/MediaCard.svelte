<script lang="ts">
  import { createEventDispatcher, onDestroy } from 'svelte';
  import { getMediaSource } from '$lib/stores/mediaCache';

export type MediaCardItem = {
  id: string;
  label?: string;
  image: string | null;
  type?: string | null;
};

  export let item: MediaCardItem;

  const dispatch = createEventDispatcher<{ select: void }>();

  let src: string = item.image ?? '';
  let displayLabel = item.label ?? item.id;
  let loading = true;
  let error: string | null = null;
  let active = true;

  $: (async () => {
    if (!item || !item.image) {
      src = '';
      loading = false;
      error = null;
      return;
    }
    loading = true;
    error = null;
    try {
      const resolved = await getMediaSource(item.image);
      if (!active) return;
      src = resolved;
    } catch (err: any) {
      if (!active) return;
      error = err?.message ?? 'Failed to load image';
      src = item.image;
    } finally {
      if (active) {
        loading = false;
      }
    }
  })();

  onDestroy(() => {
    active = false;
  });
</script>

<button class="media-card" data-item-type={item.type ?? ''} role="listitem" on:click={() => dispatch('select')}>
  <div class="media-thumb" aria-hidden="true">
    {#if loading}
      <div class="placeholder">Loading…</div>
    {:else if error}
      <div class="placeholder error">{error}</div>
    {:else if src}
      <img src={src} alt={item.label} loading="lazy" />
    {:else}
      <div class="placeholder">No image</div>
    {/if}
  </div>
  <div class="media-title">{displayLabel}</div>
</button>

<style>
  .media-card {
    display: flex;
    flex-direction: column;
    gap: 8px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--bg);
    color: var(--fg);
    padding: 8px;
    cursor: pointer;
    text-align: left;
    transition: border-color 0.2s ease, transform 0.2s ease;
  }

  .media-card:hover {
    border-color: var(--accent, #ffb6c1);
    transform: translateY(-1px);
  }

  .media-card:focus-visible {
    outline: 2px solid var(--accent, #ffb6c1);
    outline-offset: 2px;
  }

  .media-thumb {
    position: relative;
    width: 100%;
    aspect-ratio: 1 / 1;
    flex: 1 0 auto;
    overflow: hidden;
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.04);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .media-thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .media-title {
    font-weight: 600;
    font-size: 14px;
  }

  .placeholder {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--fg-muted);
    font-size: 13px;
    padding: 8px;
    text-align: center;
  }

  .placeholder.error {
    color: var(--fg-err);
  }

  .media-card[data-item-type="emoji"],
  .media-card[data-item-type="sticker"] {
    justify-content: space-between;
  }
</style>

