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
use std::path::PathBuf;
use anyhow::Result;
use tauri::Emitter;

// Compute the absolute path of the SQLite file so all modules share the same DB.
fn db_path() -> PathBuf { super::notes::notes_dir().join("joinlogs.db") }

// Create tables and add missing columns. Safe to call repeatedly, here because I kept having to remake the db manually.
pub fn db_init() -> rusqlite::Result<()> {
	let p = db_path();
	let conn = rusqlite::Connection::open(p)?;
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
		);"
	)?;
	// Attempt to add new columns for system events; ignore errors if they already exist
	let _ = conn.execute("ALTER TABLE join_log ADD COLUMN is_system INTEGER NOT NULL DEFAULT 0", []);
	let _ = conn.execute("ALTER TABLE join_log ADD COLUMN event_kind TEXT", []);
	let _ = conn.execute("ALTER TABLE join_log ADD COLUMN message TEXT", []);
	let _ = conn.execute("ALTER TABLE join_log ADD COLUMN world_id TEXT", []);
	let _ = conn.execute("ALTER TABLE join_log ADD COLUMN instance_id TEXT", []);
	let _ = conn.execute("ALTER TABLE join_log ADD COLUMN region TEXT", []);
	// Lightweight state store for miscellaneous app/session values
	conn.execute_batch(
		"CREATE TABLE IF NOT EXISTS app_state (
			key TEXT PRIMARY KEY,
			value TEXT NOT NULL
		);"
	)?;
	Ok(())
}

// Store an arbitrary key/value (e.g., last_instance_join_ts)
pub fn db_set_state(key: &str, value: &str) -> Result<()> {
	db_init()?;
	let conn = rusqlite::Connection::open(db_path())?;
	conn.execute("INSERT OR REPLACE INTO app_state (key, value) VALUES (?1, ?2)", rusqlite::params![key, value])?;
	Ok(())
	// OK :DDDDD
}

