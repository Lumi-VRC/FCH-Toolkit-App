// Database helpers and Tauri commands for join/session logging.
//
// This module maintains a single SQLite database under the app's
// LOCALAPPDATA directory (see notes::notes_dir). We keep both player rows
// and system rows (e.g., instance changes) in one table so that pagination
// and chronology stay simple across restarts.
//
// Conventions
// - Timestamps use the VRChat-friendly format: YYYY.MM.DD HH:MM:SS
// - Player rows: is_system = 0, event_kind = 'join', leave_timestamp is NULL
//   until a leave line appears or we mass-close on purge.
// - System rows: is_system = 1, with event_kind/message/world_id/instance_id/
//   region as applicable. These are used to help Live view avoid displaying system events. Is this efficient....? I'm not sure.
//
//
// Schema (current) (i had to remake the db twice before just making this comment.)
// join_log
//   id              INTEGER PRIMARY KEY AUTOINCREMENT
//   user_id         TEXT NOT NULL
//   username        TEXT
//   join_timestamp  TEXT NOT NULL
//   leave_timestamp TEXT
//   is_system       INTEGER NOT NULL DEFAULT 0
//   event_kind      TEXT
//   message         TEXT
//   world_id        TEXT
//   instance_id     TEXT
//   region          TEXT
//   UNIQUE(user_id, join_timestamp)
//
// app_state
//   key    TEXT PRIMARY KEY
//   value  TEXT NOT NULL
use crate::debug::emit_debug;
use anyhow::Result;
use std::path::PathBuf;
use std::time::Duration;
use tauri::Emitter;
use chrono::Local;
use serde::Serialize;

// Compute the absolute path of the SQLite file so all modules share the same DB.
fn db_path() -> PathBuf {
    super::notes::notes_dir().join("joinlogs.db")
}

fn open_connection() -> rusqlite::Result<rusqlite::Connection> {
    let conn = rusqlite::Connection::open(db_path())?;
    conn.busy_timeout(Duration::from_secs(1))?;
    Ok(conn)
}

// Create tables and add missing columns. Safe to call repeatedly, here because I kept having to remake the db manually.
pub fn db_init() -> rusqlite::Result<()> {
    let conn = open_connection()?;
    // Primary table: join rows and system rows live together, distinguished by
    // is_system (0 for players, 1 for system). We store join and optional leave
    // timestamps so we can reconstruct active users and browse history.
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS join_log (
			id INTEGER PRIMARY KEY AUTOINCREMENT,
			user_id TEXT NOT NULL,
			username TEXT,
			join_timestamp TEXT NOT NULL,
			leave_timestamp TEXT,
			UNIQUE(user_id, join_timestamp)
		);",
    )?;
    // Attempt to add new columns for system events; ignore errors if they already exist
    let _ = conn.execute(
        "ALTER TABLE join_log ADD COLUMN is_system INTEGER NOT NULL DEFAULT 0",
        [],
    );
    let _ = conn.execute("ALTER TABLE join_log ADD COLUMN event_kind TEXT", []);
    let _ = conn.execute("ALTER TABLE join_log ADD COLUMN message TEXT", []);
    let _ = conn.execute("ALTER TABLE join_log ADD COLUMN world_id TEXT", []);
    let _ = conn.execute("ALTER TABLE join_log ADD COLUMN instance_id TEXT", []);
    let _ = conn.execute("ALTER TABLE join_log ADD COLUMN region TEXT", []);
    // Group watchlisted flag to persist historical matches for UI backfill
    let _ = conn.execute(
        "ALTER TABLE join_log ADD COLUMN group_watchlisted INTEGER NOT NULL DEFAULT 0",
        [],
    );
    // Lightweight state store for miscellaneous app/session values
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS app_state (
			key TEXT PRIMARY KEY,
			value TEXT NOT NULL
		);",
    )?;
    // Access tokens for groups (persisted between restarts)
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS group_access (
			group_id TEXT PRIMARY KEY,
			group_name TEXT NOT NULL,
			access_token TEXT NOT NULL
		);",
    )?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS avatar_logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL,
            username TEXT NOT NULL,
            avatar_name TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS avatar_details (
            avatar_name TEXT NOT NULL,
            owner_id TEXT NOT NULL,
            file_id TEXT,
            version INTEGER,
            file_json TEXT,
            security_json TEXT,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (avatar_name, owner_id)
        );
        CREATE TABLE IF NOT EXISTS media_items (
            id TEXT PRIMARY KEY,
            item_type TEXT NOT NULL,
            owner_id TEXT,
            image_url TEXT,
            fetched_at TEXT NOT NULL
        );",
    )?;
    Ok(())
}

