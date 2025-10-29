<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';

type MediaTab = 'media' | 'emojis' | 'stickers' | 'prints' | 'avatar';
let tab: MediaTab = 'media';

  const tabs: Array<{ id: MediaTab; label: string }> = [
    { id: 'media', label: 'Media' },
    { id: 'emojis', label: 'Emojis' },
    { id: 'stickers', label: 'Stickers' },
    { id: 'prints', label: 'Prints' },
    { id: 'avatar', label: 'Avatar Logs' }
  ];

  type MediaItem = {
    id: string;
    itemType: string;
    imageUrl: string | null;
    ownerId: string | null;
    ownerName?: string | null;
    fetchedAt: string;
  };

import MediaCard from '$lib/components/MediaCard.svelte';
import { clearMediaCache, prefetchMedia } from '$lib/stores/mediaCache';
import type { MediaCardItem as CardItem } from '$lib/components/MediaCard.svelte';

export let fallbackMediaItems: MediaItem[] = [];
let mediaItems: MediaItem[] = fallbackMediaItems;
let selectedItem: MediaItem | null = null;
let activeMediaTabItems: MediaItem[] = [];
let clearing = false;
let reporting = false;

  const MEDIA_FETCH_LIMIT = 150;
  const MEDIA_TAB_LIMIT = 48;
const allowedMediaTypes = new Set(['print', 'sticker', 'emoji']);
const liveLookup = new Map<string, string>();
const historyLookup = new Map<string, string>();
type MediaTypeKey = 'print' | 'sticker' | 'emoji';
const typeDisplayMap: Record<MediaTypeKey, string> = {
  print: 'Print',
  sticker: 'Sticker',
  emoji: 'Emoji'
};
const tabDisplayMap: Record<Exclude<MediaTab, 'avatar'>, string> = {
  media: 'Media',
  emojis: 'Emoji',
  stickers: 'Sticker',
  prints: 'Print'
};

  async function loadMediaItems() {
    try {
      const raw: any[] = await invoke('get_media_items', { limit: MEDIA_FETCH_LIMIT });
      mediaItems = raw
        .map((entry) => {
          const rawType = String(entry.itemType ?? entry.item_type ?? entry.type ?? '').toLowerCase();
          const image =
            entry.imageUrl ??
            entry.image_url ??
            entry.metadata?.imageUrl ??
            entry.metadata?.image_url ??
            null;
          const owner =
            entry.ownerId ??
            entry.owner_id ??
            entry.holderId ??
            entry.holder_id ??
            null;

          return {
            id: entry.id,
            itemType: rawType,
            imageUrl: image,
            ownerId: owner,
            ownerName: entry.ownerName ?? entry.owner_name ?? (owner ? lookupOwnerName(owner) : null),
            fetchedAt: entry.fetchedAt ?? entry.fetched_at ?? ''
          } as MediaItem;
        })
        .filter((item) => allowedMediaTypes.has(item.itemType));
      console.log(`[media] gallery refreshed -> items loaded: ${mediaItems.length}`);
      if (mediaItems.length === 0) {
        console.warn('[media] no media items loaded (empty response)');
      }
      updateActiveTabItems();
    } catch (err) {
      console.error('Failed to load media items', err);
      mediaItems = fallbackMediaItems.slice();
      updateActiveTabItems();
    }
  }

  async function clearMediaItems() {
    if (clearing) return;
    clearing = true;
    try {
      await invoke('clear_media_items');
      await loadMediaItems();
      console.log('[media] media cache cleared');
    } catch (err) {
      console.error('Failed to clear media items', err);
    } finally {
      clearing = false;
    }
  }

  function getDisplayType(item: MediaItem, contextTab: MediaTab = tab): string {
    const direct = typeDisplayMap[item.itemType as MediaTypeKey];
    if (direct) return direct;
    if (contextTab !== 'avatar') {
      const fallback = tabDisplayMap[contextTab as Exclude<MediaTab, 'avatar'>];
      if (fallback) return fallback;
    }
    const raw = item.itemType.trim();
    return raw.length > 0 ? raw : 'Media';
  }

  async function populateHistoryLookup() {
    try {
      const history: any[] = await invoke('get_active_join_logs');
      history.forEach((entry) => {
        const userId = entry.userId || entry.user_id;
        const username = entry.username || entry.ownerId || entry.owner_id;
        if (userId && username) {
          historyLookup.set(String(userId), String(username));
        }
      });
    } catch (err) {
      console.warn('Failed to populate history lookup', err);
    }
  }

  let initialized = false;

  onMount(() => {
    if (!initialized) {
      initialized = true;
      void populateHistoryLookup();
      void loadMediaItems();
      void loadEntries(0);
    }
    let unlistenMedia: undefined | (() => void);
    (async () => {
      try {
        const { listen } = await import('@tauri-apps/api/event');
        unlistenMedia = await listen('media_item_updated', async () => {
          console.log('[media] event received -> refreshing gallery');
          await loadMediaItems();
          console.log('[media] gallery count after refresh:', mediaItems.length);
          updateActiveTabItems();
        });
      } catch (err) {
        console.error('Failed to bind media_item_updated listener', err);
      }
    })();
    onDestroy(() => {
      unlistenMedia?.();
    });
  });

  function openItem(item: MediaItem) {
    selectedItem = item;
  }