// Retrieve a value previously written to app_state
pub fn db_get_state(key: &str) -> Result<Option<String>> {
	db_init()?;
	let conn = rusqlite::Connection::open(db_path())?;
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
pub fn _db_insert_event(ts: Option<&str>, user_id: Option<&str>, username: Option<&str>, event: &str, world_id: Option<&str>, instance_id: Option<&str>) {
	let ts = ts.unwrap_or("");
	let user_id = user_id.unwrap_or("");
	if ts.is_empty() || user_id.is_empty() { return; }
	if db_init().is_err() { return; }
	if let Ok(conn) = rusqlite::Connection::open(db_path()) {
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
pub fn db_insert_join(app: &tauri::AppHandle, ts: &str, user_id: &str, username: &str, emit: bool) -> Result<()> {
	if ts.is_empty() || user_id.is_empty() { return Ok(()); }
	db_init()?;
	let conn = rusqlite::Connection::open(db_path())?;
	let mut stmt = conn.prepare("INSERT OR IGNORE INTO join_log (user_id, username, join_timestamp, is_system, event_kind) VALUES (?, ?, ?, 0, 'join')")?;
	let changed = stmt.execute(rusqlite::params![user_id, username, ts])?;
	let id = conn.last_insert_rowid();

	if emit && changed > 0 {
		let payload = serde_json::json!({
			"id": id,
			"userId": user_id,
			"username": username,
			"joinedAt": ts,
			"leftAt": serde_json::Value::Null,
		});
		app.emit("db_row_inserted", payload)?;
	}
	Ok(())
	// OK :DD
}

// Close the latest open join for the given user (sets leave_timestamp)
// Lots of checks trigger this to ensure there are no joins left hanging
pub fn db_update_leave(app: &tauri::AppHandle, ts: &str, user_id: &str, emit: bool) -> Result<()> {
	if ts.is_empty() || user_id.is_empty() { return Ok(()); }
	db_init()?;
	let conn = rusqlite::Connection::open(db_path())?;
	let mut stmt = conn.prepare("SELECT id FROM join_log WHERE user_id = ? AND leave_timestamp IS NULL AND is_system = 0 ORDER BY join_timestamp DESC LIMIT 1")?;
	let mut rows = stmt.query(rusqlite::params![user_id])?;
	if let Some(row) = rows.next()? {
		let id: i64 = row.get(0)?;
		conn.execute("UPDATE join_log SET leave_timestamp = ? WHERE id = ?", rusqlite::params![ts, id])?;
		if emit {
			let payload = serde_json::json!({ "id": id, "userId": user_id, "leftAt": ts });
			app.emit("db_row_updated", payload)?;
		}
	} else {
		// No matching open row (duplicate leave, or join missed during backfill)
	}
	Ok(())
	// OK :DDDDDDD
}

// Insert a system event row (e.g., instance_changed) with optional context
// Planned updates will use context window
// Update: I forgot what I update I planned for that part. I'm sure it was a good idea.
pub fn db_insert_system_event(app: &tauri::AppHandle, ts: &str, event_kind: &str, message: Option<&str>, world_id: Option<&str>, instance_id: Option<&str>, region: Option<&str>, emit: bool) -> Result<()> {
	if ts.is_empty() { return Ok(()); }
	db_init()?;
	let conn = rusqlite::Connection::open(db_path())?;
	let mut stmt = conn.prepare("INSERT INTO join_log (user_id, username, join_timestamp, is_system, event_kind, message, world_id, instance_id, region) VALUES ('system', NULL, ?, 1, ?, ?, ?, ?, ?)")?;
	let _ = stmt.execute(rusqlite::params![ts, event_kind, message, world_id, instance_id, region])?;
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
	Ok(())
	// OK :DDDDD
}

// Mark all open (non-system) rows as left at the given timestamp
// This is used to close all open joins when an instance changes, or when certain conditions are met. (Close app, join/leave instance, vrchat close, etc)
pub fn db_purge_all(app: &tauri::AppHandle, ts: &str, emit: bool) -> Result<()> {
	if ts.is_empty() { return Ok(()); }
	db_init()?;
	let conn = rusqlite::Connection::open(db_path())?;
	conn.execute("UPDATE join_log SET leave_timestamp = ? WHERE leave_timestamp IS NULL AND is_system = 0", rusqlite::params![ts])?;
	if emit {
		app.emit("db_purged", serde_json::json!({ "ts": ts }))?;
	}
	Ok(())
}

// Return a page of rows ordered by newest join first (includes system rows)
// I tried really hard to better paginate this but I'm failing miserably
// I need to figure out how to pre-load the next page without lagging the front end, but also without using tiny pages.
// Maybe we can paginate by day and then chunk load data into the page? Not important, it works for now. If people want long-term logs, they can use VRCX. This is just for convenience, really.
#[tauri::command]
pub fn get_join_logs_page(offset: i64, limit: i64) -> Result<Vec<serde_json::Value>, String> {
	db_init().map_err(|e| e.to_string())?;
	let conn = rusqlite::Connection::open(db_path()).map_err(|e| e.to_string())?;
	let mut stmt = conn.prepare("SELECT id, user_id, username, join_timestamp, leave_timestamp, is_system, event_kind, message, world_id, instance_id, region FROM join_log ORDER BY join_timestamp DESC LIMIT ?2 OFFSET ?1").map_err(|e| e.to_string())?;
	let rows = stmt.query_map(rusqlite::params![offset, limit], |row| {
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
		}))
	}).map_err(|e| e.to_string())?;
	let mut out = Vec::new(); for r in rows { out.push(r.map_err(|e| e.to_string())?); }
	Ok(out)
	// OK :DD
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
	}.map_err(|e| e.to_string())?;

	// One line functions are pretty to look at, but I'm starting to think that I shouldn't use them.
	// I wonder what the standard is? Or if they help performance at all? Probably just compile time.
	// Maybe I'll look that up later.
	let mut out = Vec::new(); for r in rows { out.push(r.map_err(|e| e.to_string())?); }
	Ok(out)
	// OK :DDDD
}

// Lookup the latest known non-empty username for a given user_id from join_log
#[tauri::command]
pub fn get_latest_username_for_user(user_id: String) -> Result<serde_json::Value, String> {
    if user_id.trim().is_empty() { return Ok(serde_json::json!({ "username": serde_json::Value::Null })); }
    db_init().map_err(|e| e.to_string())?;
    let conn = rusqlite::Connection::open(db_path()).map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare("SELECT username FROM join_log WHERE user_id = ?1 AND username IS NOT NULL AND username <> '' ORDER BY join_timestamp DESC LIMIT 1").map_err(|e| e.to_string())?;
    let mut rows = stmt.query(rusqlite::params![user_id]).map_err(|e| e.to_string())?;
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
	conn.execute("DELETE FROM join_log", []).map_err(|e| e.to_string())?;
	Ok(())
}
