// Log Parser: Parses individual log lines and extracts events
// Emits events to the frontend via Tauri

use tauri::Emitter;
use regex::Regex;
use lazy_static::lazy_static;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, Duration};

// Get VRChat log directory (Windows: %LOCALAPPDATA%\..\LocalLow\VRChat\VRChat)
fn default_vrchat_log_dir() -> PathBuf {
    let local_low = std::env::var("LOCALAPPDATA")
        .ok()
        .and_then(|p| PathBuf::from(p).parent().map(|pp| pp.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("C:/Users/Public"));
    local_low.join("LocalLow").join("VRChat").join("VRChat")
}

// Regex pattern to match OnPlayerJoined/OnPlayerLeft events
// Format: OnPlayerJoined <username> (usr_<uuid>)
// Example: OnPlayerJoined Lamp? (usr_40f4043b-87e3-42c5-ba1f-ed0ad22e49e5)
lazy_static! {
    static ref PLAYER_JOIN_LEAVE_REGEX: Regex = Regex::new(
        r"OnPlayer(Joined|Left)\s+(.+?)\s+\(usr_([a-f0-9-]+)\)"
    ).expect("Failed to compile player join/leave regex");
    
    // Regex pattern to match ban/warn events
    // Format: Admin "admin_name" (banned|warned) player "target_name" for the following reason: "reason"
    // Example: Admin "IceTiger540" banned player "IceTiger540" for the following reason: "Hateful Behavior"
    // Example: Admin "- Lumine -" warned player "- Lumine -" for the following reason: "Harassing Behavior"
    static ref MODERATION_EVENT_REGEX: Regex = Regex::new(
        r#"Admin\s+"([^"]+)"\s+(banned|warned)\s+player\s+"([^"]+)"\s+for\s+the\s+following\s+reason:\s+"([^"]+)""#
    ).expect("Failed to compile moderation event regex");
    
    // Regex pattern to extract timestamp from log line
    // Format: YYYY.MM.DD HH:MM:SS
    // Handles two formats:
    // 1. Old: 2026.01.02 06:44:07 Debug...
    // 2. New: [2026-01-13 23:27:27] [output_log_2026-01-13_23-18-14.txt] 2026.01.13 23:27:26 Debug...
    // We want to capture the timestamp in YYYY.MM.DD HH:MM:SS format
    static ref TIMESTAMP_REGEX: Regex = Regex::new(
        r"(?:^|\]\s+)(\d{4}\.\d{2}\.\d{2}\s+\d{2}:\d{2}:\d{2})"
    ).expect("Failed to compile timestamp regex");

    // [Behaviour] Joining wrld_xxx:57420~private(...)~region(us) - extract world id and instance id (discard after first ~)
    // Must match wrld_ to avoid incorrectly matching "Joining or Creating Room: X"
    static ref JOINING_WORLD_REGEX: Regex = Regex::new(
        r"\[Behaviour\]\s+Joining\s+(wrld_[^:]+):([^~]+)"
    ).expect("Failed to compile joining world regex");

    // [Behaviour] Joining or Creating Room: Furry Hideout
    static ref JOINING_ROOM_REGEX: Regex = Regex::new(
        r"\[Behaviour\]\s+Joining\s+or\s+Creating\s+Room:\s*(.+)"
    ).expect("Failed to compile joining room regex");
}

/// In-memory location state (world id, instance id, room name) - latest only, overwritten by new discoveries
#[derive(Default)]
struct LocationState {
    world_id: Option<String>,
    instance_id: Option<String>,
    room_name: Option<String>,
    /// Timestamp when we joined this instance (from Joining line). Used to discard moderation events within 15s.
    instance_joined_timestamp: Option<String>,
}

/// Instance history entry (join/leave) - in-memory, cleared on restart
#[derive(Clone, serde::Serialize)]
struct InstanceHistoryEntry {
    timestamp: String,
    kind: String, // "join" | "leave"
    world_id: Option<String>,
    instance_id: Option<String>,
    room_name: Option<String>,
}

const INSTANCE_HISTORY_MAX: usize = 200;

lazy_static! {
    static ref LOCATION_STATE: Mutex<LocationState> = Mutex::new(LocationState::default());
    static ref INSTANCE_HISTORY: Mutex<Vec<InstanceHistoryEntry>> = Mutex::new(Vec::new());
}

fn extract_timestamp_from_line(line: &str) -> String {
    TIMESTAMP_REGEX
        .captures(line)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| chrono::Local::now().format("%Y.%m.%d %H:%M:%S").to_string())
}