function selectTab(newTab: MediaTab) {
  tab = newTab;
  updateActiveTabItems(newTab);
}

  function getActiveItems(tabId: MediaTab) {
    if (tabId === 'avatar') return [];
    const filtered = mediaItems.filter((item) => {
      if (tabId === 'media') return true;
      if (tabId === 'prints') return item.itemType.toLowerCase() === 'print';
      if (tabId === 'emojis') return item.itemType.toLowerCase() === 'emoji';
      if (tabId === 'stickers') return item.itemType.toLowerCase() === 'sticker';
      return false;
    });
    const deduped = Array.from(new Map(filtered.map((item) => [item.id, item])).values());
    const sorted = deduped.sort((a, b) => (b.fetchedAt || '').localeCompare(a.fetchedAt || ''));
    return sorted
      .slice(0, MEDIA_TAB_LIMIT)
      .map((item) => ({
        ...item,
        ownerName: lookupOwnerName(item.ownerId) ?? item.ownerId ?? 'Unknown'
      }));
  }

function updateActiveTabItems(tabId: MediaTab = tab) {
  if (tabId === 'avatar') {
    activeMediaTabItems = [];
    return;
  }
  const itemsForTab = getActiveItems(tabId);
  activeMediaTabItems = itemsForTab;
  const uniqueUrls = Array.from(
    new Set(
      itemsForTab
        .map((item) => item.imageUrl)
        .filter((url): url is string => typeof url === 'string' && url.length > 0)
    )
  ).slice(0, 24);
  console.log(`[media] prefetch unique urls: ${uniqueUrls.length}`);
  prefetchMedia(uniqueUrls);
}

  function lookupOwnerName(ownerId: string | null): string | null {
    if (!ownerId) return null;
    return liveLookup.get(ownerId) || historyLookup.get(ownerId) || null;
  }

  function closeItem() {
    selectedItem = null;
    reporting = false;
  }

  async function reportSelectedItem() {
    if (!selectedItem) return;
    const payload = {
      id: selectedItem.id,
      imageUrl: selectedItem.imageUrl ?? null,
      type: selectedItem.itemType ?? 'unknown'
    };
    if (!payload.imageUrl) {
      console.warn('[media] Cannot report item without image URL');
      return;
    }
    reporting = true;
    try {
      const response = await fetch('https://fch-toolkit.com/img-report', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload)
      });
      if (!response.ok) {
        const text = await response.text();
        throw new Error(text || `HTTP ${response.status}`);
      }
      console.log('[media] instant report submitted', payload);
    } catch (err) {
      console.error('[media] instant report failed', err);
    } finally {
      reporting = false;
    }
  }

onDestroy(() => {
  clearMediaCache();
});

  type AvatarEntry = {
    avatarName: string;
    ownerId: string;
    fileId: string;
    version: number;
    updatedAt: string;
    performanceRating?: string | null;
  };

  let search = '';
  let offset = 0;
  const PAGE_SIZE = 100;
  let total = 0;
  let items: AvatarEntry[] = [];
  let loading = false;
  let errorMsg: string | null = null;

  async function loadEntries(newOffset = 0) {
    loading = true;
    errorMsg = null;
    try {
      const res: any = await invoke('list_distinct_avatar_details', {
        offset: newOffset,
        limit: PAGE_SIZE,
        search
      });
      offset = newOffset;
      total = Number(res?.total || 0);
      items = Array.isArray(res?.items) ? res.items.map((entry: any) => ({
        avatarName: entry.avatarName || '',
        ownerId: entry.ownerId || 'unknown_owner',
        fileId: entry.fileId || '',
        version: entry.version || 0,
        updatedAt: entry.updatedAt || '',
        performanceRating: entry.performanceRating ?? null,
      })) : [];
    } catch (err) {
      errorMsg = err instanceof Error ? err.message : String(err ?? 'Unknown error');
      items = [];
    } finally {
      loading = false;
    }
  }

  function nextPage() {
    if (offset + PAGE_SIZE >= total) return;
    void loadEntries(offset + PAGE_SIZE);
  }

  function prevPage() {
    if (offset === 0) return;
    const newOffset = Math.max(0, offset - PAGE_SIZE);
    void loadEntries(newOffset);
  }

  function applySearch() {
    offset = 0;
    void loadEntries(0);
  }

  onMount(() => {
    void loadEntries(0);
  });