/// Normalize avatar names returned by the VRChat API/security endpoint so that
/// they can be matched against raw avatar switch logs.
///
/// Examples:
///   "Avatar - Phybogen - Asset bundle - 2019..." -> "Phybogen"
///   "Avatar - Foo Bar - Asset bundle" -> "Foo Bar"
/// Strings that do not match the pattern are trimmed of surrounding
/// whitespace and returned unchanged.
pub fn normalize_avatar_name(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    const PREFIX: &str = "Avatar - ";
    const SUFFIX: &str = " - Asset bundle";

    let mut core = trimmed;
    if let Some(rest) = core.strip_prefix(PREFIX) {
        core = rest;
    }

    let core = if let Some(idx) = core.find(SUFFIX) {
        core.get(..idx).unwrap_or(core)
    } else {
        core
    };

    let mut candidate = core.trim().to_string();
    if candidate.is_empty() {
        return candidate;
    }

    let ascii_open = candidate.chars().filter(|&c| c == '(').count();
    let ascii_close = candidate.chars().filter(|&c| c == ')').count();
    if ascii_open > ascii_close {
        candidate.push_str(&")".repeat(ascii_open - ascii_close));
    }

    let fw_open = candidate.chars().filter(|&c| c == '（').count();
    let fw_close = candidate.chars().filter(|&c| c == '）').count();
    if fw_open > fw_close {
        candidate.push_str(&"）".repeat(fw_open - fw_close));
    }

    candidate
}

// Store an arbitrary key/value (e.g., last_instance_join_ts)
pub fn db_set_state(key: &str, value: &str) -> Result<()> {
    db_init()?;
    let conn = open_connection()?;
    conn.execute(
        "INSERT OR REPLACE INTO app_state (key, value) VALUES (?1, ?2)",
        rusqlite::params![key, value],
    )?;
    Ok(())
    // OK :DDDDD
}

// Retrieve a value previously written to app_state
pub fn db_get_state(key: &str) -> Result<Option<String>> {
    db_init()?;
    let conn = open_connection()?;
    let mut stmt = conn.prepare("SELECT value FROM app_state WHERE key = ?1")?;
    let mut rows = stmt.query(rusqlite::params![key])?;
    if let Some(row) = rows.next()? {
        Ok(Some(row.get(0)?))
        // OK :DDDDD
    } else {
        Ok(None)
        // OK :DDDDDDDDD
    }
}

// Legacy helper: insert a generic system event row (kept for future use)
// Gay and cringe, merge with other function later elegantly.
// Merge attempts failed: 5
// Hours wasted: 13
// Alcohol consumed: 2 bottles
pub fn _db_insert_event(
    ts: Option<&str>,
    user_id: Option<&str>,
    username: Option<&str>,
    event: &str,
    world_id: Option<&str>,
    instance_id: Option<&str>,
) {
    let ts = ts.unwrap_or("");
    let user_id = user_id.unwrap_or("");
    if ts.is_empty() || user_id.is_empty() {
        return;
    }
    if db_init().is_err() {
        return;
    }
    if let Ok(conn) = open_connection() {
        // OK :DDDDD
        let _ = conn.execute(
			"INSERT OR IGNORE INTO join_log (user_id, username, join_timestamp, is_system, event_kind, world_id, instance_id) VALUES (?,?,?,?,?,?,?)",
			rusqlite::params![user_id, username, ts, 1i32, event, world_id, instance_id]
		);
    }
}

// Insert a join row for a user; optionally emit a UI event for live updates
// Do not fucking touch this
// Do not fucking touch this
// If you fuck up the DB you have to delete it
// You cannot delete peoples DB's after distribution, bad practice
//
pub fn db_insert_join(
    app: &tauri::AppHandle,
    ts: &str,
    user_id: &str,
    username: &str,
    emit: bool,
) -> Result<()> {
    if ts.is_empty() || user_id.is_empty() {
        return Ok(());
    }
    db_init()?;
    let conn = open_connection()?;
    let mut stmt = conn.prepare("INSERT OR IGNORE INTO join_log (user_id, username, join_timestamp, is_system, event_kind) VALUES (?, ?, ?, 0, 'join')")?;
    let changed = stmt.execute(rusqlite::params![user_id, username, ts])?;
    let id = conn.last_insert_rowid();

    if changed > 0 {
        if emit {
            let payload = serde_json::json!({
                "id": id,
                "userId": user_id,
                "username": username,
                "joinedAt": ts,
                "leftAt": serde_json::Value::Null,
                "groupWatchlisted": serde_json::Value::Null,
            });
            app.emit("db_row_inserted", payload)?;
        }
        emit_debug(app, format!(
            "Join inserted (id={id}) for {user_id} ({username}) at {ts} [emit={emit}, changed={changed}]"
        ));
    } else {
        emit_debug(
            app,
            format!("Join ignored for {user_id} at {ts} (duplicate entry)"),
        );
    }
    Ok(())
    // OK :DD
}

// Close the latest open join for the given user (sets leave_timestamp)
// Lots of checks trigger this to ensure there are no joins left hanging
pub fn db_update_leave(app: &tauri::AppHandle, ts: &str, user_id: &str, emit: bool) -> Result<()> {
    if ts.is_empty() || user_id.is_empty() {
        return Ok(());
    }
    db_init()?;
    let conn = open_connection()?;
    let mut stmt = conn.prepare("SELECT id FROM join_log WHERE user_id = ? AND leave_timestamp IS NULL AND is_system = 0 ORDER BY join_timestamp DESC LIMIT 1")?;
    let mut rows = stmt.query(rusqlite::params![user_id])?;
    if let Some(row) = rows.next()? {
        let id: i64 = row.get(0)?;
        conn.execute(
            "UPDATE join_log SET leave_timestamp = ? WHERE id = ?",
            rusqlite::params![ts, id],
        )?;
        if emit {
            let payload = serde_json::json!({ "id": id, "userId": user_id, "leftAt": ts });
            app.emit("db_row_updated", payload)?;
        }
        emit_debug(
            app,
            format!("Leave timestamp set for {user_id} (row={id}) at {ts} [emit={emit}]"),
        );
        emit_debug(app, format!(
            "Leave operation context: user_id={user_id}, row_id={id}, ts={ts}, emit={emit}, changed_row=true"
        ));
    } else {
        emit_debug(
            app,
            format!("Leave skipped for {user_id} at {ts}: no open join found"),
        );
    }
    Ok(())
    // OK :DDDDDDD
}