fn push_instance_history_join(line: &str, world_id: Option<String>, instance_id: Option<String>, room_name: Option<String>) {
    if let Ok(mut hist) = INSTANCE_HISTORY.lock() {
        let timestamp = extract_timestamp_from_line(line);
        hist.push(InstanceHistoryEntry {
            timestamp,
            kind: "join".to_string(),
            world_id,
            instance_id,
            room_name,
        });
        let len = hist.len();
        if len > INSTANCE_HISTORY_MAX {
            hist.drain(0..(len - INSTANCE_HISTORY_MAX));
        }
    }
}

fn push_instance_history_leave(line: &str) {
    if let Ok(mut hist) = INSTANCE_HISTORY.lock() {
        let timestamp = extract_timestamp_from_line(line);
        hist.push(InstanceHistoryEntry {
            timestamp,
            kind: "leave".to_string(),
            world_id: None,
            instance_id: None,
            room_name: None,
        });
        let len = hist.len();
        if len > INSTANCE_HISTORY_MAX {
            hist.drain(0..(len - INSTANCE_HISTORY_MAX));
        }
    }
}

fn update_last_history_room(room_name: Option<String>) {
    if let (Ok(mut hist), Some(room)) = (INSTANCE_HISTORY.lock(), room_name) {
        if let Some(last) = hist.last_mut() {
            if last.kind == "join" && last.room_name.is_none() {
                last.room_name = Some(room);
            }
        }
    }
}

/// Get instance history (for stopwatch modal)
#[tauri::command]
pub fn get_instance_history() -> Result<Vec<serde_json::Value>, String> {
    if let Ok(hist) = INSTANCE_HISTORY.lock() {
        Ok(hist.iter().rev().map(|e| serde_json::json!({
            "timestamp": e.timestamp,
            "kind": e.kind,
            "world_id": e.world_id,
            "instance_id": e.instance_id,
            "room_name": e.room_name
        })).collect())
    } else {
        Err("Failed to get instance history".to_string())
    }
}

/// Parse player join/leave events from log lines
/// Returns true if a join/leave event was found and emitted
fn parse_player_join_leave(app_handle: &tauri::AppHandle, line: &str, file_name: &str) -> bool {
    if let Some(captures) = PLAYER_JOIN_LEAVE_REGEX.captures(line) {
        let event_type = captures.get(1).map(|m| m.as_str()).unwrap_or("");
        let username = captures.get(2).map(|m| m.as_str().trim()).unwrap_or("");
        let user_id = captures.get(3).map(|m| m.as_str()).unwrap_or("");
        
        // Construct full user ID with usr_ prefix
        let full_user_id = format!("usr_{}", user_id);
        
        // Determine event type
        let event_kind = if event_type == "Joined" {
            "player_joined"
        } else {
            "player_left"
        };
        
        // Emit structured event
        let _ = app_handle.emit("player_event", serde_json::json!({
            "file": file_name,
            "event": event_kind,
            "username": username,
            "user_id": full_user_id,
            "timestamp": chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            "raw_line": line
        }));
        
        return true;
    }
    
    false
}

/// Minimum seconds in instance before recording moderation events (discard earlier)
const MOD_LOG_MIN_SECONDS_IN_INSTANCE: i64 = 30;

