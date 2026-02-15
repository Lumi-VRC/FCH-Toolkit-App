// World Moderation Logs: SQLite database for ban events
//
// This module stores ban events extracted from VRChat logs.
// Each entry contains: Admin, Target, Reason, and Timestamp.
// New entries are automatically exported to the /api/worldlogs endpoint.

use rusqlite::{Connection, Result as SqlResult};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::async_runtime;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BanLogEntry {
    pub id: i64,
    pub admin: String,
    pub target: String,
    pub reason: String,
    pub timestamp: String,
    pub action_type: String, // "ban" or "warn"
    pub location: String,    // "world_id:instance_id" or "N/A"
}

/// Get the directory where the database is stored
/// Uses the same pathing as other modules (LocalAppData\FCHClient on Windows)
fn db_dir() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:/Users/Public"));
    base.join("FCHClient")
}

/// Get the path to the SQLite database file
fn db_path() -> PathBuf {
    db_dir().join("world_mod_logs.db")
}

/// Get or create database connection
fn get_connection() -> SqlResult<Connection> {
    let db_path = db_path();
    
    // Ensure directory exists (same pattern as local_db module)
    if let Some(parent) = db_path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return Err(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CANTOPEN),
                Some(format!("Failed to create directory: {}", e))
            ));
        }
    }
    
    let conn = Connection::open(&db_path)?;
    
    // Create table if it doesn't exist
    conn.execute(
        "CREATE TABLE IF NOT EXISTS ban_logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            admin TEXT NOT NULL,
            target TEXT NOT NULL,
            reason TEXT NOT NULL,
            timestamp TEXT NOT NULL,
            action_type TEXT NOT NULL DEFAULT 'ban'
        )",
        [],
    )?;
    
    // Add action_type column if it doesn't exist (for existing databases)
    conn.execute(
        "ALTER TABLE ban_logs ADD COLUMN action_type TEXT DEFAULT 'ban'",
        [],
    ).ok(); // Ignore error if column already exists

    // Add location column (world_id:instance_id) if it doesn't exist
    conn.execute(
        "ALTER TABLE ban_logs ADD COLUMN location TEXT DEFAULT 'N/A'",
        [],
    ).ok(); // Ignore error if column already exists
    
    // Create index on timestamp for faster chronological queries
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_timestamp ON ban_logs(timestamp DESC)",
        [],
    )?;
    
    // Create index on admin and target for faster searches
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_admin ON ban_logs(admin)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_target ON ban_logs(target)",
        [],
    )?;
    
    Ok(conn)
}

/// Initialize the database - creates file and tables if they don't exist
pub fn init_db() -> Result<(), String> {
    get_connection().map_err(|e| e.to_string())?;
    Ok(())
}