// Update leave timestamp by matching the most recent open row for a given username
// Useful when logs do not include the user ID on leave (e.g., "Destroying <username>")
pub fn db_update_leave_by_username(
    app: &tauri::AppHandle,
    ts: &str,
    username: &str,
    emit: bool,
) -> Result<()> {
    if ts.is_empty() || username.trim().is_empty() {
        return Ok(());
    }
    db_init()?;
    let conn = open_connection()?;
    let mut stmt = conn.prepare(
        "SELECT id, user_id FROM join_log WHERE username = ?1 AND leave_timestamp IS NULL AND is_system = 0 ORDER BY join_timestamp DESC LIMIT 1"
    )?;
    let mut rows = stmt.query(rusqlite::params![username])?;
    if let Some(row) = rows.next()? {
        let id: i64 = row.get(0)?;
        let uid: String = row.get(1)?;
        conn.execute(
            "UPDATE join_log SET leave_timestamp = ?1 WHERE id = ?2",
            rusqlite::params![ts, id],
        )?;
        if emit {
            let payload = serde_json::json!({ "id": id, "userId": uid, "leftAt": ts });
            app.emit("db_row_updated", payload)?;
        }
        emit_debug(app, format!(
            "Leave timestamp set via username for {uid} (row={id}) at {ts}; source username={username} [emit={emit}]"
        ));
        emit_debug(app, format!(
            "Leave-by-username context -> uid={uid}, username_input={username}, ts={ts}, emit={emit}, updated_row={id}"
        ));
    } else {
        emit_debug(
            app,
            format!("Leave by username skipped for {username} at {ts}: no open join found"),
        );
    }
    Ok(())
}

// Insert a system event row (e.g., instance_changed) with optional context
// Planned updates will use context window
// Update: I forgot what I update I planned for that part. I'm sure it was a good idea.
pub fn db_insert_system_event(
    app: &tauri::AppHandle,
    ts: &str,
    event_kind: &str,
    message: Option<&str>,
    world_id: Option<&str>,
    instance_id: Option<&str>,
    region: Option<&str>,
    emit: bool,
) -> Result<()> {
    if ts.is_empty() {
        return Ok(());
    }
    db_init()?;
    let conn = open_connection()?;
    let mut stmt = conn.prepare("INSERT INTO join_log (user_id, username, join_timestamp, is_system, event_kind, message, world_id, instance_id, region) VALUES ('system', NULL, ?, 1, ?, ?, ?, ?, ?)")?;
    let _ = stmt.execute(rusqlite::params![
        ts,
        event_kind,
        message,
        world_id,
        instance_id,
        region
    ])?;
    // using " _ " as a name is gay
    if emit {
        let payload = serde_json::json!({
            "type": "system",
            "ts": ts,
            "message": message,
            "worldId": world_id,
            "instanceId": instance_id,
            "region": region
        });
        app.emit("db_row_inserted", payload)?;
    }
    let message_fmt = format!(
		"System event inserted: kind={event_kind}, ts={ts}, world={:?}, instance={:?}, region={:?}, message={:?}, emit={emit}",
		world_id,
		instance_id,
		region,
		message,
	);
    emit_debug(app, message_fmt);
    Ok(())
    // OK :DDDDD
}

// Mark all open (non-system) rows as left at the given timestamp
// This is used to close all open joins when an instance changes, or when certain conditions are met. (Close app, join/leave instance, vrchat close, etc)
pub fn db_purge_all(app: &tauri::AppHandle, ts: &str, emit: bool) -> Result<()> {
    if ts.is_empty() {
        return Ok(());
    }
    db_init()?;
    let conn = open_connection()?;
    conn.execute(
        "UPDATE join_log SET leave_timestamp = ? WHERE leave_timestamp IS NULL AND is_system = 0",
        rusqlite::params![ts],
    )?;
    if emit {
        app.emit("db_purged", serde_json::json!({ "ts": ts }))?;
    }
    emit_debug(
        app,
        format!("Purge applied to open joins at {ts} [emit={emit}]"),
    );
    Ok(())
}

