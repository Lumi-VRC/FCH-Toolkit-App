// Watcher: tails the VRChat log file, parses lines incrementally, and streams
// events to the UI while maintaining a SQLite mirror of joins/leaves and
// recording system events (like instance changes).

use crate::debug::emit_debug;
use anyhow::Result;
use chrono::Local;
use regex::Regex;
use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::Emitter; // brings .emit() for sending events to the front-end

// Debug-only logging helper. To troubleshoot tailing, replace the body with
// eprintln!($($t)*); to see messages at runtime. Left empty to avoid spam.
// Debug logging in this language is soooooooo fucking gay.
macro_rules! debug_log {
    ($($t:tt)*) => {{}};
}

// Start the background log watcher. This is invoked by the front-end when the
// Instance Monitor screen is opened. We no longer purge previously "open"
// rows on startup; backfill from the latest instance join reconstructs state.
#[tauri::command]
pub async fn start_log_watcher(app_handle: tauri::AppHandle) -> Result<(), String> {
    debug_log!("[watcher] start requested");
    // Run the tail loop on a background task so the Tauri thread remains free.
    tokio::spawn(async move {
        debug_log!("[watcher] task spawned");
        if let Err(e) = log_watch_loop(app_handle).await {
            eprintln!("log watcher stopped: {e:?}");
        }
    });
    Ok(())
}

// Provide basic information about the current log file (path + size) so the
// front-end log explorer can render metadata.
#[tauri::command]
pub fn read_log_info() -> Result<serde_json::Value, String> {
    let dir = default_vrchat_log_dir();
    if let Some(p) = find_latest_log_file(&dir) {
        let md = fs::metadata(&p).map_err(|e| e.to_string())?;
        Ok(serde_json::json!({
            "path": p.to_string_lossy(),
            "size": md.len()
        }))
    } else {
        Ok(serde_json::json!({ "path": serde_json::Value::Null, "size": 0 }))
    }
}

// Collect deduplicated substrings captured from lines containing
// "User Authenticated:" across all available VRChat log files.
// For each matching line, we capture everything after "Authenticated:" to EOL.
// Example line:
//   2025.09.18 15:16:07 Debug      -  User Authenticated: - Lumine - (usr_...)
// Captured substring:
//   " - Lumine - (usr_...)"
#[tauri::command]
pub fn get_tool_authentication_lines() -> Result<Vec<String>, String> {
    let dir = default_vrchat_log_dir();
    let mut set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return Ok(Vec::new()),
    };
    for ent in entries.flatten() {
        let p = ent.path();
        let name = match p.file_name().and_then(|n| n.to_str()) {
            Some(s) => s,
            None => continue,
        };
        if !(name.starts_with("output_log_") && name.ends_with(".txt")) {
            continue;
        }
        if let Ok(content) = fs::read_to_string(&p) {
            for raw in content.split('\n') {
                let line = raw.trim_end_matches('\r');
                if let Some(idx) = line.find("User Authenticated:") {
                    // Find the colon and capture the remainder after it
                    if let Some(colon_idx) = line[idx..].find(":") {
                        let start = idx + colon_idx + 1; // position after ':'
                        let fragment = &line[start..];
                        // Keep leading spaces to preserve raw appearance; trim trailing
                        let captured = fragment.trim_end();
                        if !captured.is_empty() {
                            set.insert(captured.to_string());
                        }
                    }
                }
            }
        }
    }
    Ok(set.into_iter().collect())
}

// Stateless chunked reader used by the Log Explorer page. The UI asks for
// bytes starting at "offset" up to "max_bytes" and we return the data along
// with a new offset and EOF flag.
#[tauri::command]
pub fn read_log_chunk(offset: u64, max_bytes: u32) -> Result<serde_json::Value, String> {
    let dir = default_vrchat_log_dir();
    let path = find_latest_log_file(&dir).ok_or_else(|| "No log file found".to_string())?;
    let mut f = fs::File::open(&path).map_err(|e| e.to_string())?;
    let size = f.metadata().map_err(|e| e.to_string())?.len();
    // Clamp the start offset within the file
    let start = offset.min(size);
    f.seek(SeekFrom::Start(start)).map_err(|e| e.to_string())?;
    // Calculate the exact number of bytes to read without crossing EOF
    let to_read = (max_bytes as u64).min(size.saturating_sub(start)) as usize;
    let mut buf = vec![0u8; to_read];
    let n = f.read(&mut buf).map_err(|e| e.to_string())?;
    let data = String::from_utf8_lossy(&buf[..n]).to_string();
    let new_offset = start + n as u64;
    Ok(serde_json::json!({
        "data": data,
        "offset": new_offset,
        "eof": new_offset >= size
    }))
}

// Shared cancellation token for async search. The i32 holds the "current"
// token. When a newer token is stored, older searches cancel themselves.
pub struct SearchState(pub Arc<Mutex<i32>>);

// Perform a simple case-insensitive substring search across the current log
// and return line indices containing matches. Emits progress events and a
// cancel event if superseded by a newer token.
#[tauri::command]
pub async fn search_log_file(
    app_handle: tauri::AppHandle,
    query: String,
    search_token: i32,
    state: tauri::State<'_, SearchState>,
) -> Result<Vec<usize>, String> {
    // Store my token; if another search starts later, that token supersedes mine
    {
        let mut current_token = state.0.lock().unwrap();
        let previous = *current_token;
        if previous != 0 && previous != search_token {
            let _ = app_handle.emit("cancel_search", serde_json::json!({ "token": previous }));
        }
        *current_token = search_token;
    }

    // Load the whole file (good enough for typical VRChat logs on desktop)
    let dir = default_vrchat_log_dir();
    let path = find_latest_log_file(&dir).ok_or_else(|| "No log file found".to_string())?;
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let lines: Vec<&str> = content.split('\n').collect();
    let total = lines.len();
    let needle = query.to_lowercase();
    let mut matches = Vec::new();

    // Iterate by chunks to yield to the runtime and report progress periodically
    let batch_size = 1000;
    for (i, chunk) in lines.chunks(batch_size).enumerate() {
        // Let other tasks run
        tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
        // Check cancellation: a newer token means this search should stop
        {
            let current_token = state.0.lock().unwrap();
            if *current_token != search_token {
                return Err("Search cancelled".to_string());
            }
        }
        // Collect indices of lines that contain the lowercase needle
        for (j, line) in chunk.iter().enumerate() {
            if line.to_lowercase().contains(&needle) {
                matches.push(i * batch_size + j);
            }
        }
        // Progress is approximate but sufficient for a UI progress bar
        let progress = ((i * batch_size) as f32 / total as f32 * 100.0).round() as i32;
        app_handle
            .emit(
                "search_progress",
                serde_json::json!({ "progress": progress, "token": search_token }),
            )
            .unwrap();
    }
    Ok(matches)
}

