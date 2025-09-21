<script lang="ts">
  import { onMount } from 'svelte';
  // We will store groups locally using Tauri SQL (sqlite) via backend commands; for now, simple fetch to backend endpoints.
  let token = '';
  type AddedGroup = { groupId: string; name: string; token: string };
  let groups: AddedGroup[] = [];
  let submitting = false;
  let errorMsg: string | null = null;
  const API_BASE: string = (import.meta as any)?.env?.VITE_API_BASE || 'https://fch-toolkit.com';

  async function loadGroups() {
    try {
      // @ts-ignore
      const { invoke } = await import('@tauri-apps/api/core');
      const res = await invoke('list_group_access_tokens');
      groups = Array.isArray(res) ? res as any : [];
    } catch {}
  }

  async function submitToken() {
    errorMsg = null;
    const trimmed = token.trim();
    if (!trimmed) { errorMsg = 'Please enter a token'; return; }
    submitting = true;
    try {
      // Prefer GET to avoid CORS preflight in dev
      const resp = await fetch(`${API_BASE}/api/validate-access-token?token=${encodeURIComponent(trimmed)}`);
      let data: any = null;
      try { data = await resp.json(); } catch {}
      if (!resp.ok || !data || !data.success) { errorMsg = (data && data.error) || 'Invalid Token'; return; }
      const { groupId, groupName } = data;
      // After successful validation, gather tool auth lines and send to backend
      let lines: string[] = [];
      try {
        // @ts-ignore
        const { invoke } = await import('@tauri-apps/api/core');
        const got = await invoke('get_tool_authentication_lines');
        if (Array.isArray(got)) lines = got as any;
      } catch {}
      try {
        await fetch(`${API_BASE}/toolAuthentication`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ token: trimmed, lines })
        });
      } catch {}
      if (!groups.find(g => g.groupId === groupId)) {
        // @ts-ignore
        const { invoke } = await import('@tauri-apps/api/core');
        await invoke('add_group_access_token', { groupId, groupName, token: trimmed });
        await loadGroups();
      }
      token = '';
    } catch (e) {
      errorMsg = 'Network error';
    } finally { submitting = false; }
  }

  async function removeGroup(id: string) {
    try {
      // @ts-ignore
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('remove_group_access_token', { groupId: id });
      groups = groups.filter(g => g.groupId !== id);
    } catch {}
  }

  onMount(loadGroups);
</script>

<div class="login-card">
  <h2>Enter Access Token</h2>
  <div class="row">
    <input type="text" placeholder="Paste token here" bind:value={token} />
    <button disabled={submitting} onclick={submitToken}>{submitting ? 'Submitting...' : 'Submit'}</button>
  </div>
  {#if errorMsg}<div class="error">{errorMsg}</div>{/if}

  <h3>Added Groups</h3>
  <ul class="groups">
    {#each groups as g}
      <li>
        <div class="left">
          <div class="name">{g.name}</div>
          <div class="id">{g.groupId}</div>
        </div>
        <button class="del" onclick={() => removeGroup(g.groupId)}>Remove</button>
      </li>
    {/each}
    {#if groups.length === 0}
      <li class="empty">No groups added yet.</li>
    {/if}
  </ul>
</div>

<style>
  .login-card { max-width: 640px; margin: 0 auto; padding: 16px; background: var(--bg-elev); border: 1px solid var(--border); border-radius: 12px; }
  h2 { margin: 0 0 12px 0; }
  .row { display: grid; grid-template-columns: 1fr auto; gap: 8px; }
  input { height: 40px; border-radius: 8px; border: 1px solid var(--border); background: var(--bg); color: var(--fg); padding: 0 10px; }
  button { height: 40px; border-radius: 8px; border: 1px solid var(--border); background: var(--bg); color: var(--fg); cursor: pointer; padding: 0 14px; }
  button:hover { background: var(--bg-hover); border-color: var(--accent); }
  .error { margin-top: 8px; color: #ff8383; }
  h3 { margin: 16px 0 8px; }
  .groups { list-style: none; padding: 0; margin: 0; display: grid; gap: 8px; }
  .groups li { display: flex; align-items: center; justify-content: space-between; padding: 10px; border: 1px solid var(--border); border-radius: 8px; background: var(--bg); }
  .groups .name { font-weight: 600; }
  .groups .id { font-size: 12px; color: var(--fg-muted); }
  .groups .del { background: #2a1f24; border-color: #3a2f34; }
  .groups .empty { justify-content: center; color: var(--fg-muted); }
</style>
