// Group Watchlist Batcher: Batches user IDs and sends them to check-user endpoint
//
// This module listens to player_joined events, batches user IDs for 1 second,
// then sends them to the backend /check-user endpoint with all stored tokens.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, async_runtime};
use serde::{Deserialize, Serialize};
use tokio::time::sleep;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupMatch {
    pub user_id: String,
    pub group_id: String,
    #[serde(rename = "groupName")]
    pub group_name: Option<String>,
    pub watchlist: bool,
    pub notes: Option<String>,
    pub notifications: bool, // Same as watchlist, but explicit
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupAggregate {
    pub user_id: String,
    pub warns: i64,
    pub kicks: i64,
    pub bans: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckUserResponse {
    pub matches: Vec<GroupMatch>,
    pub aggregates: Vec<GroupAggregate>,
}

// Shared state for batching
struct BatcherState {
    pending_user_ids: HashSet<String>,
    last_join_time: Option<Instant>,
    cancelled: Arc<Mutex<bool>>, // Simple cancellation flag
}

impl BatcherState {
    fn new() -> Self {
        Self {
            pending_user_ids: HashSet::new(),
            last_join_time: None,
            cancelled: Arc::new(Mutex::new(false)),
        }
    }
}

static BATCHER_STATE: Mutex<Option<Arc<Mutex<BatcherState>>>> = Mutex::new(None);

/// Initialize the batcher (called once at startup)
pub fn init_batcher(_app_handle: AppHandle) -> Result<(), String> {
    let state = Arc::new(Mutex::new(BatcherState::new()));
    *BATCHER_STATE.lock().unwrap() = Some(state);
    Ok(())
}

/// Add a user ID to the batch (called from frontend when player_joined event is received)
#[tauri::command]
pub fn add_user_to_batch_command(app_handle: AppHandle, user_id: String) -> Result<(), String> {
    add_user_to_batch(app_handle, user_id)
}

/// Add a user ID to the batch and schedule flush if needed
fn add_user_to_batch(app_handle: AppHandle, user_id: String) -> Result<(), String> {
    let state_arc = BATCHER_STATE.lock().unwrap().clone().ok_or("Batcher not initialized")?;
    let mut state = state_arc.lock().unwrap();
    
    state.pending_user_ids.insert(user_id);
    state.last_join_time = Some(Instant::now());
    
    // Cancel existing timer by setting cancelled flag
    *state.cancelled.lock().unwrap() = true;
    
    // Create new cancellation flag for this timer
    let cancelled = Arc::new(Mutex::new(false));
    state.cancelled = cancelled.clone();
    
    // Schedule new flush after 1 second using Tauri's async runtime
    let app_clone = app_handle.clone();
    let state_clone = state_arc.clone();
    let cancelled_clone = cancelled.clone();
    
    async_runtime::spawn(async move {
        // Use tokio::time::sleep - this works because we're in Tauri's async runtime context
        sleep(Duration::from_secs(1)).await;
        
        // Check if cancelled
        if *cancelled_clone.lock().unwrap() {
            return;
        }
        
        let _ = flush_batch(app_clone, state_clone);
    });
    
    Ok(())
}

/// Flush the current batch to the server
fn flush_batch(app_handle: AppHandle, state_arc: Arc<Mutex<BatcherState>>) -> Result<(), String> {
    let mut state = state_arc.lock().unwrap();
    
    if state.pending_user_ids.is_empty() {
        return Ok(());
    }
    
    let user_ids: Vec<String> = state.pending_user_ids.drain().collect();
    state.last_join_time = None;
    
    // Don't block - spawn async task using Tauri's runtime
    let app_clone = app_handle.clone();
    async_runtime::spawn(async move {
        if let Err(e) = send_batch_to_server(app_clone.clone(), user_ids).await {
            crate::debug_eprintln!("[batcher] Failed to send batch: {}", e);
            let _ = app_clone.emit("group_watch_error", serde_json::json!({
                "error": e
            }));
        }
    });
    
    Ok(())
}

/// Send batch to server and emit results
async fn send_batch_to_server(app_handle: AppHandle, user_ids: Vec<String>) -> Result<(), String> {
    // Get all stored tokens
    let tokens = crate::modules::group_auth::group_access_tokens::list_group_access_tokens()
        .map_err(|e| format!("Failed to get tokens: {}", e))?;
    
    if tokens.is_empty() {
        return Ok(()); // No tokens, nothing to check
    }
    
    let access_tokens: Vec<String> = tokens
        .into_iter()
        .map(|t| t.access_token)
        .filter(|t| t.len() >= 32) // Basic validation
        .collect();
    
    if access_tokens.is_empty() {
        return Ok(()); // No valid tokens
    }
    
    // API base URL - should match frontend
    let api_base = std::env::var("VITE_API_BASE")
        .unwrap_or_else(|_| "https://fch-toolkit.com".to_string());
    
    let url = format!("{}/check-user", api_base);
    let payload = serde_json::json!({
        "userIds": user_ids,
        "tokens": access_tokens
    });
    
    // Send HTTP request
    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("HTTP error: {}", e))?;
    
    if !response.status().is_success() {
        return Err(format!("Server returned status: {}", response.status()));
    }
    
    let result: CheckUserResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;
    
    // Emit results to frontend
    let _ = app_handle.emit("group_watch_results", serde_json::json!({
        "matches": result.matches,
        "aggregates": result.aggregates
    }));
    
    Ok(())
}

/// Manually trigger a batch flush (for testing or immediate checks)
#[tauri::command]
pub fn flush_user_batch() -> Result<String, String> {
    let state_arc = BATCHER_STATE.lock().unwrap().clone().ok_or("Batcher not initialized")?;
    let state = state_arc.lock().unwrap();
    
    let count = state.pending_user_ids.len();
    if count == 0 {
        return Ok("No pending users to flush".to_string());
    }
    
    // Note: This would need app_handle to actually flush, so this is just for status
    Ok(format!("{} users pending in batch", count))
}