function formatTitle(item: MediaItem): string {
  const owner = item.ownerName || item.ownerId || 'Unknown';
  return `[ ${getDisplayType(item)} | ${owner} ]`;
}
</script>

<div class="page">
  <div class="header" role="tablist">
    <div class="tabs">
      {#each tabs as t}
        <button
          role="tab"
          aria-selected={tab === t.id}
          class:active={tab === t.id}
          onclick={() => selectTab(t.id)}
        >
          {t.label}
        </button>
      {/each}
    </div>
    <button class="clear-button" onclick={clearMediaItems} disabled={clearing}>
      {clearing ? 'Clearing…' : 'Clear Media'}
    </button>
  </div>

  <section class="content">
    {#if tab === 'avatar'}
      <div class="panel" role="tabpanel">
        <h2>Avatar Logs</h2>
        <div class="controls">
          <input
            placeholder="Search avatar name..."
            bind:value={search}
            onkeydown={(e) => {
              if (e.key === 'Enter') applySearch();
            }}
          />
          <button class="ghost" onclick={applySearch} disabled={loading}>Search</button>
        </div>
        {#if errorMsg}
          <div class="error">{errorMsg}</div>
        {:else if loading}
          <div class="loading">Loading…</div>
        {:else if items.length === 0}
          <div class="empty">No avatar records found.</div>
        {:else}
          <div class="table-wrapper">
            <table>
              <thead>
                <tr>
                  <th>Avatar</th>
                  <th>Owner</th>
                  <th>Version</th>
                  <th>Performance</th>
                  <th>File ID</th>
                  <th>Updated</th>
                </tr>
              </thead>
              <tbody>
                {#each items as entry}
                  <tr>
                    <td>{entry.avatarName}</td>
                    <td>{entry.ownerId}</td>
                    <td>{entry.version}</td>
                    <td>{entry.performanceRating ?? '—'}</td>
                    <td title={entry.fileId}>{entry.fileId || '—'}</td>
                    <td>{entry.updatedAt}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
          <div class="pager">
            <button onclick={prevPage} disabled={offset === 0 || loading}>Prev</button>
            <span>{Math.floor(offset / PAGE_SIZE) + 1} / {Math.max(1, Math.ceil(total / PAGE_SIZE))}</span>
            <button onclick={nextPage} disabled={offset + PAGE_SIZE >= total || loading}>Next</button>
          </div>
        {/if}
      </div>
    {:else}
      {@const activeItems = activeMediaTabItems}
      {@const currentLabel = tabs.find((t) => t.id === tab)?.label ?? ''}
      <div class="panel" role="tabpanel">
        <div class="panel-header">
          <h2>{currentLabel} Gallery</h2>
          <p class="panel-subtitle">Browse captured {currentLabel.toLowerCase()} assets. Select an item to view details.</p>
        </div>
        {#if activeItems.length === 0}
          <div class="empty">No items available yet.</div>
        {:else}
          <div class="media-grid" role="list">
            {#each activeItems as item}
              <MediaCard
                item={{
                  id: item.id,
                  label: `${getDisplayType(item, tab)} :: ${item.ownerName || item.ownerId || 'Unknown'}`,
                  image: item.imageUrl,
                  type: item.itemType
                }}
                on:select={() => openItem(item)}
              />
            {/each}
          </div>
        {/if}
      </div>
    {/if}
  </section>
</div>

{#if selectedItem}
  <div class="modal-backdrop" role="button" aria-label="Close preview" tabindex="0" onclick={closeItem} onkeydown={(e) => { if (e.key === 'Escape' || e.key === 'Enter' || e.key === ' ') { e.preventDefault(); closeItem(); }}}>
    <div class="modal" role="dialog" aria-modal="true" onclick={(e) => e.stopPropagation()}>
      <header>
        <h3>{selectedItem ? formatTitle(selectedItem) : ''}</h3>
        <button class="close" aria-label="Close" onclick={closeItem}>×</button>
      </header>
      <div class="modal-body">
        <div class="modal-thumb" data-item-type={selectedItem?.itemType}>
          {#if selectedItem?.imageUrl}
            <img src={selectedItem.imageUrl} alt={selectedItem.id} />
          {:else}
            <div class="placeholder">No image available</div>
          {/if}
        </div>
        <div class="meta">
          <div class="row"><strong>Image:</strong> {selectedItem?.imageUrl || '—'}</div>
          <div class="row"><strong>Owner:</strong> {selectedItem?.ownerName || selectedItem?.ownerId || 'Unknown'}</div>
          <div class="row"><strong>Fetched:</strong> {selectedItem?.fetchedAt}</div>
        </div>
        <button
          class="report-button"
          title="Export emoji to a private VRC dev channel to help VRChat improve their systems!"
          onclick={reportSelectedItem}
          disabled={reporting || !selectedItem?.imageUrl}
        >
          {reporting ? 'Reporting…' : 'Instant Report'}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .header {
    display: flex;
    justify-content: flex-start;
    gap: 12px;
    align-items: center;
  }

  .tabs {
    display: inline-flex;
    gap: 8px;
  }

  .header .clear-button {
    margin-left: auto;
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--fg);
    border-radius: 8px;
    padding: 6px 12px;
    cursor: pointer;
    font-weight: 600;
  }

  .header .clear-button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .tabs button {
    height: 36px;
    padding: 0 14px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--fg);
    cursor: pointer;
    font-weight: 600;
  }

  .tabs button.active {
    background: var(--bg-elev);
    border-color: var(--fg-muted);
  }

  .content {
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--bg-elev);
    padding: 16px;
  }

  .panel {
    display: grid;
    gap: 12px;
  }

  h2 {
    margin: 0;
  }

  p {
    margin: 0;
    color: var(--fg-muted);
  }

  .panel-subtitle {
    font-size: 13px;
  }

  .panel-header {
    display: grid;
    gap: 6px;
  }

  .controls {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    align-items: center;
  }

  .controls input {
    flex: 1;
    min-width: 240px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--fg);
    padding: 6px 10px;
  }

  .controls button {
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--fg);
    border-radius: 8px;
    padding: 6px 12px;
    cursor: pointer;
  }

  .controls button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .table-wrapper {
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: 8px;
  }

  table {
    width: 100%;
    border-collapse: collapse;
  }

  th, td {
    padding: 8px 10px;
    border-bottom: 1px solid var(--border);
    text-align: left;
  }

  th {
    background: rgba(255,255,255,0.05);
    font-weight: 600;
  }

  tr:last-child td {
    border-bottom: none;
  }

  .pager {
    display: flex;
    align-items: center;
    gap: 12px;
    justify-content: flex-end;
  }

  .pager button {
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--fg);
    border-radius: 8px;
    padding: 4px 10px;
    cursor: pointer;
  }

  .pager button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .empty, .loading, .error {
    color: var(--fg-muted);
    padding: 12px 0;
  }

  .error {
    color: var(--fg-err);
  }

  .media-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 12px;
    max-height: calc(100vh - 340px);
    overflow-y: auto;
    padding-right: 4px;
  }

  .media-card {
    display: grid;
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
    aspect-ratio: 16 / 9;
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

  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.65);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }

  .modal {
    width: min(720px, 92vw);
    background: var(--bg-elev);
    color: var(--fg);
    border: 1px solid var(--border);
    border-radius: 14px;
    box-shadow: 0 24px 70px rgba(0, 0, 0, 0.45);
    display: grid;
    grid-template-rows: auto 1fr;
    max-height: 90vh;
  }

  .modal header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 18px;
    border-bottom: 1px solid var(--border);
  }

  .modal header h3 {
    margin: 0;
    font-size: 18px;
  }

  .modal .close {
    border: none;
    background: transparent;
    color: var(--fg);
    font-size: 22px;
    cursor: pointer;
    line-height: 1;
  }

  .modal-body {
    padding: 18px;
    display: flex;
    flex-direction: column;
    gap: 16px;
    align-items: stretch;
    overflow-y: auto;
  }

  .modal-thumb {
    width: 100%;
    aspect-ratio: 16 / 9;
    flex: 0 0 auto;
    border-radius: 10px;
    overflow: hidden;
    background: rgba(255, 255, 255, 0.06);
  }

  .modal-thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .modal-thumb[data-item-type="emoji"],
  .modal-thumb[data-item-type="sticker"] {
    aspect-ratio: 1 / 1;
  }

  .meta {
    display: grid;
    gap: 10px;
    font-size: 14px;
  }

  .report-button {
    align-self: flex-start;
    border: none;
    border-radius: 8px;
    padding: 8px 14px;
    background: #b3261e;
    color: #fff;
    font-weight: 600;
    cursor: pointer;
    transition: filter 0.2s ease;
  }

  .report-button:hover:enabled {
    filter: brightness(1.1);
  }

  .report-button:disabled {
    opacity: 0.7;
    cursor: not-allowed;
  }

  .meta .row {
    display: grid;
    grid-template-columns: 130px 1fr;
    gap: 8px;
  }

  .meta a {
    color: var(--accent, #ffb6c1);
    word-break: break-all;
  }

  .modal {
    display: flex;
    flex-direction: column;
  }

  .modal .body,
  .modal-body {
    overflow-y: auto;
  }
</style>

