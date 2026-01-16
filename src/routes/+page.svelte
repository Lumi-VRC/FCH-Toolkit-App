<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { pushDebug } from '$lib/stores/debugLog';

  let token = $state('');
  let groups = $state([] as Array<{ groupId: string; name: string; token: string }>);
  let submitting = $state(false);
  let errorMsg = $state(null as string | null);
  
  // API base URL - can be configured via environment variable
  const API_BASE = ((import.meta as any)?.env?.VITE_API_BASE || 'https://fch-toolkit.com') as string;

  async function loadGroups() {
    pushDebug('[Login] Loading stored groups...', undefined, 'info', 'frontend');
    try {
      const res = await invoke('list_group_access_tokens') as any;
      pushDebug(`[Login] Loaded groups from database: ${JSON.stringify(res)}`, undefined, 'info', 'frontend');
      groups = Array.isArray(res) ? res.map((g) => {
        const group = g as any;
        return {
          groupId: group.group_id || group.groupId,
          name: group.group_name || group.name,
          token: group.access_token || group.token
        };
      }) : [];
      pushDebug(`[Login] Parsed ${groups.length} group(s) successfully`, undefined, 'info', 'frontend');
    } catch (err) {
      const errMsg = String(err);
      pushDebug(`[Login] Failed to load groups: ${errMsg}`, undefined, 'error', 'frontend');
      console.error('Failed to load groups:', err);
      groups = [];
    }
  }

  async function submitToken() {
    errorMsg = null;
    const trimmed = token.trim();
    
    pushDebug(`[Login] Token submission started. Token length: ${trimmed.length}`, undefined, 'info', 'frontend');
    
    if (!trimmed) {
      errorMsg = 'Please enter a token';
      pushDebug('[Login] Validation failed: Empty token', undefined, 'warn', 'frontend');
      return;
    }
    
    // Validate token format (64 hex characters)
    if (!/^[0-9a-fA-F]{64}$/.test(trimmed)) {
      errorMsg = 'Invalid token format. Token must be 64 hexadecimal characters.';
      pushDebug(`[Login] Validation failed: Invalid token format. Token preview: ${trimmed.substring(0, 16)}...`, undefined, 'warn', 'frontend');
      return;
    }
    
    submitting = true;
    const tokenPreview = `${trimmed.substring(0, 8)}...${trimmed.substring(56)}`;
    pushDebug(`[Login] Sending token to backend: ${tokenPreview}`, undefined, 'info', 'frontend');
    pushDebug(`[Login] API endpoint: ${API_BASE}/api/validate-access-token`, undefined, 'info', 'frontend');
    
    try {
      const requestUrl = `${API_BASE}/api/validate-access-token?token=${encodeURIComponent(trimmed)}`;
      pushDebug(`[Login] Full request URL: ${requestUrl.replace(trimmed, tokenPreview)}`, undefined, 'debug', 'frontend');
      
      // Validate token with backend API
      const resp = await fetch(requestUrl);
      
      pushDebug(`[Login] Response received. Status: ${resp.status} ${resp.statusText}`, undefined, 'info', 'frontend');
      pushDebug(`[Login] Response headers: ${JSON.stringify(Object.fromEntries(resp.headers.entries()))}`, undefined, 'debug', 'frontend');
      
      let data;
      let responseText = '';
      try {
        responseText = await resp.text();
        pushDebug(`[Login] Raw response body: ${responseText.substring(0, 500)}${responseText.length > 500 ? '...' : ''}`, undefined, 'debug', 'frontend');
        data = JSON.parse(responseText);
        pushDebug(`[Login] Parsed response data: ${JSON.stringify(data)}`, undefined, 'info', 'frontend');
      } catch (e) {
        const parseErr = String(e);
        pushDebug(`[Login] Failed to parse server response: ${parseErr}. Response text: ${responseText.substring(0, 200)}`, undefined, 'error', 'frontend');
        errorMsg = 'Failed to parse server response';
        submitting = false;
        return;
      }
      
      if (!resp.ok || !data || !(data as any).success) {
        const errorMessages = {
          'invalid_token_format': 'Invalid token format',
          'invalid_token': 'Token not found or invalid',
          'no_permissions': 'No permissions configured for this token',
          'not_authorized': 'You are not authorized to use this token',
          'watchlist_not_enabled': 'Group watchlist is not enabled for this group',
          'validation_failed': 'Token validation failed',
          'missing_token': 'Token is missing from request'
        };
        const dataErr = data as any;
        const errorMessagesTyped = errorMessages as Record<string, string>;
        const errorCode = dataErr.error || 'unknown_error';
        errorMsg = errorMessagesTyped[errorCode] || dataErr.error || 'Invalid token';
        
        pushDebug(`[Login] Validation failed. Error code: ${errorCode}, Full response: ${JSON.stringify(dataErr)}`, undefined, 'error', 'frontend');
        submitting = false;
        return;
      }
      
      const { groupId, groupName } = data as { groupId: string; groupName: string };
      pushDebug(`[Login] Validation succeeded. Group ID: ${groupId}, Group Name: ${groupName || 'null'}`, undefined, 'info', 'frontend');
      
      // Check if group already exists
      if (groups.find(g => g.groupId === groupId)) {
        errorMsg = 'This group is already added';
        pushDebug(`[Login] Group ${groupId} already exists in local database`, undefined, 'warn', 'frontend');
        submitting = false;
        return;
      }
      
      pushDebug(`[Login] Storing token in local database for group ${groupId}`, undefined, 'info', 'frontend');
      
      // Store token in database
      try {
        await invoke('add_group_access_token', {
          groupId,
          groupName: groupName || 'Unknown Group',
          token: trimmed
        });
        pushDebug(`[Login] Token stored successfully in database`, undefined, 'info', 'frontend');
      } catch (storeErr) {
        const storeErrMsg = String(storeErr);
        pushDebug(`[Login] Failed to store token in database: ${storeErrMsg}`, undefined, 'error', 'frontend');
        throw storeErr;
      }
      
      // Reload groups list
      await loadGroups();
      
      // Clear input
      token = '';
      errorMsg = null;
      pushDebug(`[Login] Token submission completed successfully`, undefined, 'info', 'frontend');
    } catch (e) {
      const err = e as any;
      const errMsg = err?.message || String(err) || 'Network error. Please check your connection.';
      pushDebug(`[Login] Exception during token submission: ${errMsg}`, undefined, 'error', 'frontend');
      pushDebug(`[Login] Exception details: ${JSON.stringify({ name: err?.name, stack: err?.stack?.substring(0, 500) })}`, undefined, 'error', 'frontend');
      console.error('Failed to submit token:', e);
      errorMsg = errMsg;
    } finally {
      submitting = false;
    }
  }

  async function removeGroup(groupId) {
    const id = groupId as string;
    pushDebug(`[Login] Removing group: ${id}`, undefined, 'info', 'frontend');
    try {
      await invoke('remove_group_access_token', { groupId: id });
      groups = groups.filter(g => g.groupId !== id);
      pushDebug(`[Login] Group ${id} removed successfully`, undefined, 'info', 'frontend');
    } catch (err) {
      const errMsg = String(err);
      pushDebug(`[Login] Failed to remove group ${id}: ${errMsg}`, undefined, 'error', 'frontend');
      console.error('Failed to remove group:', err);
      errorMsg = 'Failed to remove group';
    }
  }

  // Handle Enter key press in token input
  function handleKeyPress(event) {
    const e = event as KeyboardEvent;
    if (e.key === 'Enter' && !submitting) {
      submitToken();
    }
  }

  onMount(() => {
    loadGroups();
  });
