// Local Database: Notes & Watchlist storage
//
// This module persists small user-centric metadata (notes, last-known usernames,
// and a boolean watchlist) in a single JSON file under the app's data folder.
//
// Design goals:
// - Keep it human-readable/editable (JSON on disk).
// - Avoid complex schemas for this lightweight data.
// - Provide simple Tauri commands for the front-end to read/write.

use std::{fs, path::PathBuf};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct UserNotes {
    // Map of userId -> array of notes. We currently keep only the latest note
    // in the array (to preserve timestamp), but store as Vec for future growth.
    #[serde(default)]
    pub notes: std::collections::BTreeMap<String, Vec<Note>>, // userId -> notes
    // Map of userId -> watch flag. True means show special UI/notification.
    #[serde(default)]
    pub watchlist: std::collections::BTreeMap<String, bool>, // userId -> watch boolean
    // Map of userId -> last known username (for nicer display in lists).
    #[serde(default)]
    pub usernames: std::collections::BTreeMap<String, String>, // userId -> last known username
    // Map of userId -> optional custom sound path override.
    #[serde(default)]
    pub sounds: std::collections::BTreeMap<String, String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Note {
    // Human-readable timestamp (YYYY.MM.DD HH:MM:SS), to align with logs.
    pub ts: String,
    // The user-authored note body.
    pub text: String,
}

/// Get the directory where notes are stored
pub fn notes_dir() -> PathBuf {
    // Choose a stable per-user folder (LocalAppData\FCHClient on Windows).
    // This is shared by dev and release unless you differentiate elsewhere.
    let base = std::env::var("LOCALAPPDATA")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:/Users/Public"));
    base.join("FCHClient")
}

/// Get the path to the notes.json file
fn notes_path() -> PathBuf {
    // Single JSON file that contains all notes/watchlist/usernames
    notes_dir().join("notes.json")
}

/// Load all notes from disk (best-effort: returns empty structure on failure)
pub fn load_all_notes() -> UserNotes {
    let start_time = std::time::Instant::now();
    let p = notes_path();
    let read_start = std::time::Instant::now();
    if let Ok(data) = fs::read(&p) {
        let read_duration = read_start.elapsed();
        let parse_start = std::time::Instant::now();
        if let Ok(n) = serde_json::from_slice::<UserNotes>(&data) {
            let parse_duration = parse_start.elapsed();
            let total_duration = start_time.elapsed();
            crate::debug_println!("[PERF] load_all_notes: {:.2}ms (read: {:.2}ms, parse: {:.2}ms, {} users)", 
                total_duration.as_secs_f64() * 1000.0,
                read_duration.as_secs_f64() * 1000.0,
                parse_duration.as_secs_f64() * 1000.0,
                n.notes.len());
            return n;
        }
        let parse_duration = parse_start.elapsed();
        crate::debug_println!("[PERF] load_all_notes parse failed: {:.2}ms", parse_duration.as_secs_f64() * 1000.0);
    } else {
        let read_duration = read_start.elapsed();
        crate::debug_println!("[PERF] load_all_notes read failed: {:.2}ms", read_duration.as_secs_f64() * 1000.0);
    }
    UserNotes::default()
}

/// Save all notes to disk
pub fn save_all_notes(notes: &UserNotes) -> Result<(), String> {
    // Ensure the folder exists, then write a pretty JSON snapshot.
    let dir = notes_dir();
    if let Err(e) = fs::create_dir_all(&dir) {
        return Err(e.to_string());
    }
    let p = notes_path();
    let data = serde_json::to_vec_pretty(notes).map_err(|e| e.to_string())?;
    fs::write(p, data).map_err(|e| e.to_string())
}

/// Initialize the notes database - creates file if it doesn't exist
pub fn init_notes_db() -> Result<(), String> {
    let p = notes_path();
    
    // If file doesn't exist, create it with default empty structure
    if !p.exists() {
        let default_notes = UserNotes::default();
        save_all_notes(&default_notes)?;
    }
    
    Ok(())
}

#[tauri::command]
pub fn add_note(user_id: String, text: String) -> Result<(), String> {
    // Validate input and capture a human-readable timestamp.
    if user_id.trim().is_empty() {
        return Err("user_id required".into());
    }
    let ts = chrono::Local::now().format("%Y.%m.%d %H:%M:%S").to_string();
    let mut all = load_all_notes();
    // We currently store only the latest note; keeping a Vec preserves the timestamp
    // structure and makes it easy to extend to multiple notes later.
    let entry = all.notes.entry(user_id).or_default();
    entry.clear();
    entry.push(Note { ts, text });
    save_all_notes(&all)
}

// Note: get_notes is kept for potential future use but not currently exposed as a Tauri command
// The frontend uses get_all_notes() instead for bulk loading
#[allow(dead_code)]
pub fn get_notes(user_id: String) -> Result<serde_json::Value, String> {
    // Return an array of notes for a user (empty if none). Front-end can choose
    // to display only the latest.
    let all = load_all_notes();
    let items = all.notes.get(&user_id).cloned().unwrap_or_default();
    Ok(serde_json::json!({ "userId": user_id, "notes": items }))
}

#[tauri::command]
pub fn get_all_notes() -> Result<serde_json::Value, String> {
    // Convenient bulk read used by the database page to hydrate its list.
    let start_time = std::time::Instant::now();
    crate::debug_println!("[PERF] get_all_notes START");
    let all = load_all_notes();
    let serialize_start = std::time::Instant::now();
    let result = serde_json::json!({
        "notes": all.notes,
        "usernames": all.usernames,
        "watchlist": all.watchlist,
        "sounds": all.sounds,
    });
    let serialize_duration = serialize_start.elapsed();
    let total_duration = start_time.elapsed();
    crate::debug_println!("[PERF] get_all_notes END: {:.2}ms (serialize: {:.2}ms)", 
        total_duration.as_secs_f64() * 1000.0,
        serialize_duration.as_secs_f64() * 1000.0);
    Ok(result)
}

#[tauri::command]
pub fn delete_user(user_id: String) -> Result<(), String> {
    // Remove all traces of a user from notes, watchlist, and username cache.
    if user_id.trim().is_empty() {
        return Err("user_id required".into());
    }
    let mut all = load_all_notes();
    all.notes.remove(&user_id);
    all.watchlist.remove(&user_id);
    all.usernames.remove(&user_id);
    all.sounds.remove(&user_id);
    save_all_notes(&all)
}

#[tauri::command]
pub fn get_note(user_id: String) -> Result<serde_json::Value, String> {
    // Convenience API that returns only the latest note's text for quick editing.
    let start_time = std::time::Instant::now();
    let all = load_all_notes();
    let text = all
        .notes
        .get(&user_id)
        .and_then(|v| v.last())
        .map(|n| n.text.clone())
        .unwrap_or_default();
    let duration = start_time.elapsed();
    crate::debug_println!("[PERF] get_note({}): {:.2}ms", user_id, duration.as_secs_f64() * 1000.0);
    Ok(serde_json::json!({ "userId": user_id, "text": text }))
}

#[tauri::command]
pub fn set_watch(user_id: String, watch: bool) -> Result<(), String> {
    // Toggle a user's watch flag (front-end can visually indicate this state).
    if user_id.trim().is_empty() {
        return Err("user_id required".into());
    }
    let mut all = load_all_notes();
    all.watchlist.insert(user_id, watch);
    save_all_notes(&all)
}

#[tauri::command]
pub fn get_watch(user_id: String) -> Result<serde_json::Value, String> {
    // Read a user's watch flag (defaults to false when not present).
    let start_time = std::time::Instant::now();
    let all = load_all_notes();
    let watch = all.watchlist.get(&user_id).copied().unwrap_or(false);
    let duration = start_time.elapsed();
    crate::debug_println!("[PERF] get_watch({}): {:.2}ms", user_id, duration.as_secs_f64() * 1000.0);
    Ok(serde_json::json!({ "userId": user_id, "watch": watch }))
}

#[tauri::command]
pub fn set_user_sound(user_id: String, path: Option<String>) -> Result<(), String> {
    if user_id.trim().is_empty() {
        return Err("user_id required".into());
    }
    let mut all = load_all_notes();
    match path.and_then(|p| if p.trim().is_empty() { None } else { Some(p) }) {
        Some(p) => {
            all.sounds.insert(user_id, p);
        }
        None => {
            all.sounds.remove(&user_id);
        }
    }
    save_all_notes(&all)
}

#[tauri::command]
pub fn get_user_sound(user_id: String) -> Result<serde_json::Value, String> {
    let all = load_all_notes();
    let path = all.sounds.get(&user_id).cloned().unwrap_or_default();
    Ok(
        serde_json::json!({ "userId": user_id, "soundPath": if path.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(path) } }),
    )
}

#[tauri::command]
pub fn set_username(user_id: String, username: String) -> Result<(), String> {
    if user_id.trim().is_empty() {
        return Err("user_id required".into());
    }
    let mut all = load_all_notes();
    let effective = if username.trim().is_empty() {
        "Not Yet Recorded".to_string()
    } else {
        username
    };
    all.usernames.insert(user_id, effective);
    save_all_notes(&all)
}

#[tauri::command]
pub fn browse_sound() -> Result<serde_json::Value, String> {
    let file = rfd::FileDialog::new()
        .add_filter("Audio", &["wav", "mp3", "ogg", "flac", "m4a"])
        .pick_file();
    Ok(serde_json::json!({ "path": file.map(|p| p.to_string_lossy().to_string()) }))
}