/// Parse ban/warn events from log lines
/// Returns true if a moderation event was found and stored
fn parse_ban_event(app_handle: &tauri::AppHandle, line: &str, _file_name: &str) -> bool {
    if let Some(captures) = MODERATION_EVENT_REGEX.captures(line) {
        let admin = captures.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
        let action_type = captures.get(2).map(|m| m.as_str()).unwrap_or("").to_string(); // "banned" or "warned"
        let target = captures.get(3).map(|m| m.as_str()).unwrap_or("").to_string();
        let reason = captures.get(4).map(|m| m.as_str()).unwrap_or("").to_string();
        
        // Normalize action type: "banned" -> "ban", "warned" -> "warn"
        let action_normalized = if action_type == "warned" {
            "warn".to_string()
        } else {
            "ban".to_string()
        };
        
        // Extract timestamp from log line (format: YYYY.MM.DD HH:MM:SS)
        // Looks for timestamp after the closing bracket of the filename
        let timestamp = TIMESTAMP_REGEX
            .captures(line)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| chrono::Local::now().format("%Y.%m.%d %H:%M:%S").to_string());
        
        // Discard events within 15 seconds of joining the instance (avoids carryover/stale events)
        if let Ok(state) = LOCATION_STATE.lock() {
            if let Some(ref join_ts) = state.instance_joined_timestamp {
                let join_dt = chrono::NaiveDateTime::parse_from_str(join_ts, "%Y.%m.%d %H:%M:%S");
                let ban_dt = chrono::NaiveDateTime::parse_from_str(&timestamp, "%Y.%m.%d %H:%M:%S");
                if let (Ok(join), Ok(ban)) = (join_dt, ban_dt) {
                    let elapsed = ban.signed_duration_since(join);
                    if elapsed >= chrono::Duration::zero()
                        && elapsed < chrono::Duration::seconds(MOD_LOG_MIN_SECONDS_IN_INSTANCE)
                    {
                        crate::debug_println!(
                            "[MOD_LOG] Discarding {} event (within {}s of instance join)",
                            action_normalized,
                            MOD_LOG_MIN_SECONDS_IN_INSTANCE
                        );
                        return false; // Don't record or emit
                    }
                }
            }
        }
        
        // Store moderation log entry in database with extracted timestamp
        let db_start = std::time::Instant::now();
        let location = get_current_location_for_mod_log();
        if let Err(e) = crate::modules::world_mod::world_mod_logs::add_ban_log(
            admin.clone(),
            target.clone(),
            reason.clone(),
            timestamp.clone(),
            action_normalized.clone(),
            location,
        ) {
            crate::debug_eprintln!("Failed to store moderation log: {}", e);
        }
        let db_duration = db_start.elapsed();
        crate::debug_println!("[PERF] parse_ban_event DB store: {:.2}ms", db_duration.as_secs_f64() * 1000.0);
        
        // Emit event to frontend for real-time updates
        let emit_start = std::time::Instant::now();
        let _ = app_handle.emit("ban_event", serde_json::json!({
            "admin": admin,
            "target": target,
            "reason": reason,
            "timestamp": timestamp,
            "action_type": action_normalized,
            "raw_line": line
        }));
        let emit_duration = emit_start.elapsed();
        crate::debug_println!("[PERF] parse_ban_event emit: {:.2}ms", emit_duration.as_secs_f64() * 1000.0);
        
        return true;
    }
    
    false
}

/// Clear location state (e.g. when instance is cleared)
fn clear_location_state() {
    if let Ok(mut state) = LOCATION_STATE.lock() {
        *state = LocationState::default();
    }
}

/// Get current location as "world_id:instance_id" for enrichment when recording moderation events.
/// Returns "N/A" if no location or on error.
pub fn get_current_location_for_mod_log() -> String {
    if let Ok(state) = LOCATION_STATE.lock() {
        match (&state.world_id, &state.instance_id) {
            (Some(w), Some(i)) => format!("{}:{}", w, i),
            (Some(w), None) => format!("{}:N/A", w),
            (None, Some(i)) => format!("N/A:{}", i),
            (None, None) => "N/A".to_string(),
        }
    } else {
        "N/A".to_string()
    }
}

/// Get current location (for frontend to request when tab becomes visible)
#[tauri::command]
pub fn get_current_location() -> Result<serde_json::Value, String> {
    if let Ok(state) = LOCATION_STATE.lock() {
        Ok(serde_json::json!({
            "world_id": state.world_id,
            "instance_id": state.instance_id,
            "room_name": state.room_name
        }))
    } else {
        Err("Failed to get location state".to_string())
    }
}