// Tail the latest VRChat log and emit/record events as they arrive.
async fn log_watch_loop(app: tauri::AppHandle) -> Result<()> {
    let vrchat_dir = default_vrchat_log_dir();
    debug_log!("[watcher] started; dir={}", vrchat_dir.display());
    // Regexes for timestamp, joins, leaves, purges, and instance changes
    let re_ts = Regex::new(r"^(\d{4}\.\d{2}\.\d{2}\s+\d{2}:\d{2}:\d{2})").unwrap();
    let re_join =
        Regex::new(r"OnPlayerJoined\s+(?:\[[^\]]+\]\s*)?([^\r\n(]+?)\s*\((usr_[a-f0-9\-]{36})\)")
            .unwrap();
    let re_left = Regex::new(r"OnPlayerLeft\s+([^\r\n(]+?)\s*\((usr_[a-f0-9\-]{36})\)").unwrap();
    let re_purge1 = Regex::new(r"Successfully left room").unwrap();
    let re_purge2 = Regex::new(r"VRCNP: Stopping server").unwrap();
    let re_purge3 = Regex::new(r"Successfully joined room").unwrap();
    let re_quit = Regex::new(r"VRCApplication:\s*HandleApplicationQuit").unwrap();
    let re_destroying = Regex::new(r"Destroying\s+([^\r\n]+)").unwrap();
    let re_joining =
        Regex::new(r"Joining\s+(wrld_[a-f0-9\-]{36}):([^~\s]+)(?:~region\(([^)]+)\))?").unwrap();
    let re_analysis_path = Regex::new(r"/analysis/(file_[a-z0-9\-]+)/([0-9]+)/security").unwrap();
    let re_prints_path = Regex::new(r"prints/(prnt_[a-z0-9\-]+)").unwrap();
    let re_inventory_path =
        Regex::new(r"user/(usr_[a-z0-9\-]+)/inventory/(inv_[a-z0-9\-]+)").unwrap();
    let re_switch_avatar =
        Regex::new(r"\[Behaviour\]\s+Switching\s+(.+?)\s+to\s+avatar\s+(.+)").unwrap();

    let api_client = api_checks::Client::new(app.clone());

    // Current file and tailing state
    let mut current_path: Option<PathBuf> = None;
    let mut file: Option<fs::File> = None;
    let mut last_offset: u64 = 0; // where we've read up to in the file
    let mut last_check = Instant::now(); // for rotation/truncation checks
    let mut pending_line = String::new(); // buffer for partial last line of a chunk
    let mut _did_backfill = false; // for debug visibility only
    let mut last_api_call_id: Option<u32> = None;

    loop {
        debug_log!(
            "[watcher] tick: offset={} file={} pending={} did_backfill={}",
            last_offset,
            current_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "<none>".to_string()),
            pending_line.len(),
            _did_backfill
        );
        // Rotation/truncation check and initial open
        if last_check.elapsed() >= Duration::from_millis(1000) || current_path.is_none() {
            debug_log!("[watcher] checking for latest file or truncation");
            let latest = find_latest_log_file(&vrchat_dir);
            if latest.as_ref() != current_path.as_ref() {
                let is_initial_open = current_path.is_none();
                if let Some(p) = latest.clone() {
                    match fs::File::open(&p) {
                        Ok(mut f) => {
                            let len = f.metadata().map(|m| m.len()).unwrap_or(0);
                            // Purge if the log filename changed since last run (rotation)
                            if let Some(name_os) = p.file_name() {
                                if let Some(name) = name_os.to_str() {
                                    let prev = super::db::db_get_state("last_log_filename")
                                        .unwrap_or(None);
                                    if prev.as_deref() != Some(name) {
                                        let ts_now =
                                            Local::now().format("%Y.%m.%d %H:%M:%S").to_string();
                                        let _ = super::db::db_purge_all(&app, &ts_now, true);
                                        let _ = super::db::db_set_state("last_log_filename", name);
                                        emit_debug(&app, format!("Log rotation detected (new file: {name}); applied purge with timestamp {ts_now}"));
                                    }
                                }
                            }
                            // Backfill once on the very first open to reconstruct who is active
                            if is_initial_open {
                                const BACKFILL_SCAN_MAX: u64 = 4 * 1024 * 1024; // scan last 4MB for context
                                let start = len.saturating_sub(BACKFILL_SCAN_MAX);
                                if let Ok(mut bf) = fs::File::open(&p) {
                                    debug_log!(
                                        "[watcher] backfill: scanning tail {} bytes from {}",
                                        len - start,
                                        p.display()
                                    );
                                    let _ = bf.seek(SeekFrom::Start(start));
                                    let mut buf = Vec::with_capacity((len - start) as usize);
                                    if bf.read_to_end(&mut buf).is_ok() {
                                        let mut text = String::from_utf8_lossy(&buf).to_string();
                                        // If we started mid-line, skip to the first full line
                                        if start > 0 {
                                            if let Some(pos) = text.find('\n') {
                                                text = text.split_off(pos + 1);
                                            }
                                        }
                                        let lines: Vec<&str> = text.split('\n').collect();
                                        // Find the most recent anchor: either a "Joining wrld_..." line or a "Successfully joined room"
                                        let last_joining_idx = lines.iter().rposition(|raw| {
                                            re_joining.is_match(raw.trim_end_matches('\r'))
                                        });
                                        let last_success_idx = lines.iter().rposition(|raw| {
                                            re_purge3.is_match(raw.trim_end_matches('\r'))
                                        });

                                        let mut anchor: Option<(usize, bool)> = None; // (index, is_joining)
                                        if let Some(idx) = last_joining_idx {
                                            anchor = Some((idx, true));
                                        }
                                        if let Some(idx) = last_success_idx {
                                            match anchor {
                                                Some((prev_idx, _)) if idx > prev_idx => {
                                                    anchor = Some((idx, false));
                                                }
                                                None => anchor = Some((idx, false)),
                                                _ => {}
                                            }
                                        }

                                        if let Some((start_idx, anchor_is_joining)) = anchor {
                                            let mut purge_reason: Option<&'static str> = None;
                                            let mut purged_after_anchor = false;
                                            for raw in lines[start_idx + 1..].iter() {
                                                let line = raw.trim_end_matches('\r');
                                                if line.is_empty() {
                                                    continue;
                                                }
                                                if re_purge1.is_match(line) {
                                                    purge_reason = Some("Successfully left room");
                                                    purged_after_anchor = true;
                                                    break;
                                                }
                                                if re_purge2.is_match(line) {
                                                    purge_reason = Some("VRCNP: Stopping server");
                                                    purged_after_anchor = true;
                                                    break;
                                                }
                                                if re_purge3.is_match(line) {
                                                    // Only treat as purge if this is a subsequent success and we started from a join
                                                    if anchor_is_joining {
                                                        purge_reason = Some("Successfully joined room");
                                                        purged_after_anchor = true;
                                                        break;
                                                    }
                                                    // If we anchored on a success line already, a second success implies a new session started
                                                    purge_reason = Some("Successfully joined room");
                                                    purged_after_anchor = true;
                                                    break;
                                                }
                                                if re_quit.is_match(line) {
                                                    purge_reason = Some("Application quit");
                                                    purged_after_anchor = true;
                                                    break;
                                                }
                                            }

                                            let anchor_line = lines[start_idx].trim_end_matches('\r');
                                            let anchor_ts = re_ts
                                                .captures(anchor_line)
                                                .and_then(|c| c.get(1))
                                                .map(|m| m.as_str());
                                            if let Some(ts) = anchor_ts {
                                                if anchor_is_joining {
                                                    if let Some(caps) =
                                                        re_joining.captures(anchor_line)
                                                    {
                                                        let world_id = caps
                                                            .get(1)
                                                            .map(|m| m.as_str())
                                                            .unwrap_or("");
                                                        let instance_id = caps
                                                            .get(2)
                                                            .map(|m| m.as_str())
                                                            .unwrap_or("");
                                                        let region =
                                                            caps.get(3).map(|m| m.as_str());

                                                        if purged_after_anchor {
                                                            emit_debug(
                                                                &app,
                                                                format!(
                                                                    "Startup backfill skipped: purge marker '{:?}' detected after join at {ts} (world={world_id}, instance={instance_id}, region={:?})",
                                                                    purge_reason,
                                                                    region
                                                                ),
                                                            );
                                                        } else {
                                                            emit_debug(
                                                                &app,
                                                                format!(
                                                                    "Startup backfill using join at {ts}: world={world_id}, instance={instance_id}, region={:?}",
                                                                    region
                                                                ),
                                                            );
                                                            let _ = super::db::db_set_state(
                                                                "last_instance_join_ts",
                                                                ts,
                                                            );
                                                            // Record a system row so the paginated log view shows the transition
                                                            let msg = match region { Some(r) => format!("Joining: {} | Instance: {} | Region: {}", world_id, instance_id, r), None => format!("Joining: {} | Instance: {}", world_id, instance_id) };
                                                            let _ = super::db::db_insert_system_event(
                                                                &app,
                                                                ts,
                                                                "instance_changed",
                                                                Some(&msg),
                                                                Some(world_id),
                                                                Some(instance_id),
                                                                region,
                                                                false,
                                                            );
                                                            emit_debug(&app, format!("Backfill instance window established at {ts} (world={world_id}, instance={instance_id}, region={:?}) during startup scan", region));
                                                        }
                                                    }
                                                } else {
                                                    if purged_after_anchor {
                                                        emit_debug(
                                                            &app,
                                                            format!(
                                                                "Startup backfill skipped: purge marker '{:?}' detected after 'Successfully joined room' at {ts}",
                                                                purge_reason
                                                            ),
                                                        );
                                                    } else {
                                                        emit_debug(
                                                            &app,
                                                            format!(
                                                                "Startup backfill anchored to 'Successfully joined room' at {ts}"
                                                            ),
                                                        );
                                                        let _ = super::db::db_set_state(
                                                            "last_instance_join_ts",
                                                            ts,
                                                        );
                                                    }
                                                }

                                                if !purged_after_anchor {
                                                    // Reconstruct who is still in the instance by replaying lines
                                                    for raw in &lines[start_idx + 1..] {
                                                        let line = raw.trim_end_matches('\r');
                                                        if line.is_empty() {
                                                            continue;
                                                        }
                                                        let ts = match re_ts
                                                            .captures(line)
                                                            .and_then(|c| c.get(1))
                                                            .map(|m| m.as_str())
                                                        {
                                                            Some(t) => t,
                                                            None => continue,
                                                        };
                                                        if let Some(caps) = re_join.captures(line) {
                                                            let username = caps
                                                                .get(1)
                                                                .map(|m| m.as_str().trim())
                                                                .unwrap_or("");
                                                            let uid = caps
                                                                .get(2)
                                                                .map(|m| m.as_str())
                                                                .unwrap_or("");
                                                            if !uid.is_empty() {
                                                                let _ = super::db::db_insert_join(
                                                                    &app, ts, uid, username, false,
                                                                );
                                                            }
                                                        } else if let Some(caps) =
                                                            re_left.captures(line)
                                                        {
                                                            let uid = caps
                                                                .get(2)
                                                                .map(|m| m.as_str())
                                                                .unwrap_or("");
                                                            if !uid.is_empty() {
                                                                let _ = super::db::db_update_leave(
                                                                    &app, ts, uid, false,
                                                                );
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                // After the scan, signal the frontend that it can load the initial state
                                let _ = app.emit("watcher_ready", ());
                                // Seek to the end of the file to process new lines
                                let _ = f.seek(SeekFrom::Start(len));
                                last_offset = len;
                                pending_line.clear();
                                last_api_call_id = None;
                                file = Some(f);
                                current_path = Some(p);
                                debug_log!(
                                    "[watcher] opened latest log: {} (len={})",
                                    current_path.as_ref().unwrap().display(),
                                    last_offset
                                );
                                _did_backfill = true;
                            }
                        }
                        Err(e) => {
                            eprintln!("[watcher] failed to open log file {}: {e}", p.display());
                            file = None;
                            current_path = None;
                            last_offset = 0;
                            pending_line.clear();
                        }
                    }
                } else {
                    debug_log!("[watcher] no log file found in {}", vrchat_dir.display());
                    file = None;
                    current_path = None;
                    last_offset = 0;
                    pending_line.clear();
                }
            } else if let (Some(ref p), Some(ref mut f)) = (current_path.as_ref(), file.as_mut()) {
                // If the log got truncated (e.g., new session), start over from 0
                if let Ok(ms) = fs::metadata(p) {
                    if ms.len() < last_offset {
                        debug_log!("[watcher] truncation detected; resetting offset to 0");
                        last_offset = 0;
                        let _ = f.seek(SeekFrom::Start(0));
                        pending_line.clear();
                    }
                }
            }
            last_check = Instant::now();
        }
        // Read newly appended bytes and process complete lines when available
        // I had a *lot* of help making this function. It's a conglomerate of multiple previous temporary tools, with a focus on low overhead.
        // The core structure was taken from a stackoverflow answer about a performant large log file reader, but I lost the LINK.
        if let Some(ref mut f) = file {
            if let Ok(meta) = f.metadata() {
                let size = meta.len();
                if size > last_offset {
                    let mut remaining = (size - last_offset) as usize;
                    let mut buf = vec![0u8; 64 * 1024];
                    while remaining > 0 {
                        let to_read = buf.len().min(remaining);
                        let n = match f.read(&mut buf[..to_read]) {
                            Ok(n) => n,
                            Err(e) => {
                                eprintln!("[watcher] read error: {e}");
                                break;
                            }
                        };
                        if n == 0 {
                            break;
                        }
                        last_offset += n as u64;
                        remaining -= n;
                        let chunk = String::from_utf8_lossy(&buf[..n]);
                        pending_line.push_str(&chunk);
                        // Only process full lines; keep the remainder in pending_line
                        if let Some(nl_idx) = pending_line.rfind('\n') {
                            let to_process = pending_line[..nl_idx].to_string();
                            let remainder = pending_line[nl_idx + 1..].to_string();
                            pending_line = remainder;
                            for raw in to_process.split('\n') {
                                let line = raw.trim_end_matches('\r');
                                if line.is_empty() {
                                    continue;
                                }
                                let ts_cap = re_ts
                                    .captures(line)
                                    .and_then(|c| c.get(1))
                                    .map(|m| m.as_str());
                                let ts = match ts_cap {
                                    Some(t) => t,
                                    None => continue,
                                };
                                if line.contains("[Behaviour] Switching") {
                                    if let Some(caps) = re_switch_avatar.captures(line) {
                                        let avatar_owner = caps
                                            .get(1)
                                            .map(|m| m.as_str().trim())
                                            .unwrap_or("");
                                        let avatar_name = caps
                                            .get(2)
                                            .map(|m| m.as_str().trim())
                                            .unwrap_or("");
                                        emit_debug(
                                            &app,
                                            format!(
                                                "[watcher] parsed avatar switch line -> owner='{}' avatar='{}' ts={}",
                                                avatar_owner,
                                                avatar_name,
                                                ts
                                            ),
                                        );
                                        if avatar_owner.trim().is_empty()
                                            && avatar_name.trim().is_empty()
                                        {
                                            emit_debug(
                                                &app,
                                                format!(
                                                    "[watcher] avatar log skipped, empty owner/name -> owner='{}' avatar='{}' line={}",
                                                    avatar_owner,
                                                    avatar_name,
                                                    line
                                                ),
                                            );
                                        } else {
                                            let owner_norm = avatar_owner.trim();
                                            let name_norm = avatar_name.trim();
                                            if owner_norm.is_empty() || name_norm.is_empty() {
                                                emit_debug(
                                                    &app,
                                                    format!(
                                                        "[watcher] avatar log skipped after trim -> owner='{}' avatar='{}' line={}",
                                                        avatar_owner,
                                                        avatar_name,
                                                        line
                                                    ),
                                                );
                                            } else {
                                                match super::db::db_insert_avatar_log(
                                                    &app,
                                                    ts,
                                                    owner_norm,
                                                    name_norm,
                                                ) {
                                                    Ok(_) => emit_debug(
                                                        &app,
                                                        format!(
                                                            "[watcher] avatar log inserted :: user={} avatar={} ts={}",
                                                            owner_norm,
                                                            name_norm,
                                                            ts
                                                        ),
                                                    ),
                                                    Err(err) => emit_debug(
                                                        &app,
                                                        format!(
                                                            "[watcher] avatar log insert failed :: user={} avatar={} ts={} err={:?}",
                                                            owner_norm,
                                                            name_norm,
                                                            ts,
                                                            err
                                                        ),
                                                    ),
                                                }
                                            }
                                        }
                                    } else {
                                        emit_debug(
                                            &app,
                                            format!(
                                                "[watcher] switching line did not match regex -> {}",
                                                line
                                            ),
                                        );
                                    }
                                    continue;
                                }
                                if let Some(idx) = line.find("[API] [") {
                                    let segment = &line[idx + 6..]; // skip "[API] "
                                    if let Some(end_bracket) = segment.find(']') {
                                        let (call_id_str, remainder) =
                                            segment.split_at(end_bracket);
                                        let remainder = remainder[1..].trim_start();
                                        if remainder.starts_with("Sending Get request to ") {
                                            let url = &remainder["Sending Get request to ".len()..];
                                            if url.starts_with("https://api.vrchat.cloud/api/1/analysis")
            || url.starts_with("https://api.vrchat.cloud/api/1/avatars")
            || url.contains("/prints/")
            || url.contains("/inventory/")
        {
                                                let call_id_parsed =
                                                    call_id_str.trim().parse::<u32>().ok();
                                                let should_emit = match call_id_parsed {
                                                    Some(id) => {
                                                        if last_api_call_id == Some(id) {
                                                            false
                                                        } else {
                                                            last_api_call_id = Some(id);
                                                            true
                                                        }
                                                    }
                                                    None => true,
                                                };
                                                if should_emit {
                                                    let message = match call_id_parsed {
                                                        Some(id) => {
                                                            format!(
                                                                "[VRCAPI] call #{id} -> GET {url}"
                                                            )
                                                        }
                                                        None => {
                                                            format!("[VRCAPI] GET {url}")
                                                        }
                                                    };
                                                    emit_debug(&app, message);
                                                }
                                                if let Some(version_caps) =
                                                    re_analysis_path.captures(url)
                                                {
                                                    let file_id = version_caps
                                                        .get(1)
                                                        .map(|m| m.as_str().to_string())
                                                        .unwrap_or_default();
                                                    let version = version_caps
                                                        .get(2)
                                                        .and_then(|m| m.as_str().parse::<i32>().ok())
                                                        .unwrap_or_default();
                                                    emit_debug(
                                                        &app,
                                                        format!(
                                                            "[watcher] analysis request detected -> file_id={} version={} ts={}",
                                                            file_id,
                                                            version,
                                                            ts
                                                        ),
                                                    );
                                                    if !file_id.is_empty() && version > 0 {
                                                        api_client.submit(file_id.clone(), version);
                                                        emit_debug(
                                                            &app,
                                                            format!(
                                                                "[API] analysis :: file_id={} version={} url={} ts={}",
                                                                file_id,
                                                                version,
                                                                url,
                                                                ts
                                                            ),
                                                        );
                                                    }
                                                }
                                                if let Some(print_caps) =
                                                    re_prints_path.captures(url)
                                                {
                                                    if let Some(identifier) = print_caps
                                                        .get(1)
                                                        .map(|m| m.as_str().to_string())
                                                    {
                                                        emit_debug(
                                                            &app,
                                                            format!(
                                                                "[API] prints :: id={} url={} ts={}",
                                                                identifier,
                                                                url,
                                                                ts
                                                            ),
                                                        );
                                                        emit_debug(
                                                            &app,
                                                            format!(
                                                                "[watcher] prints request detected -> id={} ts={}",
                                                                identifier,
                                                                ts
                                                            ),
                                                        );
                                                        emit_debug(
                                                            &app,
                                                            format!(
                                                                "[media] send invChk print id={identifier}"
                                                            ),
                                                        );
                                                        api_client.submit_print(identifier.clone());
                                                    }
                                                }
                                                if let Some(inv_caps) =
                                                    re_inventory_path.captures(url)
                                                {
                                                    let user_id = inv_caps
                                                        .get(1)
                                                        .map(|m| m.as_str().to_string())
                                                        .unwrap_or_default();
                                                    let inventory_id = inv_caps
                                                        .get(2)
                                                        .map(|m| m.as_str().to_string())
                                                        .unwrap_or_default();
                                                    if !user_id.is_empty() && !inventory_id.is_empty() {
                                                        let combined = format!("{}&{}", user_id, inventory_id);
                                                        emit_debug(
                                                            &app,
                                                            format!(
                                                                "[API] inventory :: id={} url={} ts={}",
                                                                combined,
                                                                url,
                                                                ts
                                                            ),
                                                        );
                                                        emit_debug(
                                                            &app,
                                                            format!(
                                                                "[watcher] inventory request detected -> ids={} ts={}",
                                                                combined,
                                                                ts
                                                            ),
                                                        );
                                                        emit_debug(
                                                            &app,
                                                            format!(
                                                                "[media] send invChk inventory id={}",
                                                                combined
                                                            ),
                                                        );
                                                        api_client.submit_inventory(combined.clone());
                                                    }
                                                }
                                                continue;
                                            }
                                        }
                                    }
                                }
                                // Purge triggers: end-of-session markers or explicit VRChat logs
                                if re_purge1.is_match(line)
                                    || re_purge2.is_match(line)
                                    || re_purge3.is_match(line)
                                    || re_quit.is_match(line)
                                {
                                    let loop_trigger = if re_purge1.is_match(line) {
                                        "Successfully left room"
                                    } else if re_purge2.is_match(line) {
                                        "VRCNP: Stopping server"
                                    } else if re_purge3.is_match(line) {
                                        "Successfully joined room"
                                    } else {
                                        "Application quit"
                                    };
                                    emit_debug(
                                        &app,
                                        format!(
                                            "Purge trigger detected at {ts}: reason='{loop_trigger}'"
                                        ),
                                    );
                                    last_api_call_id = None;
                                    if let Err(e) = super::db::db_purge_all(&app, ts, true) {
                                        eprintln!("[watcher] failed to purge all: {e:?}");
                                    }
                                    continue;
                                }
                                // Instance change: close previous, emit a system row and UI event
                                if let Some(caps) = re_joining.captures(line) {
                                    last_api_call_id = None;
                                    if let Err(e) = super::db::db_purge_all(&app, ts, true) {
                                        eprintln!("[watcher] failed to purge all on instance change: {e:?}");
                                    }
                                    let world_id = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                                    let instance_id = caps.get(2).map(|m| m.as_str()).unwrap_or("");
                                    let region = caps.get(3).map(|m| m.as_str());
                                    let region_display = region.unwrap_or("auto");
                                    emit_debug(
                                        &app,
                                        format!(
                                            "World join detected at {ts}: world={world_id} instance={instance_id} region={region_display}"
                                        ),
                                    );
                                    let _ = app.emit("instance_changed", serde_json::json!({ "worldId": world_id, "instanceId": instance_id, "region": region, "ts": ts }));
                                    let msg = match region {
                                        Some(r) => format!(
                                            "Joining: {} | Instance: {} | Region: {}",
                                            world_id, instance_id, r
                                        ),
                                        None => format!(
                                            "Joining: {} | Instance: {}",
                                            world_id, instance_id
                                        ),
                                    };
                                    let _ = super::db::db_insert_system_event(
                                        &app,
                                        ts,
                                        "instance_changed",
                                        Some(&msg),
                                        Some(world_id),
                                        Some(instance_id),
                                        region,
                                        true,
                                    );
                                    continue;
                                }
                                // Player joined: insert row, cache username, maybe notify
                                if let Some(caps) = re_join.captures(line) {
                                    let username =
                                        caps.get(1).map(|m| m.as_str().trim()).unwrap_or("");
                                    let uid = caps.get(2).map(|m| m.as_str()).unwrap_or("");
                                    if !uid.is_empty() {
                                        if let Err(e) =
                                            super::db::db_insert_join(&app, ts, uid, username, true)
                                        {
                                            eprintln!("[watcher] failed to insert join: {e:?}");
                                        } else {
                                            emit_debug(&app, format!(
                                                "Watcher processed join line -> uid={uid}, username={username}, ts={ts}, emit=true"
                                            ));
                                        }
                                        let mut all = super::notes::load_all_notes();
                                        let existing = all.usernames.get(uid).cloned();
                                        let mut changed = false;
                                        if username.is_empty() {
                                            if existing.is_none() {
                                                all.usernames.insert(
                                                    uid.to_string(),
                                                    "Not Yet Recorded".to_string(),
                                                );
                                                changed = true;
                                            }
                                        } else {
                                            if existing.as_deref().unwrap_or("") != username {
                                                all.usernames
                                                    .insert(uid.to_string(), username.to_string());
                                                changed = true;
                                            }
                                        }
                                        if changed {
                                            let _ = super::notes::save_all_notes(&all);
                                        }
                                        let watch_override = all.sounds.get(uid).cloned();
                                        if let Some(path) = watch_override {
                                            let conf = crate::config::load_config();
                                            let vol = conf.sound_volume.unwrap_or(1.0);
                                            crate::sound::play_user_sound(&path, vol);
                                        } else if all.watchlist.get(uid).copied().unwrap_or(false) {
                                            #[cfg(target_os = "windows")]
                                            {
                                                let msg = format!("{} has joined", username);
                                                let _ = winrt_notification::Toast::new("FCH")
                                                    .title("- FCH Notifier -")
                                                    .text1(&msg)
                                                    .show();
                                            }
                                            let _ = app.emit("sound_triggered", serde_json::json!({ "source": "local_watchlist", "userId": uid, "username": username, "ts": ts }));
                                            crate::sound::play_watch_sound();
                                        }
                                    }
                                    continue;
                                }
                                // Player left: update the most recent open join for that user
                                if let Some(caps) = re_left.captures(line) {
                                    let uid = caps.get(2).map(|m| m.as_str()).unwrap_or("");
                                    if !uid.is_empty() {
                                        if let Err(e) =
                                            super::db::db_update_leave(&app, ts, uid, true)
                                        {
                                            eprintln!("[watcher] failed to update leave: {e:?}");
                                        } else {
                                            emit_debug(
                                                &app,
                                                format!(
																					"Watcher processed leave line -> uid={uid}, ts={ts}, emit=true"
																				),
                                            );
                                        }
                                    }
                                    continue;
                                }
                                // Fallback: some leaves log as "Destroying <username>"
                                if let Some(caps) = re_destroying.captures(line) {
                                    let username =
                                        caps.get(1).map(|m| m.as_str().trim()).unwrap_or("");
                                    if !username.is_empty() {
                                        if let Err(e) = super::db::db_update_leave_by_username(
                                            &app, ts, username, true,
                                        ) {
                                            eprintln!("[watcher] failed to update leave by username: {e:?}");
                                        } else {
                                            emit_debug(&app, format!(
																					"Watcher processed username-only leave -> username={username}, ts={ts}, emit=true"
																				));
                                        }
                                    }
                                    continue;
                                }
                            }
                        }
                    }
                }
            }
        }
        // Short sleep prevents busy-looping while tailing the file
        tokio::time::sleep(Duration::from_millis(750)).await;
    }
}

// VRChat logs live under LocalLow\VRChat\VRChat
fn default_vrchat_log_dir() -> PathBuf {
    let local_low = std::env::var("LOCALAPPDATA")
        .ok()
        .and_then(|p| PathBuf::from(p).parent().map(|pp| pp.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("C:/Users/Public"));
    local_low.join("LocalLow").join("VRChat").join("VRChat")
}

// Pick the newest output_log_*.txt by modification time
fn find_latest_log_file(dir: &PathBuf) -> Option<PathBuf> {
    let mut entries: Vec<(std::time::SystemTime, PathBuf)> = fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let p = e.path();
            let name = p.file_name()?.to_string_lossy().into_owned();
            if name.starts_with("output_log_") && name.ends_with(".txt") {
                let mt = e.metadata().ok()?.modified().ok()?;
                Some((mt, p))
            } else {
                None
            }
        })
        .collect();
    entries.sort_by_key(|(mt, _)| std::cmp::Reverse(*mt));
    entries.into_iter().map(|(_, p)| p).next()
}

mod api_checks {
    use super::emit_debug;
    use serde::{Deserialize, Serialize};
    use serde_json::json;
    use std::collections::VecDeque;
    use std::sync::OnceLock;
    use std::time::Duration;
    use tauri::Emitter;

    #[derive(Clone)]
    pub struct Client {
        sender: tokio::sync::mpsc::UnboundedSender<Job>,
    }

    #[derive(Debug, Clone)]
    enum Job {
        SecurityCheck { file_id: String, version: i32 },
        InvCheck { identifier: String },
    }

    #[derive(Serialize, Deserialize, Debug)]
    struct ApiResponse {
        success: bool,
        results: Option<Vec<serde_json::Value>>,
        error: Option<String>,
    }

    static INSTANCE: OnceLock<Client> = OnceLock::new();

    impl Client {
        pub fn new(app: tauri::AppHandle) -> Client {
            INSTANCE
                .get_or_init(|| {
                    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                    let client = Client { sender: tx.clone() };
                    tokio::task::spawn(worker(app.clone(), rx));
                    client
                })
                .clone()
        }

        pub fn submit(&self, file_id: String, version: i32) {
            let _ = self.sender.send(Job::SecurityCheck { file_id, version });
        }

        pub fn submit_inventory(&self, identifier: String) {
            let _ = self.sender.send(Job::InvCheck { identifier });
        }

        pub fn submit_print(&self, identifier: String) {
            let _ = self.sender.send(Job::InvCheck { identifier });
        }
    }

    async fn worker(app: tauri::AppHandle, mut rx: tokio::sync::mpsc::UnboundedReceiver<Job>) {
        let scheme = std::env::var("API_CHECKS_HTTP_SCHEME").unwrap_or_else(|_| "https".into());
        let host =
            std::env::var("API_CHECKS_HTTP_HOST").unwrap_or_else(|_| "fch-toolkit.com".into());
        let path =
            std::env::var("API_CHECKS_HTTP_PATH").unwrap_or_else(|_| "/api/security-check".into());
        let url = std::env::var("API_CHECKS_HTTP_URL")
            .unwrap_or_else(|_| format!("{scheme}://{host}{path}"));
        let inv_path = std::env::var("API_INV_CHECK_PATH").unwrap_or_else(|_| "/invChk".into());
        let inv_url = std::env::var("API_INV_CHECK_URL")
            .unwrap_or_else(|_| format!("{scheme}://{host}{inv_path}"));
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(90))
            .build()
            .expect("failed to build http client");

        let mut backlog: VecDeque<Job> = VecDeque::new();

        loop {
            while let Ok(job) = rx.try_recv() {
                match &job {
                    Job::SecurityCheck { file_id, version } => emit_debug(
                        &app,
                        format!(
                            "[apiChecks] job enqueued -> file_id={} version={}",
                            file_id, version
                        ),
                    ),
                    Job::InvCheck { identifier } => emit_debug(
                        &app,
                        format!("[apiChecks] invChk job enqueued -> id={identifier}"),
                    ),
                }
                backlog.push_back(job);
                let _ = app.emit("api_queue_length", backlog.len() as i64);
            }

            let job = match backlog.pop_front() {
                Some(job) => {
                    match &job {
                        Job::SecurityCheck { file_id, version } => emit_debug(
                            &app,
                            format!(
                                "[apiChecks] processing job -> file_id={} version={}",
                                file_id, version
                            ),
                        ),
                        Job::InvCheck { identifier } => emit_debug(
                            &app,
                            format!("[apiChecks] processing invChk -> id={identifier}"),
                        ),
                    }
                    job
                }
                None => {
                    match rx.recv().await {
                        Some(job) => job,
                        None => break,
                    }
                }
            };

            match job {
                Job::SecurityCheck { file_id, version } => {
                    let payload = json!({
                        "jobs": [json!({ "fileId": file_id, "version": version })]
                    });

                    match http
                        .post(&url)
                        .timeout(Duration::from_secs(17))
                        .json(&payload)
                        .send()
                        .await
                    {
                        Ok(resp) => match resp.json::<ApiResponse>().await {
                            Ok(parsed) => {
                                if !parsed.success {
                                    emit_debug(
                                        &app,
                                        format!(
                                            "[VRCAPI] security-check error: {}",
                                            parsed.error.unwrap_or_else(|| "Unknown".into())
                                        ),
                                    );
                                    backlog.push_back(Job::SecurityCheck { file_id, version });
                                    let _ = app.emit("api_queue_length", backlog.len() as i64);
                                } else if let Some(results) = parsed.results {
                                    for value in results {
                                        if let Some(fid) = value.get("file_id").and_then(|v| v.as_str()) {
                                            let version_val = value
                                                .get("version")
                                                .and_then(|v| v.as_i64())
                                                .unwrap_or_default();
                                            let success = value
                                                .get("success")
                                                .and_then(|v| v.as_bool())
                                                .unwrap_or(true);
                                            emit_debug(
                                                &app,
                                                format!(
                                                    "[VRCAPI] security-check complete {fid} v{version_val} success={success}"
                                                ),
                                            );

                                            let version_i32 = version_val as i32;
                                            let file_json = value.get("file").cloned();
                                            let security_json = value.get("security").cloned();
                                            let owner_id = value
                                                .get("owner_id")
                                                .and_then(|v| v.as_str())
                                                .or_else(|| {
                                                    value
                                                        .get("file")
                                                        .and_then(|f| f.get("ownerId"))
                                                        .and_then(|v| v.as_str())
                                                })
                                                .unwrap_or("");
                                            let avatar_name = value
                                                .get("avatar_name")
                                                .and_then(|v| v.as_str())
                                                .or_else(|| {
                                                    value
                                                        .get("avatarName")
                                                        .and_then(|v| v.as_str())
                                                })
                                                .or_else(|| {
                                                    value
                                                        .get("file")
                                                        .and_then(|f| f.get("name"))
                                                        .and_then(|v| v.as_str())
                                                })
                                                .unwrap_or("");

                                            if !avatar_name.trim().is_empty() {
                                                match crate::db::db_insert_avatar_details(
                                                    &app,
                                                    avatar_name,
                                                    owner_id,
                                                    Some(fid),
                                                    Some(version_i32),
                                                    file_json.as_ref(),
                                                    security_json.as_ref(),
                                                ) {
                                                    Ok(_) => emit_debug(
                                                        &app,
                                                        format!(
                                                            "[VRCAPI] avatar details stored :: avatar={} owner={} version={}",
                                                            avatar_name, owner_id, version_i32
                                                        ),
                                                    ),
                                                    Err(err) => emit_debug(
                                                        &app,
                                                        format!(
                                                            "[VRCAPI] avatar details store failed :: avatar={} err={:?}",
                                                            avatar_name, err
                                                        ),
                                                    ),
                                                }
                                            }

                                            let _ = app.emit("api_checks_result", value.clone());
                                        }
                                    }
                                    let _ = app.emit("api_queue_length", backlog.len() as i64);
                                }
                            }
                            Err(err) => {
                                emit_debug(
                                    &app,
                                    format!("[VRCAPI] security-check parse failed: {err}")
                                );
                                backlog.push_back(Job::SecurityCheck { file_id, version });
                                let _ = app.emit("api_queue_length", backlog.len() as i64);
                                tokio::time::sleep(Duration::from_secs(3)).await;
                            }
                        },
                        Err(err) => {
                            emit_debug(
                                &app,
                                format!("[VRCAPI] security-check request failed: {err}")
                            );
                            backlog.push_back(Job::SecurityCheck { file_id, version });
                            let _ = app.emit("api_queue_length", backlog.len() as i64);
                            tokio::time::sleep(Duration::from_secs(3)).await;
                        }
                    }
                }
                Job::InvCheck { identifier } => {
                    match http
                        .post(&inv_url)
                        .timeout(Duration::from_secs(17))
                        .json(&json!({ "id": identifier }))
                        .send()
                        .await
                    {
                        Ok(resp) => {
                            if !resp.status().is_success() {
                                emit_debug(
                                    &app,
                                    format!(
                                        "[apiChecks] invChk error: HTTP {}",
                                        resp.status()
                                    )
                                );
                            } else {
                                emit_debug(
                                    &app,
                                    format!(
                                        "[apiChecks] invChk dispatched successfully id={identifier}"
                                    ),
                                );
                                if let Ok(json_value) = resp.json::<serde_json::Value>().await {
                                    emit_debug(
                                        &app,
                                        format!("[media] invChk result raw: {}", json_value),
                                    );
                                    let payload = json_value.get("payload");
                                    if let Some(obj) = payload.and_then(|v| v.as_object()) {
                                        let resolved_type = obj
                                            .get("itemType")
                                            .or_else(|| obj.get("item_type"))
                                            .and_then(|v| v.as_str())
                                            .map(|s| s.to_lowercase())
                                            .unwrap_or_default();
                                        let owner = obj
                                            .get("ownerId")
                                            .or_else(|| obj.get("owner_id"))
                                            .or_else(|| obj.get("holderId"))
                                            .or_else(|| obj.get("holder_id"))
                                            .and_then(|v| v.as_str())
                                            .map(|s| s.to_string());
                                        let image = obj
                                            .get("imageUrl")
                                            .or_else(|| obj.get("image_url"))
                                            .and_then(|v| v.as_str())
                                            .map(|s| s.to_string());
                                        let canonical_type = match resolved_type.as_str() {
                                            "print" | "sticker" | "emoji" => resolved_type.clone(),
                                            other => {
                                                let fields = [
                                                    obj.get("id").and_then(|v| v.as_str()),
                                                    obj.get("itemId").and_then(|v| v.as_str()),
                                                    obj.get("item_id").and_then(|v| v.as_str()),
                                                    obj.get("inventoryId").and_then(|v| v.as_str()),
                                                    obj.get("inventory_id").and_then(|v| v.as_str()),
                                                ];

                                                let mut inferred: Option<String> = None;
                                                if identifier.starts_with("prnt_")
                                                    || fields.iter().flatten().any(|v| v.starts_with("prnt_"))
                                                {
                                                    inferred = Some("print".to_string());
                                                } else if fields
                                                    .iter()
                                                    .flatten()
                                                    .any(|v| v.starts_with("sticker_"))
                                                {
                                                    inferred = Some("sticker".to_string());
                                                } else if fields
                                                    .iter()
                                                    .flatten()
                                                    .any(|v| v.starts_with("emoji_"))
                                                {
                                                    inferred = Some("emoji".to_string());
                                                } else if let Some(meta) = obj
                                                    .get("metadata")
                                                    .and_then(|v| v.as_object())
                                                {
                                                    if let Some(template_id) = meta
                                                        .get("templateId")
                                                        .or_else(|| meta.get("template_id"))
                                                        .and_then(|v| v.as_str())
                                                    {
                                                        let lower = template_id.to_lowercase();
                                                        if lower.contains("sticker") {
                                                            inferred = Some("sticker".to_string());
                                                        } else if lower.contains("emoji") {
                                                            inferred = Some("emoji".to_string());
                                                        }
                                                    }
                                                    if inferred.is_none() {
                                                        if let Some(tags_val) = meta.get("tags") {
                                                            if let Some(arr) = tags_val.as_array() {
                                                                if arr.iter().any(|tag| {
                                                                    tag.as_str()
                                                                        .map(|t| t.eq_ignore_ascii_case("sticker"))
                                                                        .unwrap_or(false)
                                                                }) {
                                                                    inferred = Some("sticker".to_string());
                                                                } else if arr.iter().any(|tag| {
                                                                    tag.as_str()
                                                                        .map(|t| t.eq_ignore_ascii_case("emoji"))
                                                                        .unwrap_or(false)
                                                                }) {
                                                                    inferred = Some("emoji".to_string());
                                                                }
                                                            }
                                                        }
                                                    }
                                                }

                                                inferred.unwrap_or_else(|| {
                                                    if other.is_empty() {
                                                        "inventory".to_string()
                                                    } else {
                                                        other.to_string()
                                                    }
                                                })
                                            }
                                        };
                                        let normalized = if canonical_type == "print" {
                                            identifier.clone()
                                        } else {
                                            if identifier.contains('&') {
                                                identifier.clone()
                                            } else if let (Some(owner_ref), Some(id_ref)) = (
                                                owner.as_ref(),
                                                obj.get("id").and_then(|v| v.as_str()),
                                            ) {
                                                format!("{}&{}", owner_ref, id_ref)
                                            } else {
                                                identifier.clone()
                                            }
                                        };
                                        let _ = crate::db::db_upsert_media_item(
                                            normalized.as_str(),
                                            canonical_type.as_str(),
                                            owner.as_deref(),
                                            image.as_deref(),
                                        );
                                        let _ = app.emit(
                                            "media_item_updated",
                                            serde_json::json!({
                                                "id": normalized,
                                                "itemType": canonical_type,
                                                "ownerId": owner,
                                            }),
                                        );
                                        emit_debug(
                                            &app,
                                            format!(
                                                "[media] emitted media_item_updated (inventory) id={}",
                                                normalized
                                            ),
                                        );
                                    }
                                }
                            }
                            let _ = app.emit("api_queue_length", backlog.len() as i64);
                        }
                        Err(err) => {
                            emit_debug(
                                &app,
                                format!("[apiChecks] invChk request failed: {err}")
                            );
                            backlog.push_back(Job::InvCheck { identifier });
                            let _ = app.emit("api_queue_length", backlog.len() as i64);
                            tokio::time::sleep(Duration::from_secs(3)).await;
                        }
                    }
                }
            }
        }
    }
}