// Close duplicate open joins per user, keeping only the latest open row
#[tauri::command]
pub fn dedupe_open_joins(app: tauri::AppHandle) -> Result<usize, String> {
    db_init().map_err(|e| e.to_string())?;
    let conn = open_connection().map_err(|e| e.to_string())?;
    // Find users with multiple open rows
    let mut stmt = conn.prepare("SELECT user_id FROM join_log WHERE leave_timestamp IS NULL AND is_system = 0 GROUP BY user_id HAVING COUNT(*) > 1").map_err(|e| e.to_string())?;
    let user_ids = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    let mut total_closed = 0usize;
    // Get a reasonable timestamp to mark older rows closed
    let ts = super::db::db_get_state("last_instance_join_ts")
        .unwrap_or(None)
        .unwrap_or_else(|| chrono::Local::now().format("%Y.%m.%d %H:%M:%S").to_string());
    for uid_res in user_ids {
        let uid = match uid_res {
            Ok(u) => u,
            Err(_) => continue,
        };
        let mut q = conn.prepare("SELECT id, join_timestamp FROM join_log WHERE user_id = ?1 AND leave_timestamp IS NULL AND is_system = 0 ORDER BY join_timestamp DESC").map_err(|e| e.to_string())?;
        let rows = q
            .query_map(rusqlite::params![&uid], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?;
        let mut to_close: Vec<i64> = Vec::new();
        for (idx, r) in rows.enumerate() {
            if let Ok((id, _jt)) = r {
                if idx > 0 {
                    to_close.push(id);
                }
            }
        }
        for id in to_close {
            let _ = conn.execute(
                "UPDATE join_log SET leave_timestamp = ?1 WHERE id = ?2",
                rusqlite::params![&ts, id],
            );
            total_closed += 1;
            let _ = app.emit(
                "db_row_updated",
                serde_json::json!({ "id": id, "userId": uid, "leftAt": ts }),
            );
        }
    }
    Ok(total_closed)
}

// Persist group_watchlisted flag for all rows of given users in current instance window
#[tauri::command]
pub fn set_group_watchlisted_for_users(user_ids: Vec<String>) -> Result<usize, String> {
    if user_ids.is_empty() {
        return Ok(0);
    }
    db_init().map_err(|e| e.to_string())?;
    let conn = open_connection().map_err(|e| e.to_string())?;
    let since = super::db::db_get_state("last_instance_join_ts").unwrap_or(None);
    let placeholders = (0..user_ids.len())
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(",");
    let sql = if let Some(ref _ts_value) = since {
        format!("UPDATE join_log SET group_watchlisted = 1 WHERE is_system = 0 AND user_id IN ({}) AND join_timestamp >= ?", placeholders)
    } else {
        format!(
            "UPDATE join_log SET group_watchlisted = 1 WHERE is_system = 0 AND user_id IN ({})",
            placeholders
        )
    };
    let mut params: Vec<&dyn rusqlite::ToSql> =
        user_ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
    if let Some(ref ts_value) = since {
        params.push(ts_value);
    }
    let changed = conn
        .execute(&sql, params.as_slice())
        .map_err(|e| e.to_string())? as usize;
    Ok(changed)
}

// Return a page of rows ordered by newest join first (includes system rows)
// I tried really hard to better paginate this but I'm failing miserably
// I need to figure out how to pre-load the next page without lagging the front end, but also without using tiny pages.
// Maybe we can paginate by day and then chunk load data into the page? Not important, it works for now. If people want long-term logs, they can use VRCX. This is just for convenience, really.
#[tauri::command]
pub fn get_join_logs_page(offset: i64, limit: i64) -> Result<Vec<serde_json::Value>, String> {
    db_init().map_err(|e| e.to_string())?;
    let conn = open_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare("SELECT id, user_id, username, join_timestamp, leave_timestamp, is_system, event_kind, message, world_id, instance_id, region, group_watchlisted FROM join_log ORDER BY join_timestamp DESC LIMIT ?2 OFFSET ?1").map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![offset, limit], |row| {
            // Yeah this is super ugly and I had to google how to do this.
            // I've never messed with pagination in a local app before, just web pages... Managing lag is harder than expected.
            // Will consult with friends later to see if there's a better way to do this.
            // Todd Howard: It Just Works.
            Ok(serde_json::json!({ // OK :DDDDD
                "id": row.get::<_, i64>(0)?,
                "userId": row.get::<_, String>(1)?,
                "username": row.get::<_, Option<String>>(2)?,
                "joinedAt": row.get::<_, String>(3)?,
                "leftAt": row.get::<_, Option<String>>(4)?,
                "isSystem": row.get::<_, i64>(5)? == 1,
                "eventKind": row.get::<_, Option<String>>(6)?,
                "message": row.get::<_, Option<String>>(7)?,
                "worldId": row.get::<_, Option<String>>(8)?,
                "instanceId": row.get::<_, Option<String>>(9)?,
                "region": row.get::<_, Option<String>>(10)?,
                "groupWatchlisted": row.get::<_, Option<i64>>(11)?.unwrap_or(0) == 1,
            }))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| e.to_string())?);
    }
    Ok(out)
    // OK :DD
}

// --- Group access token storage ---