/// Parse [Behaviour] Joining world:instance and [Behaviour] Joining or Creating Room lines.
/// Updates in-memory state (latest only). If emit is true, emits location_update event.
fn parse_location_update(app_handle: &tauri::AppHandle, line: &str, emit: bool) -> bool {
    let mut updated = false;

    // [Behaviour] Joining wrld_xxx:57420~...
    if let Some(captures) = JOINING_WORLD_REGEX.captures(line) {
        let world_id = captures.get(1).map(|m| m.as_str().trim().to_string()).filter(|s| !s.is_empty());
        let instance_id = captures.get(2).map(|m| m.as_str().trim().to_string()).filter(|s| !s.is_empty());

        if world_id.is_some() || instance_id.is_some() {
            push_instance_history_join(line, world_id.clone(), instance_id.clone(), None);
            if let Ok(mut state) = LOCATION_STATE.lock() {
                if world_id.is_some() {
                    state.world_id = world_id;
                }
                if instance_id.is_some() {
                    state.instance_id = instance_id;
                }
                state.instance_joined_timestamp = Some(extract_timestamp_from_line(line));
                updated = true;
            }
        }
    }

    // [Behaviour] Joining or Creating Room: Room Name
    if let Some(captures) = JOINING_ROOM_REGEX.captures(line) {
        let room_name = captures.get(1).map(|m| m.as_str().trim().to_string()).filter(|s| !s.is_empty());

        if room_name.is_some() {
            update_last_history_room(room_name.clone());
            if let Ok(mut state) = LOCATION_STATE.lock() {
                state.room_name = room_name;
                updated = true;
            }
        }
    }

    if updated && emit {
        if let Ok(state) = LOCATION_STATE.lock() {
            let _ = app_handle.emit("location_update", serde_json::json!({
                "world_id": state.world_id,
                "instance_id": state.instance_id,
                "room_name": state.room_name
            }));
        }
    }

    updated
}

pub fn emit_log_line(app_handle: &tauri::AppHandle, line: &str, file_name: &str) {
    // Check for "[Behaviour] Successfully joined room" or "[Behaviour] OnLeftRoom"
    // These indicate a new instance session or leaving the instance
    if line.contains("[Behaviour] Successfully joined room") || 
       line.contains("[Behaviour] OnLeftRoom") {
        // Only clear location when leaving - "Successfully joined room" comes AFTER Joining lines,
        // so clearing here would wipe the location we just parsed for the new instance
        let left = line.contains("[Behaviour] OnLeftRoom");
        if left {
            push_instance_history_leave(line);
            clear_location_state();
            let _ = app_handle.emit("location_update", serde_json::json!({
                "world_id": null,
                "instance_id": null,
                "room_name": null
            }));
        }
        // Emit event to clear instance monitor (clears player list)
        // left: true when OnLeftRoom so frontend can clear location/timer; false when Successfully joined room
        let _ = app_handle.emit("instance_cleared", serde_json::json!({
            "file": file_name,
            "timestamp": chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            "left": left
        }));
    }

    // Parse location lines ([Behaviour] Joining world:instance, Joining or Creating Room)
    parse_location_update(app_handle, line, true);
    
    // Check for ban events
    parse_ban_event(app_handle, line, file_name);
    
    // Check for player join/leave events
    parse_player_join_leave(app_handle, line, file_name);
    
    // Always emit the raw log line event to frontend
    let _ = app_handle.emit("log_line", serde_json::json!({
        "file": file_name,
        "line": line,
        "timestamp": chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
    }));
}

/// Find the log file with the most recent modification time
fn find_most_recently_modified_log_file(log_dir: &PathBuf) -> Result<Option<PathBuf>, String> {
    let entries = std::fs::read_dir(log_dir)
        .map_err(|e| format!("Failed to read log directory: {}", e))?;
    
    let mut most_recent_file: Option<(PathBuf, SystemTime)> = None;
    
    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        let path = entry.path();
        
        // Check if it's a log file
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if !name.starts_with("output_log_") || !name.ends_with(".txt") {
                continue;
            }
        } else {
            continue;
        }
        
        // Get modification time
        if let Ok(metadata) = entry.metadata() {
            if let Ok(modified_time) = metadata.modified() {
                match most_recent_file {
                    None => {
                        most_recent_file = Some((path, modified_time));
                    }
                    Some((_, current_time)) => {
                        if modified_time > current_time {
                            most_recent_file = Some((path, modified_time));
                        }
                    }
                }
            }
        }
    }
    
    Ok(most_recent_file.map(|(path, _)| path))
}

// Event cache for retroactive scan
#[derive(Debug, Clone)]
struct CachedPlayerEvent {
    event_type: String, // "player_joined" or "player_left"
    username: String,
    user_id: String,
    timestamp: String, // Extracted from log line
    raw_line: String,
}

