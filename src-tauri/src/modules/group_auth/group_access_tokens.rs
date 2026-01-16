// Group Access Tokens: SQLite database for storing authentication tokens
//
// This module stores group access tokens that are used to authenticate
// with the FCH backend API for group watchlist functionality.

use rusqlite::{Connection, Result as SqlResult};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GroupAccessToken {
    pub group_id: String,
    pub group_name: String,
    pub access_token: String,
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
/// Uses the same database as world_mod_logs for consistency
fn db_path() -> PathBuf {
    db_dir().join("fchapp.db")
}

/// Get or create database connection
fn get_connection() -> SqlResult<Connection> {
    let db_path = db_path();
    
    // Ensure directory exists
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
        "CREATE TABLE IF NOT EXISTS group_access (
            group_id TEXT PRIMARY KEY,
            group_name TEXT NOT NULL,
            access_token TEXT NOT NULL
        )",
        [],
    )?;
    
    // Create index for faster lookups
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_group_access_token ON group_access(access_token)",
        [],
    )?;
    
    Ok(conn)
}

/// Initialize the database
pub fn init_db() -> SqlResult<()> {
    get_connection()?;
    Ok(())
}

/// Add or update a group access token
#[tauri::command]
pub fn add_group_access_token(
    group_id: String,
    group_name: String,
    token: String,
) -> Result<(), String> {
    if group_id.trim().is_empty() || token.trim().is_empty() {
        return Err("Missing group_id or token".to_string());
    }
    
    let conn = get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO group_access (group_id, group_name, access_token) VALUES (?1, ?2, ?3)",
        rusqlite::params![group_id, group_name, token]
    ).map_err(|e| e.to_string())?;
    
    Ok(())
}

/// List all group access tokens
#[tauri::command]
pub fn list_group_access_tokens() -> Result<Vec<GroupAccessToken>, String> {
    let conn = get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT group_id, group_name, access_token FROM group_access ORDER BY group_name ASC",
        )
        .map_err(|e| e.to_string())?;
    
    let rows = stmt
        .query_map([], |row| {
            Ok(GroupAccessToken {
                group_id: row.get(0)?,
                group_name: row.get(1)?,
                access_token: row.get(2)?,
            })
        })
        .map_err(|e| e.to_string())?;
    
    let mut tokens = Vec::new();
    for row in rows {
        tokens.push(row.map_err(|e| e.to_string())?);
    }
    
    Ok(tokens)
}

/// Remove a group access token by group_id
#[tauri::command]
pub fn remove_group_access_token(group_id: String) -> Result<(), String> {
    if group_id.trim().is_empty() {
        return Ok(());
    }
    
    let conn = get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM group_access WHERE group_id = ?1",
        rusqlite::params![group_id],
    )
    .map_err(|e| e.to_string())?;
    
    Ok(())
}