#[tauri::command]
pub fn add_group_access_token(
    group_id: String,
    group_name: String,
    token: String,
) -> Result<(), String> {
    if group_id.trim().is_empty() || token.trim().is_empty() {
        return Err("Missing group or token".into());
    }
    db_init().map_err(|e| e.to_string())?;
    let conn = open_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO group_access (group_id, group_name, access_token) VALUES (?1, ?2, ?3)",
        rusqlite::params![group_id, group_name, token]
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn list_group_access_tokens() -> Result<Vec<serde_json::Value>, String> {
    db_init().map_err(|e| e.to_string())?;
    let conn = open_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT group_id, group_name, access_token FROM group_access ORDER BY group_name ASC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(serde_json::json!({
                "groupId": row.get::<_, String>(0)?,
                "name": row.get::<_, String>(1)?,
                "token": row.get::<_, String>(2)?,
            }))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

#[tauri::command]
pub fn remove_group_access_token(group_id: String) -> Result<(), String> {
    if group_id.trim().is_empty() {
        return Ok(());
    }
    db_init().map_err(|e| e.to_string())?;
    let conn = rusqlite::Connection::open(db_path()).map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM group_access WHERE group_id = ?1",
        rusqlite::params![group_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

// Return currently active users (no leave_timestamp), filtered by session start
// This is used to display the active users in the live view.
// SQL is fun to learn but making a mistake and having to purge the db is so fking annoying.
#[tauri::command]
pub fn get_active_join_logs() -> Result<Vec<serde_json::Value>, String> {
    db_init().map_err(|e| e.to_string())?;
    let conn = rusqlite::Connection::open(db_path()).map_err(|e| e.to_string())?;

    let last_join_ts = super::db::db_get_state("last_instance_join_ts").unwrap_or(None);

    let mut query = "SELECT id, user_id, username, join_timestamp, leave_timestamp FROM join_log WHERE leave_timestamp IS NULL AND is_system = 0".to_string();
    if last_join_ts.is_some() {
        query.push_str(" AND join_timestamp >= ?1");
    }
    query.push_str(" ORDER BY join_timestamp ASC");

    let mut stmt = conn.prepare(&query).map_err(|e| e.to_string())?;

    // Maps are fun. I hadn't had a chance to use these since college.
    let map_row = |row: &rusqlite::Row| {
        Ok(serde_json::json!({
            "id": row.get::<_, i64>(0)?,
            "userId": row.get::<_, String>(1)?,
            "username": row.get::<_, Option<String>>(2)?,
            "joinedAt": row.get::<_, String>(3)?,
            "leftAt": row.get::<_, Option<String>>(4)?,
        }))
    };

    let rows = if let Some(ts) = &last_join_ts {
        stmt.query_map(rusqlite::params![ts], map_row)
    } else {
        stmt.query_map([], map_row)
    }
    .map_err(|e| e.to_string())?;

    // One line functions are pretty to look at, but I'm starting to think that I shouldn't use them.
    // I wonder what the standard is? Or if they help performance at all? Probably just compile time.
    // Maybe I'll look that up later.
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| e.to_string())?);
    }
    Ok(out)
    // OK :DDDD
}

// Lookup the latest known non-empty username for a given user_id from join_log
#[tauri::command]
pub fn get_latest_username_for_user(user_id: String) -> Result<serde_json::Value, String> {
    if user_id.trim().is_empty() {
        return Ok(serde_json::json!({ "username": serde_json::Value::Null }));
    }
    db_init().map_err(|e| e.to_string())?;
    let conn = rusqlite::Connection::open(db_path()).map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare("SELECT username FROM join_log WHERE user_id = ?1 AND username IS NOT NULL AND username <> '' ORDER BY join_timestamp DESC LIMIT 1").map_err(|e| e.to_string())?;
    let mut rows = stmt
        .query(rusqlite::params![user_id])
        .map_err(|e| e.to_string())?;
    if let Some(row) = rows.next().map_err(|e| e.to_string())? {
        let uname: String = row.get(0).map_err(|e: rusqlite::Error| e.to_string())?;
        Ok(serde_json::json!({ "username": uname }))
    } else {
        Ok(serde_json::json!({ "username": serde_json::Value::Null }))
    }
}

// Danger: delete all rows (explicit action from the UI)
// Manly used for debugging, but I'm keeping it here on the off chance people want it.
// Oh, and for beta testers.
#[tauri::command]
pub fn purge_join_log_table() -> Result<(), String> {
    db_init().map_err(|e| e.to_string())?;
    let conn = rusqlite::Connection::open(db_path()).map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM join_log", [])
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn db_insert_avatar_log(
    app: &tauri::AppHandle,
    ts: &str,
    username: &str,
    avatar_name: &str,
) -> Result<()> {
    if ts.trim().is_empty() || username.trim().is_empty() || avatar_name.trim().is_empty() {
        return Ok(());
    }
    db_init()?;
    let conn = open_connection()?;
    conn.execute(
        "INSERT INTO avatar_logs (timestamp, username, avatar_name) VALUES (?1, ?2, ?3)",
        rusqlite::params![ts, username, avatar_name],
    )?;
    emit_debug(
        app,
        format!("[DB] avatar_logs inserted :: ts={ts} user={username} avatar={avatar_name}"),
    );
    Ok(())
}

pub fn db_insert_avatar_details(
    app: &tauri::AppHandle,
    avatar_name: &str,
    owner_id: &str,
    file_id: Option<&str>,
    version: Option<i32>,
    file_json: Option<&serde_json::Value>,
    security_json: Option<&serde_json::Value>,
) -> Result<()> {
    let trimmed_input = avatar_name.trim();
    if trimmed_input.is_empty() {
        return Ok(());
    }
    let normalized = normalize_avatar_name(avatar_name);
    if normalized.is_empty() {
        return Ok(());
    }
    let owner_id = if owner_id.trim().is_empty() {
        "unknown_owner"
    } else {
        owner_id
    };
    db_init()?;
    let conn = open_connection()?;
    if trimmed_input != normalized {
        let _ = conn.execute(
            "DELETE FROM avatar_details WHERE avatar_name = ?1 AND owner_id = ?2",
            rusqlite::params![trimmed_input, owner_id],
        );
    }
    let now = chrono::Local::now().format("%Y.%m.%d %H:%M:%S").to_string();
    emit_debug(
        app,
        format!(
            "[DB] preparing avatar_details upsert :: avatar={} owner={} file={} version={:?}",
            normalized,
            owner_id,
            file_id.unwrap_or(""),
            version
        ),
    );
    conn.execute(
        "INSERT OR REPLACE INTO avatar_details (avatar_name, owner_id, file_id, version, file_json, security_json, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            normalized,
            owner_id,
            file_id.unwrap_or(""),
            version.unwrap_or_default(),
            file_json.map(|v| v.to_string()).unwrap_or_default(),
            security_json.map(|v| v.to_string()).unwrap_or_default(),
            now
        ],
    )?;
    emit_debug(
        app,
        format!("[DB] avatar_details upserted :: avatar={normalized} owner={owner_id}"),
    );
    Ok(())
}

pub fn db_upsert_media_item(
    id: &str,
    item_type: &str,
    owner_id: Option<&str>,
    image_url: Option<&str>,
) -> Result<()> {
    db_init()?;
    let conn = open_connection()?;
    let fetched_at = Local::now().format("%Y.%m.%d %H:%M:%S").to_string();
    conn.execute(
        "INSERT INTO media_items (id, item_type, owner_id, image_url, fetched_at) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(id) DO UPDATE SET item_type = excluded.item_type, owner_id = excluded.owner_id, image_url = excluded.image_url, fetched_at = excluded.fetched_at",
        rusqlite::params![id, item_type, owner_id, image_url, fetched_at],
    )?;
    Ok(())
}

pub fn db_clear_media_items() -> Result<()> {
    db_init()?;
    let conn = open_connection()?;
    conn.execute("DELETE FROM media_items", [])?;
    Ok(())
}

#[tauri::command]
pub fn db_get_avatar_details(avatar_name: String) -> Result<Vec<serde_json::Value>, String> {
    let normalized = normalize_avatar_name(&avatar_name);
    let search_name = if normalized.is_empty() {
        avatar_name.trim().to_string()
    } else {
        normalized
    };
    if search_name.trim().is_empty() {
        return Ok(Vec::new());
    }
    db_init().map_err(|e| e.to_string())?;
    let conn = open_connection().map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    let mut stmt = conn
        .prepare(
            "SELECT owner_id, file_id, version, file_json, security_json, updated_at
             FROM avatar_details WHERE avatar_name = ?1 ORDER BY updated_at DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![&search_name], |row| {
            let file_raw: Option<String> = row.get(3)?;
            let security_raw: Option<String> = row.get(4)?;
            let file_val = file_raw
                .as_ref()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                .unwrap_or(serde_json::Value::Null);
            let security_val = security_raw
                .as_ref()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                .unwrap_or(serde_json::Value::Null);

            let perf_from_file = file_val
                .get("performance")
                .and_then(|p| p.get("performanceRating"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let perf_from_security_direct = security_val
                .get("performanceRating")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let perf_from_security_nested = security_val
                .get("performance")
                .and_then(|p| p.get("performanceRating"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let performance_rating = perf_from_file
                .or(perf_from_security_direct)
                .or(perf_from_security_nested)
                .map(serde_json::Value::String)
                .unwrap_or(serde_json::Value::Null);

            Ok(serde_json::json!({
                "ownerId": row.get::<_, String>(0)?,
                "fileId": row.get::<_, String>(1)?,
                "version": row.get::<_, i32>(2)?,
                "file": file_val,
                "security": security_val,
                "updatedAt": row.get::<_, String>(5)?,
                "performanceRating": performance_rating,
            }))
        })
        .map_err(|e| e.to_string())?;
    for r in rows {
        out.push(r.map_err(|e| e.to_string())?);
    }

    Ok(out)
}

#[tauri::command]
pub fn list_recent_avatar_details(limit: Option<i64>) -> Result<Vec<serde_json::Value>, String> {
    db_init().map_err(|e| e.to_string())?;
    let conn = open_connection().map_err(|e| e.to_string())?;
    let lim = limit.unwrap_or(10).max(1);
    let mut stmt = conn
        .prepare(
            "SELECT avatar_name, owner_id, file_id, version, file_json, security_json, updated_at
             FROM avatar_details
             ORDER BY updated_at DESC
             LIMIT ?1",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![lim], |row| {
            let raw_file: Option<String> = row.get(4)?;
            let raw_security: Option<String> = row.get(5)?;
            let stored_name: String = row.get(0)?;
            let display_name = normalize_avatar_name(&stored_name);
            let file_val = raw_file
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                .unwrap_or(serde_json::Value::Null);
            let security_val = raw_security
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                .unwrap_or(serde_json::Value::Null);
            let perf_from_file = file_val
                .get("performance")
                .and_then(|p| p.get("performanceRating"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let perf_from_security_direct = security_val
                .get("performanceRating")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let perf_from_security_nested = security_val
                .get("performance")
                .and_then(|p| p.get("performanceRating"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let performance_rating = perf_from_file
                .or(perf_from_security_direct)
                .or(perf_from_security_nested)
                .unwrap_or_default();
            Ok(serde_json::json!({
                "avatarName": if display_name.is_empty() { stored_name } else { display_name },
                "ownerId": row.get::<_, String>(1)?,
                "fileId": row.get::<_, String>(2)?,
                "version": row.get::<_, i32>(3)?,
                "file": file_val,
                "security": security_val,
                "updatedAt": row.get::<_, String>(6)?,
                "performanceRating": if performance_rating.is_empty() {
                    serde_json::Value::Null
                } else {
                    serde_json::Value::String(performance_rating)
                },
            }))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

#[tauri::command]
pub fn list_distinct_avatar_details(
    offset: Option<i64>,
    limit: Option<i64>,
    search: Option<String>,
) -> Result<serde_json::Value, String> {
    db_init().map_err(|e| e.to_string())?;
    let conn = open_connection().map_err(|e| e.to_string())?;

    let lim = limit.unwrap_or(100).max(1);
    let off = offset.unwrap_or(0).max(0);
    let filter = search.unwrap_or_default();
    let trimmed = filter.trim();
    let like = if trimmed.is_empty() {
        "%".to_string()
    } else {
        format!("%{}%", trimmed)
    };

    let mut count_stmt = conn
        .prepare(
            "SELECT COUNT(*) FROM (
                 SELECT avatar_name
                 FROM avatar_details
                 WHERE avatar_name LIKE ?1
                 GROUP BY avatar_name
             )",
        )
        .map_err(|e| e.to_string())?;
    let total: i64 = count_stmt
        .query_row(rusqlite::params![&like], |row| row.get::<_, i64>(0))
        .map_err(|e| e.to_string())?
        .max(0);

    let mut stmt = conn
        .prepare(
            "WITH latest AS (
                 SELECT avatar_name,
                        owner_id,
                        file_id,
                        version,
                        file_json,
                        security_json,
                        updated_at,
                        ROW_NUMBER() OVER (
                            PARTITION BY avatar_name
                            ORDER BY datetime(updated_at) DESC
                        ) AS rn
                 FROM avatar_details
                 WHERE avatar_name LIKE ?1
             )
             SELECT avatar_name,
                    owner_id,
                    file_id,
                    version,
                    file_json,
                    security_json,
                    updated_at
             FROM latest
             WHERE rn = 1
             ORDER BY datetime(updated_at) DESC
             LIMIT ?2 OFFSET ?3",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(rusqlite::params![&like, lim, off], |row| {
            let raw_file: Option<String> = row.get(4)?;
            let raw_security: Option<String> = row.get(5)?;
            let stored_name: String = row.get(0)?;
            let display_name = normalize_avatar_name(&stored_name);
            let file_val = raw_file
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                .unwrap_or(serde_json::Value::Null);
            let security_val = raw_security
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                .unwrap_or(serde_json::Value::Null);
            let perf_from_file = file_val
                .get("performance")
                .and_then(|p| p.get("performanceRating"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let perf_from_security_direct = security_val
                .get("performanceRating")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let perf_from_security_nested = security_val
                .get("performance")
                .and_then(|p| p.get("performanceRating"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let performance_rating = perf_from_file
                .or(perf_from_security_direct)
                .or(perf_from_security_nested)
                .unwrap_or_default();

            Ok(serde_json::json!({
                "avatarName": if display_name.is_empty() { stored_name } else { display_name },
                "ownerId": row.get::<_, String>(1)?,
                "fileId": row.get::<_, String>(2)?,
                "version": row.get::<_, i32>(3)?,
                "file": file_val,
                "security": security_val,
                "updatedAt": row.get::<_, String>(6)?,
                "performanceRating": if performance_rating.is_empty() {
                    serde_json::Value::Null
                } else {
                    serde_json::Value::String(performance_rating)
                },
            }))
        })
        .map_err(|e| e.to_string())?;

    let mut items = Vec::new();
    for r in rows {
        items.push(r.map_err(|e| e.to_string())?);
    }

    Ok(serde_json::json!({
        "total": total,
        "items": items,
        "offset": off,
        "limit": lim,
    }))
}

#[tauri::command]
pub fn list_recent_avatar_logs(limit: Option<i64>) -> Result<Vec<serde_json::Value>, String> {
    db_init().map_err(|e| e.to_string())?;
    let conn = rusqlite::Connection::open(db_path()).map_err(|e| e.to_string())?;
    let lim = limit.unwrap_or(10).max(1);
    let mut stmt = conn
        .prepare(
            "SELECT timestamp, username, avatar_name
             FROM avatar_logs
             ORDER BY timestamp DESC
             LIMIT ?1",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![lim], |row| {
            let timestamp: String = row.get(0)?;
            let username: String = row.get(1)?;
            let stored_name: String = row.get(2)?;
            let display_name = normalize_avatar_name(&stored_name);
            Ok(serde_json::json!({
                "timestamp": timestamp,
                "username": username,
                "avatarName": if display_name.is_empty() { stored_name } else { display_name },
            }))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

#[tauri::command]
pub fn get_latest_avatar_for_user(
    user_id: Option<String>,
    username: Option<String>,
) -> Result<serde_json::Value, String> {
    db_init().map_err(|e| e.to_string())?;
    let conn = rusqlite::Connection::open(db_path()).map_err(|e| e.to_string())?;

    if let Some(name) = username.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let mut stmt = conn
            .prepare("SELECT username, avatar_name, timestamp FROM avatar_logs WHERE username = ?1 ORDER BY timestamp DESC LIMIT 1")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query(rusqlite::params![name])
            .map_err(|e| e.to_string())?;
        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let uname: String = row.get(0).map_err(|e: rusqlite::Error| e.to_string())?;
            let avatar_name: String = row.get(1).map_err(|e: rusqlite::Error| e.to_string())?;
            let ts: String = row.get(2).map_err(|e: rusqlite::Error| e.to_string())?;
            return Ok(serde_json::json!({
                "username": uname,
                "avatarName": avatar_name,
                "timestamp": ts,
            }));
        }
    }

    if let Some(uid) = user_id.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let mut stmt = conn
            .prepare("SELECT username FROM join_log WHERE user_id = ?1 AND username IS NOT NULL AND username <> '' ORDER BY join_timestamp DESC LIMIT 1")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query(rusqlite::params![uid])
            .map_err(|e| e.to_string())?;
        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let name: String = row.get(0).map_err(|e: rusqlite::Error| e.to_string())?;
            let mut stmt = conn
                .prepare("SELECT username, avatar_name, timestamp FROM avatar_logs WHERE username = ?1 ORDER BY timestamp DESC LIMIT 1")
                .map_err(|e| e.to_string())?;
            let mut rows = stmt
                .query(rusqlite::params![&name])
                .map_err(|e| e.to_string())?;
            if let Some(row) = rows.next().map_err(|e| e.to_string())? {
                let uname: String = row.get(0).map_err(|e: rusqlite::Error| e.to_string())?;
                let avatar_name: String = row.get(1).map_err(|e: rusqlite::Error| e.to_string())?;
                let ts: String = row.get(2).map_err(|e: rusqlite::Error| e.to_string())?;
                return Ok(serde_json::json!({
                    "username": uname,
                    "avatarName": avatar_name,
                    "timestamp": ts,
                }));
            }
        }
    }

    Ok(serde_json::json!({
        "username": serde_json::Value::Null,
        "avatarName": serde_json::Value::Null,
        "timestamp": serde_json::Value::Null,
    }))
}

#[tauri::command]
pub fn insert_avatar_details(
    app: tauri::AppHandle,
    avatar_name: String,
    owner_id: Option<String>,
    file_id: Option<String>,
    version: Option<i32>,
    file_json: Option<serde_json::Value>,
    security_json: Option<serde_json::Value>,
) -> Result<(), String> {
    let avatar = avatar_name.trim().to_string();
    if avatar.is_empty() {
        emit_debug(&app, "[DB] insert_avatar_details skipped: empty avatar name".to_string());
        return Ok(());
    }
    let owner = owner_id.unwrap_or_default();
    let owner_trimmed = owner.trim();
    let file_id_trimmed = file_id
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let file_ref = file_json.as_ref();
    let security_ref = security_json.as_ref();
    db_insert_avatar_details(
        &app,
        &avatar,
        owner_trimmed,
        file_id_trimmed.as_deref(),
        version,
        file_ref,
        security_ref,
    )
    .map_err(|e| e.to_string())
}

pub fn db_get_media_items(limit: usize) -> Result<Vec<MediaItem>> {
    db_init()?;
    let conn = open_connection()?;
    let mut stmt = conn.prepare(
        "SELECT id, item_type, owner_id, image_url, fetched_at FROM media_items ORDER BY datetime(fetched_at) DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(rusqlite::params![limit as i64], |row| {
        Ok(MediaItem {
            id: row.get(0)?,
            item_type: row.get(1)?,
            owner_id: row.get::<_, Option<String>>(2)?,
            image_url: row.get::<_, Option<String>>(3)?,
            fetched_at: row.get(4)?,
        })
    })?;
    let mut items = Vec::new();
    for row in rows {
        if let Ok(item) = row {
            items.push(item);
        }
    }
    Ok(items)
}

#[derive(Serialize, Debug, Clone)]
pub struct MediaItem {
    pub id: String,
    pub item_type: String,
    pub owner_id: Option<String>,
    pub image_url: Option<String>,
    pub fetched_at: String,
}

#[tauri::command]
pub fn get_media_items(limit: Option<usize>) -> Result<Vec<MediaItem>, String> {
    let lim = limit.unwrap_or(200);
    db_get_media_items(lim).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clear_media_items() -> Result<(), String> {
    db_clear_media_items().map_err(|e| e.to_string())
}