/// Retroactive scan: Scan the latest log file from bottom up for join/leave events
/// Scans from bottom until "[Behaviour] Successfully joined room" marker or top of file
/// Only processes if file was modified within last 10 minutes
/// Caches all events and emits them in chronological order (oldest first)
#[tauri::command]
pub fn manual_refresh_scan(app_handle: tauri::AppHandle) -> Result<String, String> {
    let start_time = std::time::Instant::now();
    crate::debug_println!("[PERF] manual_refresh_scan START");
    
    // Use the default VRChat log directory
    let log_dir = default_vrchat_log_dir();
    
    // Find the most recently modified log file
    let find_start = std::time::Instant::now();
    let log_file_path = match find_most_recently_modified_log_file(&log_dir)? {
        Some(path) => path,
        None => return Err("No log files found".to_string()),
    };
    let find_duration = find_start.elapsed();
    crate::debug_println!("[PERF] manual_refresh_scan find_most_recently_modified_log_file: {:.2}ms", find_duration.as_secs_f64() * 1000.0);
    
    // Check if file exists
    if !log_file_path.exists() {
        return Err(format!("Log file does not exist: {}", log_file_path.display()));
    }
    
    // Check if file was modified within last 10 minutes
    let metadata_start = std::time::Instant::now();
    let metadata = std::fs::metadata(&log_file_path)
        .map_err(|e| format!("Failed to get file metadata: {}", e))?;
    
    let modified_time = metadata.modified()
        .map_err(|e| format!("Failed to get modified time: {}", e))?;
    
    let now = SystemTime::now();
    let ten_minutes = Duration::from_secs(10 * 60);
    
    if let Ok(elapsed) = now.duration_since(modified_time) {
        if elapsed > ten_minutes {
            return Err(format!(
                "Log file is too old (modified {} minutes ago, max 10 minutes)",
                elapsed.as_secs() / 60
            ));
        }
    } else {
        return Err("Failed to calculate file age".to_string());
    }
    let metadata_duration = metadata_start.elapsed();
    crate::debug_println!("[PERF] manual_refresh_scan metadata check: {:.2}ms", metadata_duration.as_secs_f64() * 1000.0);
    
    // Get file name for event emission
    let file_name = log_file_path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();
    
    // Read file from bottom up until "[Behaviour] Joining wrld_xxx:instance" (earliest of the three; always occurs first)
    let open_start = std::time::Instant::now();
    let mut file = File::open(&log_file_path)
        .map_err(|e| format!("Failed to open log file: {}", e))?;
    let open_duration = open_start.elapsed();
    crate::debug_println!("[PERF] manual_refresh_scan file open: {:.2}ms", open_duration.as_secs_f64() * 1000.0);
    
    let file_size = metadata.len();
    if file_size == 0 {
        return Ok("Log file is empty".to_string());
    }
    crate::debug_println!("[PERF] manual_refresh_scan file size: {} bytes", file_size);
    
    // Read file from bottom up in chunks
    const CHUNK_SIZE: u64 = 8192; // 8KB chunks
    let mut buffer = Vec::new();
    let mut position = file_size;
    let mut found_joining = false;
    
    // Read backwards in chunks
    while position > 0 && !found_joining {
        let chunk_start = if position > CHUNK_SIZE {
            position - CHUNK_SIZE
        } else {
            0
        };
        
        let chunk_size = (position - chunk_start) as usize;
        
        file.seek(SeekFrom::Start(chunk_start))
            .map_err(|e| format!("Failed to seek in file: {}", e))?;
        
        let mut chunk = vec![0u8; chunk_size];
        let bytes_read = file.read(&mut chunk)
            .map_err(|e| format!("Failed to read chunk: {}", e))?;
        
        if bytes_read == 0 {
            break;
        }
        
        // Prepend chunk to buffer (only the bytes we actually read)
        buffer.splice(0..0, chunk[..bytes_read].iter().cloned());
        
        // Check for "[Behaviour] Joining wrld_xxx:instance" in the buffer (earliest of the three; occurs first)
        let buffer_str = match String::from_utf8(buffer.clone()) {
            Ok(s) => s,
            Err(_) => {
                // If UTF-8 conversion fails, try with lossy conversion
                String::from_utf8_lossy(&buffer).to_string()
            }
        };
        
        if buffer_str.contains("Joining wrld_") {
            found_joining = true;

            // Find the last occurrence and keep from the start of that line (includes Joining, Joining or Creating Room, Successfully joined room, player events)
            if let Some(last_pos) = buffer_str.rfind("Joining wrld_") {
                // Find the start of the line containing the marker (previous newline or start of buffer)
                let line_start = buffer_str[..last_pos].rfind('\n')
                    .map(|i| i + 1)
                    .unwrap_or(0);
                let remaining_str = &buffer_str[line_start..];
                buffer = remaining_str.as_bytes().to_vec();
            }

            // Emit event to clear instance monitor before we push new data
            let _ = app_handle.emit("instance_cleared", serde_json::json!({
                "file": file_name,
                "timestamp": chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
            }));
        }
        
        position = chunk_start;
    }
    
    // If we didn't find the marker, process the entire file
    // Parse all lines in buffer (from bottom to top)
    let parse_start = std::time::Instant::now();
    let content = String::from_utf8_lossy(&buffer);
    let all_lines: Vec<&str> = content.lines().collect();
    let parse_duration = parse_start.elapsed();
    crate::debug_println!("[PERF] manual_refresh_scan parsed {} lines: {:.2}ms", all_lines.len(), parse_duration.as_secs_f64() * 1000.0);

    // Parse location from lines (forward order so last match wins - most recent state)
    clear_location_state();
    for line in all_lines.iter() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            parse_location_update(&app_handle, trimmed, false);
        }
    }
    // Emit location_update once with final state
    if let Ok(state) = LOCATION_STATE.lock() {
        let _ = app_handle.emit("location_update", serde_json::json!({
            "world_id": state.world_id,
            "instance_id": state.instance_id,
            "room_name": state.room_name
        }));
    }

    // Cache all player events during scan (don't emit yet)
    let mut cached_events: Vec<CachedPlayerEvent> = Vec::new();
    let mut join_count = 0;
    let mut leave_count = 0;
    
    // Process lines in reverse order (from newest to oldest) to collect events
    let process_start = std::time::Instant::now();
    for line in all_lines.iter().rev() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        
        // Parse the line for ban events (these can be emitted immediately)
        parse_ban_event(&app_handle, trimmed, &file_name);
        
        // Parse the line for join/leave events and cache them
        if let Some(captures) = PLAYER_JOIN_LEAVE_REGEX.captures(trimmed) {
            let event_type = captures.get(1).map(|m| m.as_str()).unwrap_or("");
            let username = captures.get(2).map(|m| m.as_str().trim()).unwrap_or("");
            let user_id = captures.get(3).map(|m| m.as_str()).unwrap_or("");
            
            // Construct full user ID with usr_ prefix
            let full_user_id = format!("usr_{}", user_id);
            
            // Determine event type
            let event_kind = if event_type == "Joined" {
                "player_joined"
            } else {
                "player_left"
            };
            
            // Extract timestamp from log line
            let timestamp = TIMESTAMP_REGEX
                .captures(trimmed)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string())
                .unwrap_or_else(|| chrono::Local::now().format("%Y.%m.%d %H:%M:%S").to_string());
            
            // Cache the event
            cached_events.push(CachedPlayerEvent {
                event_type: event_kind.to_string(),
                username: username.to_string(),
                user_id: full_user_id,
                timestamp: timestamp.clone(),
                raw_line: trimmed.to_string(),
            });
            
            if event_kind == "player_joined" {
                join_count += 1;
            } else {
                leave_count += 1;
            }
        }
    }
    let process_duration = process_start.elapsed();
    crate::debug_println!("[PERF] manual_refresh_scan process lines: {:.2}ms ({} joins, {} leaves)", process_duration.as_secs_f64() * 1000.0, join_count, leave_count);
    
    // Sort cached events by timestamp (oldest first) so joins happen before leaves
    let sort_start = std::time::Instant::now();
    cached_events.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    let sort_duration = sort_start.elapsed();
    crate::debug_println!("[PERF] manual_refresh_scan sort events: {:.2}ms", sort_duration.as_secs_f64() * 1000.0);
    
    // Capture event count before moving cached_events
    let event_count = cached_events.len();
    
    // Now emit all events in chronological order
    let emit_start = std::time::Instant::now();
    for event in cached_events {
        let _ = app_handle.emit("player_event", serde_json::json!({
            "file": file_name,
            "event": event.event_type,
            "username": event.username,
            "user_id": event.user_id,
            "timestamp": chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            "raw_line": event.raw_line
        }));
    }
    let emit_duration = emit_start.elapsed();
    crate::debug_println!("[PERF] manual_refresh_scan emit events: {:.2}ms ({} events)", emit_duration.as_secs_f64() * 1000.0, event_count);
    
    let total_duration = start_time.elapsed();
    crate::debug_println!("[PERF] manual_refresh_scan END: {:.2}ms", total_duration.as_secs_f64() * 1000.0);
    
    Ok(format!(
        "Scanned log file: found {} joins and {} leaves",
        join_count, leave_count
    ))
}