/// Add a moderation log entry (ban or warn)
/// Returns the row ID if inserted, or the existing ID if a duplicate timestamp already exists
/// timestamp: The timestamp extracted from the log line (format: YYYY.MM.DD HH:MM:SS)
/// action_type: "ban" or "warn"
/// location: "world_id:instance_id" or "N/A" (current instance when event occurred)
pub fn add_ban_log(admin: String, target: String, reason: String, timestamp: String, action_type: String, location: String) -> Result<i64, String> {
    let start_time = std::time::Instant::now();
    crate::debug_println!("[PERF] add_ban_log START (action: {}, admin: {}, target: {})", action_type, admin, target);
    
    let conn_start = std::time::Instant::now();
    let conn = get_connection().map_err(|e| e.to_string())?;
    let conn_duration = conn_start.elapsed();
    crate::debug_println!("[PERF] add_ban_log get_connection: {:.2}ms", conn_duration.as_secs_f64() * 1000.0);
    
    // Time-based deduplication: Check if there's an existing entry for the same target/reason
    // within 3 seconds (regardless of admin)
    let check_start = std::time::Instant::now();
    
    // Parse the timestamp to check for nearby entries
    // Format: YYYY.MM.DD HH:MM:SS
    let parsed_timestamp = chrono::NaiveDateTime::parse_from_str(&timestamp, "%Y.%m.%d %H:%M:%S")
        .ok();
    
    let existing: Option<i64> = if let Some(ts) = parsed_timestamp {
        // Check for entries with same target and reason within 3 seconds
        // We check backwards in time (3 seconds before current timestamp)
        // to find the "first" entry in the 3-second window
        let window_start = ts - chrono::Duration::seconds(3);
        let window_end = ts; // Current timestamp
        
        let window_start_str = window_start.format("%Y.%m.%d %H:%M:%S").to_string();
        let window_end_str = window_end.format("%Y.%m.%d %H:%M:%S").to_string();
        
        // Find the earliest entry with same target and reason within the window
        let mut stmt = conn
            .prepare(
                "SELECT id FROM ban_logs 
                 WHERE target = ?1 
                 AND reason = ?2 
                 AND timestamp >= ?3 
                 AND timestamp <= ?4
                 ORDER BY timestamp ASC
                 LIMIT 1"
            )
            .map_err(|e| e.to_string())?;
        
        stmt.query_row(
            rusqlite::params![target, reason, window_start_str, window_end_str],
            |row| row.get(0)
        )
        .optional()
        .map_err(|e| e.to_string())?
    } else {
        // Fallback: if timestamp parsing fails, check for exact match on target + reason + timestamp
        let mut stmt = conn
            .prepare(
                "SELECT id FROM ban_logs 
                 WHERE target = ?1 
                 AND reason = ?2 
                 AND timestamp = ?3
                 LIMIT 1"
            )
            .map_err(|e| e.to_string())?;
        
        stmt.query_row(
            rusqlite::params![target, reason, timestamp],
            |row| row.get(0)
        )
        .optional()
        .map_err(|e| e.to_string())?
    };
    
    let check_duration = check_start.elapsed();
    crate::debug_println!("[PERF] add_ban_log duplicate check: {:.2}ms", check_duration.as_secs_f64() * 1000.0);
    
    // If duplicate exists (same target/reason within 3 seconds), return the existing ID without inserting
    if let Some(existing_id) = existing {
        let total_duration = start_time.elapsed();
        crate::debug_println!("[PERF] add_ban_log END (duplicate - same target/reason within 3s): {:.2}ms", total_duration.as_secs_f64() * 1000.0);
        return Ok(existing_id);
    }
    
    // Insert new entry
    let insert_start = std::time::Instant::now();
    let loc = if location.is_empty() { "N/A" } else { location.as_str() };
    conn.execute(
        "INSERT INTO ban_logs (admin, target, reason, timestamp, action_type, location) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![admin.clone(), target.clone(), reason.clone(), timestamp, action_type, loc],
    )
    .map_err(|e| e.to_string())?;
    let insert_duration = insert_start.elapsed();
    crate::debug_println!("[PERF] add_ban_log INSERT: {:.2}ms", insert_duration.as_secs_f64() * 1000.0);
    
    let row_id = conn.last_insert_rowid();
    
    // Export to API asynchronously (don't block on this)
    let admin_clone = admin.clone();
    let target_clone = target.clone();
    let reason_clone = reason.clone();
    let action_type_clone = action_type.clone();
    let location_clone = location.clone();
    async_runtime::spawn(async move {
        if let Err(e) = send_log_to_api(admin_clone, target_clone, reason_clone, action_type_clone, location_clone).await {
            crate::debug_eprintln!("[world_mod_logs] Failed to export log to API: {}", e);
        }
    });
    
    let total_duration = start_time.elapsed();
    crate::debug_println!("[PERF] add_ban_log END: {:.2}ms", total_duration.as_secs_f64() * 1000.0);
    Ok(row_id)
}