</script>

<div class="login-card">
  <h2>Group Authentication</h2>
  <p class="muted">Enter your group access token to enable watchlist features.</p>
  
  <div class="token-section">
    <div class="row">
      <input 
        type="text" 
        placeholder="Paste 64-character token here" 
        bind:value={token}
        onkeypress={handleKeyPress}
        disabled={submitting}
        maxlength="64"
      />
      <button 
        disabled={submitting || !token.trim()} 
        onclick={submitToken}
        class="submit-btn"
      >
        {submitting ? 'Validating...' : 'Add Group'}
      </button>
    </div>
    
    {#if errorMsg}
      <div class="error">{errorMsg}</div>
    {/if}
  </div>

  <div class="groups-section">
    <h3>Added Groups</h3>
    <ul class="groups">
      {#each groups as g}
        <li>
          <div class="left">
            <div class="name">{g.name}</div>
            <div class="id">ID: {g.groupId}</div>
          </div>
          <button class="del" onclick={() => removeGroup(g.groupId)}>Remove</button>
        </li>
      {/each}
      {#if groups.length === 0}
        <li class="empty">No groups added yet. Add a token above to get started.</li>
      {/if}
    </ul>
  </div>
</div>

<style>
  .login-card {
    max-width: 640px;
    margin: 0 auto;
    padding: 24px;
    background: var(--bg-elev);
    border: 1px solid var(--border);
    border-radius: 12px;
  }

  h2 {
    margin: 0 0 8px 0;
    font-size: 24px;
    font-weight: 600;
  }

  .muted {
    color: var(--fg-muted);
    margin: 0 0 24px 0;
    font-size: 14px;
  }

  .token-section {
    margin-bottom: 32px;
  }

  .row {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 8px;
    margin-bottom: 12px;
  }

  input {
    height: 40px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--fg);
    padding: 0 12px;
    font-family: 'Courier New', monospace;
    font-size: 13px;
    transition: border-color 0.2s, box-shadow 0.2s;
  }

  input:focus {
    outline: none;
    border-color: var(--accent);
    box-shadow: 0 0 0 2px rgba(255, 20, 147, 0.1);
  }

  input:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  button {
    height: 40px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--fg);
    cursor: pointer;
    padding: 0 16px;
    font-weight: 500;
    transition: background 0.2s, border-color 0.2s, box-shadow 0.2s;
  }

  button:hover:not(:disabled) {
    background: var(--bg-hover);
    border-color: var(--accent);
    box-shadow: 0 0 0 2px rgba(255, 20, 147, 0.1);
  }

  button:active:not(:disabled) {
    box-shadow: 0 0 0 6px rgba(255, 20, 147, 0.2);
  }

  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .submit-btn {
    min-width: 120px;
  }

  .error {
    margin-top: 8px;
    padding: 10px 12px;
    background: rgba(255, 131, 131, 0.1);
    border: 1px solid rgba(255, 131, 131, 0.3);
    border-radius: 6px;
    color: #ff8383;
    font-size: 13px;
  }

  .groups-section {
    margin-top: 32px;
    padding-top: 24px;
    border-top: 1px solid var(--border);
  }

  h3 {
    margin: 0 0 16px 0;
    font-size: 18px;
    font-weight: 600;
  }

  .groups {
    list-style: none;
    padding: 0;
    margin: 0;
    display: grid;
    gap: 8px;
  }

  .groups li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg);
    transition: border-color 0.2s;
  }

  .groups li:hover {
    border-color: var(--accent);
  }

  .left {
    flex: 1;
    min-width: 0;
  }

  .name {
    font-weight: 600;
    margin-bottom: 4px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .id {
    font-size: 12px;
    color: var(--fg-muted);
    font-family: 'Courier New', monospace;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .del {
    background: rgba(255, 131, 131, 0.1);
    border-color: rgba(255, 131, 131, 0.3);
    color: #ff8383;
    padding: 6px 12px;
    height: auto;
    font-size: 13px;
  }

  .del:hover:not(:disabled) {
    background: rgba(255, 131, 131, 0.2);
    border-color: #ff8383;
  }

  .empty {
    justify-content: center;
    color: var(--fg-muted);
    font-style: italic;
    padding: 24px;
  }
</style>
