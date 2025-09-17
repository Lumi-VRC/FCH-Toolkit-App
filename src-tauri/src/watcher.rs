// Watcher: tails the VRChat log file, parses lines incrementally, and streams
// events to the UI while maintaining a SQLite mirror of joins/leaves and
// recording system events (like instance changes).

use std::{fs, io::{Read, Seek, SeekFrom}, path::PathBuf, time::{Duration, Instant}, sync::{Arc, Mutex}};
use anyhow::Result;
use regex::Regex;
use tauri::Emitter; // brings .emit() for sending events to the front-end

// Debug-only logging helper. To troubleshoot tailing, replace the body with
// eprintln!($($t)*); to see messages at runtime. Left empty to avoid spam.
// Debug logging in this language is soooooooo fucking gay.
macro_rules! debug_log { ($($t:tt)*) => {{}} }

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
pub async fn search_log_file(app_handle: tauri::AppHandle, query: String, search_token: i32, state: tauri::State<'_, SearchState>) -> Result<Vec<usize>, String> {
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
		app_handle.emit("search_progress", serde_json::json!({ "progress": progress, "token": search_token })).unwrap();
	}
	Ok(matches)
}

// Tail the latest VRChat log and emit/record events as they arrive.
async fn log_watch_loop(app: tauri::AppHandle) -> Result<()> {
	let vrchat_dir = default_vrchat_log_dir();
	debug_log!("[watcher] started; dir={}", vrchat_dir.display());
	// Regexes for timestamp, joins, leaves, purges, and instance changes
	let re_ts = Regex::new(r"^(\d{4}\.\d{2}\.\d{2}\s+\d{2}:\d{2}:\d{2})").unwrap();
	let re_join = Regex::new(r"OnPlayerJoined\s+(?:\[[^\]]+\]\s*)?([^\r\n(]+?)\s*\((usr_[a-f0-9\-]{36})\)").unwrap();
	let re_left = Regex::new(r"OnPlayerLeft\s+([^\r\n(]+?)\s*\((usr_[a-f0-9\-]{36})\)").unwrap();
	let re_purge1 = Regex::new(r"Successfully left room").unwrap();
	let re_purge2 = Regex::new(r"VRCNP: Stopping server").unwrap();
	let re_purge3 = Regex::new(r"Successfully joined room").unwrap();
	let re_joining = Regex::new(r"Joining\s+(wrld_[a-f0-9\-]{36}):([^~\s]+)(?:~region\(([^)]+)\))?").unwrap();

	// Current file and tailing state
	let mut current_path: Option<PathBuf> = None;
	let mut file: Option<fs::File> = None;
	let mut last_offset: u64 = 0; // where we've read up to in the file
	let mut last_check = Instant::now(); // for rotation/truncation checks
	let mut pending_line = String::new(); // buffer for partial last line of a chunk
	let mut _did_backfill = false; // for debug visibility only

	loop {
		debug_log!("[watcher] tick: offset={} file={} pending={} did_backfill={}",
			last_offset,
			current_path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "<none>".to_string()),
			pending_line.len(),
			_did_backfill);
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
							// Backfill once on the very first open to reconstruct who is active
							if is_initial_open {
								const BACKFILL_SCAN_MAX: u64 = 4 * 1024 * 1024; // scan last 4MB for context
								let start = len.saturating_sub(BACKFILL_SCAN_MAX);
								if let Ok(mut bf) = fs::File::open(&p) {
									debug_log!("[watcher] backfill: scanning tail {} bytes from {}", len - start, p.display());
									let _ = bf.seek(SeekFrom::Start(start));
									let mut buf = Vec::with_capacity((len - start) as usize);
									if bf.read_to_end(&mut buf).is_ok() {
										let mut text = String::from_utf8_lossy(&buf).to_string();
										// If we started mid-line, skip to the first full line
										if start > 0 {
											if let Some(pos) = text.find('\n') { text = text.split_off(pos + 1); }
										}
										let lines: Vec<&str> = text.split('\n').collect();
										// Find last "Joining wrld..."; starts a new instance section
										let last_joining_idx = lines.iter().rposition(|raw| re_joining.is_match(raw.trim_end_matches('\r')));
										if let Some(start_idx) = last_joining_idx {
											// If the log shows a clean purge after joining, skip backfill
											let purged_after_join = lines[start_idx+1..].iter().any(|raw| {
												let line = raw.trim_end_matches('\r');
												re_purge1.is_match(line) || re_purge2.is_match(line)
											});
											if !purged_after_join {
												if let Some(joining_line) = lines.get(start_idx) {
													if let Some(caps) = re_joining.captures(joining_line) {
														if let Some(ts) = caps.get(0).and_then(|m| re_ts.captures(m.as_str())).and_then(|c| c.get(1)).map(|m| m.as_str()) {
															let _ = super::db::db_set_state("last_instance_join_ts", ts);
															let world_id = caps.get(1).map(|m| m.as_str()).unwrap_or("");
															let instance_id = caps.get(2).map(|m| m.as_str()).unwrap_or("");
															let region = caps.get(3).map(|m| m.as_str());
															// Record a system row so the paginated log view shows the transition
															let msg = match region { Some(r) => format!("Joining: {} | Instance: {} | Region: {}", world_id, instance_id, r), None => format!("Joining: {} | Instance: {}", world_id, instance_id) };
															let _ = super::db::db_insert_system_event(&app, ts, "instance_changed", Some(&msg), Some(world_id), Some(instance_id), region, false);
														}
													}
												}
												// Reconstruct who is still in the instance by replaying lines
												for raw in &lines[start_idx+1..] {
													let line = raw.trim_end_matches('\r');
													if line.is_empty() { continue; }
													let ts = match re_ts.captures(line).and_then(|c| c.get(1)).map(|m| m.as_str()) { Some(t) => t, None => continue };
													if let Some(caps) = re_join.captures(line) {
														let username = caps.get(1).map(|m| m.as_str().trim()).unwrap_or("");
														let uid = caps.get(2).map(|m| m.as_str()).unwrap_or("");
														if !uid.is_empty() { let _ = super::db::db_insert_join(&app, ts, uid, username, false); }
													} else if let Some(caps) = re_left.captures(line) {
														let uid = caps.get(2).map(|m| m.as_str()).unwrap_or("");
														if !uid.is_empty() { let _ = super::db::db_update_leave(&app, ts, uid, false); }
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
								file = Some(f);
								current_path = Some(p);
								debug_log!("[watcher] opened latest log: {} (len={})", current_path.as_ref().unwrap().display(), last_offset);
								_did_backfill = true;
							}
						}
						Err(e) => { eprintln!("[watcher] failed to open log file {}: {e}", p.display()); file = None; current_path = None; last_offset = 0; pending_line.clear(); }
					}
				} else {
					debug_log!("[watcher] no log file found in {}", vrchat_dir.display());
					file = None; current_path = None; last_offset = 0; pending_line.clear();
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
						let n = match f.read(&mut buf[..to_read]) { Ok(n) => n, Err(e) => { eprintln!("[watcher] read error: {e}"); break; } };
						if n == 0 { break; }
						last_offset += n as u64;
						remaining -= n;
						let chunk = String::from_utf8_lossy(&buf[..n]);
						pending_line.push_str(&chunk);
						// Only process full lines; keep the remainder in pending_line
						if let Some(nl_idx) = pending_line.rfind('\n') {
							let to_process = pending_line[..nl_idx].to_string();
							let remainder = pending_line[nl_idx + 1 ..].to_string();
							pending_line = remainder;
							for raw in to_process.split('\n') {
								let line = raw.trim_end_matches('\r');
								if line.is_empty() { continue; }
								let ts_cap = re_ts.captures(line).and_then(|c| c.get(1)).map(|m| m.as_str());
								let ts = match ts_cap { Some(t) => t, None => continue };
								// Purge triggers: end-of-session markers or explicit VRChat logs
								if re_purge1.is_match(line) || re_purge2.is_match(line) || re_purge3.is_match(line) {
									if let Err(e) = super::db::db_purge_all(&app, ts, true) { eprintln!("[watcher] failed to purge all: {e:?}"); }
									continue;
								}
								// Instance change: close previous, emit a system row and UI event
								if let Some(caps) = re_joining.captures(line) {
									if let Err(e) = super::db::db_purge_all(&app, ts, true) { eprintln!("[watcher] failed to purge all on instance change: {e:?}"); }
									let world_id = caps.get(1).map(|m| m.as_str()).unwrap_or("");
									let instance_id = caps.get(2).map(|m| m.as_str()).unwrap_or("");
									let region = caps.get(3).map(|m| m.as_str());
									let _ = app.emit("instance_changed", serde_json::json!({ "worldId": world_id, "instanceId": instance_id, "region": region, "ts": ts }));
									let msg = match region { Some(r) => format!("Joining: {} | Instance: {} | Region: {}", world_id, instance_id, r), None => format!("Joining: {} | Instance: {}", world_id, instance_id) };
									let _ = super::db::db_insert_system_event(&app, ts, "instance_changed", Some(&msg), Some(world_id), Some(instance_id), region, true);
									continue;
								}
								// Player joined: insert row, cache username, maybe notify
								if let Some(caps) = re_join.captures(line) {
									let username = caps.get(1).map(|m| m.as_str().trim()).unwrap_or("");
									let uid = caps.get(2).map(|m| m.as_str()).unwrap_or("");
									if !uid.is_empty() {
										if let Err(e) = super::db::db_insert_join(&app, ts, uid, username, true) { eprintln!("[watcher] failed to insert join: {e:?}"); }
										let mut all = super::notes::load_all_notes();
										let existing = all.usernames.get(uid).cloned();
										let mut changed = false;
										if username.is_empty() {
											if existing.is_none() {
												all.usernames.insert(uid.to_string(), "Not Yet Recorded".to_string());
												changed = true;
											}
										} else {
											if existing.as_deref().unwrap_or("") != username {
												all.usernames.insert(uid.to_string(), username.to_string());
												changed = true;
											}
										}
										if changed { let _ = super::notes::save_all_notes(&all); }
										if all.watchlist.get(uid).copied().unwrap_or(false) {
															if all.watchlist.get(uid).copied().unwrap_or(false) {
																#[cfg(target_os = "windows")]
																{
																	let msg = format!("{} has joined", username);
																	let _ = winrt_notification::Toast::new("FCH").title("- FCH Notifier -").text1(&msg).show();
																}
																let _ = super::config::preview_sound();
															}
														}
									}
									continue;
								}
								// Player left: update the most recent open join for that user
								if let Some(caps) = re_left.captures(line) {
									let uid = caps.get(2).map(|m| m.as_str()).unwrap_or("");
									if !uid.is_empty() { if let Err(e) = super::db::db_update_leave(&app, ts, uid, true) { eprintln!("[watcher] failed to update leave: {e:?}"); } }
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
	let local_low = std::env::var("LOCALAPPDATA").ok().and_then(|p| PathBuf::from(p).parent().map(|pp| pp.to_path_buf())).unwrap_or_else(|| PathBuf::from("C:/Users/Public"));
	local_low.join("LocalLow").join("VRChat").join("VRChat")
}

// Pick the newest output_log_*.txt by modification time
fn find_latest_log_file(dir: &PathBuf) -> Option<PathBuf> {
	let mut entries: Vec<(std::time::SystemTime, PathBuf)> = fs::read_dir(dir).ok()?
		.filter_map(|e| e.ok())
		.filter_map(|e| {
			let p = e.path();
			let name = p.file_name()?.to_string_lossy().into_owned();
			if name.starts_with("output_log_") && name.ends_with(".txt") {
				let mt = e.metadata().ok()?.modified().ok()?;
				Some((mt, p))
			} else { None }
		})
		.collect();
	entries.sort_by_key(|(mt, _)| std::cmp::Reverse(*mt));
	entries.into_iter().map(|(_, p)| p).next()
}