/// Get all ban logs, ordered chronologically (newest first)
pub fn get_all_ban_logs() -> Result<Vec<BanLogEntry>, String> {
    let start_time = std::time::Instant::now();
    crate::debug_println!("[PERF] get_all_ban_logs START");
    
    let conn_start = std::time::Instant::now();
    let conn = get_connection().map_err(|e| e.to_string())?;
    let conn_duration = conn_start.elapsed();
    crate::debug_println!("[PERF] get_all_ban_logs get_connection: {:.2}ms", conn_duration.as_secs_f64() * 1000.0);
    
    let query_start = std::time::Instant::now();
    let mut stmt = conn
        .prepare("SELECT id, admin, target, reason, timestamp, action_type, COALESCE(location, 'N/A') FROM ban_logs ORDER BY timestamp DESC")
        .map_err(|e| e.to_string())?;
    
    let entries = stmt
        .query_map([], |row| {
            Ok(BanLogEntry {
                id: row.get(0)?,
                admin: row.get(1)?,
                target: row.get(2)?,
                reason: row.get(3)?,
                timestamp: row.get(4)?,
                action_type: row.get(5).unwrap_or_else(|_| "ban".to_string()),
                location: row.get(6).unwrap_or_else(|_| "N/A".to_string()),
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let query_duration = query_start.elapsed();
    crate::debug_println!("[PERF] get_all_ban_logs query: {:.2}ms ({} entries)", query_duration.as_secs_f64() * 1000.0, entries.len());
    
    let total_duration = start_time.elapsed();
    crate::debug_println!("[PERF] get_all_ban_logs END: {:.2}ms", total_duration.as_secs_f64() * 1000.0);
    Ok(entries)
}

/// Search ban logs by admin or target name
pub fn search_ban_logs(query: &str) -> Result<Vec<BanLogEntry>, String> {
    let start_time = std::time::Instant::now();
    crate::debug_println!("[PERF] search_ban_logs START (query: {})", query);
    
    let conn_start = std::time::Instant::now();
    let conn = get_connection().map_err(|e| e.to_string())?;
    let conn_duration = conn_start.elapsed();
    crate::debug_println!("[PERF] search_ban_logs get_connection: {:.2}ms", conn_duration.as_secs_f64() * 1000.0);
    
    let search_pattern = format!("%{}%", query);
    
    let query_start = std::time::Instant::now();
    let mut stmt = conn
        .prepare(
            "SELECT id, admin, target, reason, timestamp, action_type, COALESCE(location, 'N/A') FROM ban_logs 
             WHERE admin LIKE ?1 OR target LIKE ?1 
             ORDER BY timestamp DESC"
        )
        .map_err(|e| e.to_string())?;
    
    let entries = stmt
        .query_map(rusqlite::params![search_pattern], |row| {
            Ok(BanLogEntry {
                id: row.get(0)?,
                admin: row.get(1)?,
                target: row.get(2)?,
                reason: row.get(3)?,
                timestamp: row.get(4)?,
                action_type: row.get(5).unwrap_or_else(|_| "ban".to_string()),
                location: row.get(6).unwrap_or_else(|_| "N/A".to_string()),
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let query_duration = query_start.elapsed();
    crate::debug_println!("[PERF] search_ban_logs query: {:.2}ms ({} entries)", query_duration.as_secs_f64() * 1000.0, entries.len());
    
    let total_duration = start_time.elapsed();
    crate::debug_println!("[PERF] search_ban_logs END: {:.2}ms", total_duration.as_secs_f64() * 1000.0);
    Ok(entries)
}

/// Send a moderation log entry to the API endpoint.
/// This is called asynchronously after a successful database insertion
async fn send_log_to_api(admin: String, target: String, reason: String, action_type: String, location: String) -> Result<(), String> {
    // Get all stored tokens (similar to watchlist checks)
    let tokens = crate::modules::group_auth::group_access_tokens::list_group_access_tokens()
        .map_err(|e| format!("Failed to get tokens: {}", e))?;
    
    let access_tokens: Vec<String> = tokens
        .into_iter()
        .map(|t| t.access_token)
        .filter(|t| t.len() >= 32) // Basic validation
        .collect();
    
    // API base URL - should match frontend and other modules
    let api_base = std::env::var("VITE_API_BASE")
        .unwrap_or_else(|_| "https://fch-toolkit.com".to_string());
    
    let url = format!("{}/api/worldlogs", api_base);
    
    // Prepare JSON payload with tokens, action_type, and location (world_id:instance_id)
    let payload = serde_json::json!({
        "admin": admin,
        "target": target,
        "reason": reason,
        "action_type": action_type,
        "location": location,
        "tokens": access_tokens
    });
    
    // Send HTTP POST request
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
    
    crate::debug_println!("[world_mod_logs] Successfully exported log to API");
    Ok(())
}

// Tauri commands

#[tauri::command]
pub fn add_ban_log_entry(admin: String, target: String, reason: String, timestamp: Option<String>, action_type: Option<String>) -> Result<i64, String> {
    // If timestamp not provided, use current time (for manual entries)
    let ts = timestamp.unwrap_or_else(|| chrono::Local::now().format("%Y.%m.%d %H:%M:%S").to_string());
    let action = action_type.unwrap_or_else(|| "ban".to_string());
    add_ban_log(admin, target, reason, ts, action, "N/A".to_string())
}

#[tauri::command]
pub fn get_all_ban_log_entries() -> Result<Vec<BanLogEntry>, String> {
    get_all_ban_logs()
}

#[tauri::command]
pub fn search_ban_log_entries(query: String) -> Result<Vec<BanLogEntry>, String> {
    if query.trim().is_empty() {
        get_all_ban_logs()
    } else {
        search_ban_logs(&query)
    }
}
