<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';

  let tab: 'media' | 'avatar' = 'media';

  const tabs = [
    { id: 'media', label: 'Media' },
    { id: 'avatar', label: 'Avatar Logs' }
  ];

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
</script>

<div class="page">
  <div class="header" role="tablist">
    <div class="tabs">
      {#each tabs as t}
        <button
          role="tab"
          aria-selected={tab === t.id}
          class:active={tab === t.id}
          onclick={() => tab = t.id as any}
        >
          {t.label}
        </button>
      {/each}
    </div>
  </div>

  <section class="content">
    {#if tab === 'media'}
      <div class="panel" role="tabpanel">
        <h2>Media Moderation</h2>
        <p>Coming soon. Use this area to review and manage world media assets.</p>
      </div>
    {:else}
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
    {/if}
  </section>
</div>

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .header {
    display: flex;
    justify-content: flex-start;
  }

  .tabs {
    display: inline-flex;
    gap: 8px;
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
    gap: 8px;
  }

  h2 {
    margin: 0;
  }

  p {
    margin: 0;
    color: var(--fg-muted);
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
</style>

