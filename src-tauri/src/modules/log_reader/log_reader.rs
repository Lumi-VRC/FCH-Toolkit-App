// Log Reader: Monitors VRChat log files for changes and reads new lines incrementally
// Mimics the VRCX LogWatcher architecture using polling instead of FileSystemWatcher

use std::{
    collections::HashMap,
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use crate::modules::log_reader::log_parser;
use tauri::AppHandle;

// Context for tracking state of each log file
struct LogContext {
    position: u64,  // Last read byte position
    length: u64,    // Current file size
}

// Main log reader struct
pub struct LogReader {
    log_directory: PathBuf,
    log_contexts: Arc<Mutex<HashMap<String, LogContext>>>,
    thread_handle: Option<thread::JoinHandle<()>>,
    active: Arc<Mutex<bool>>,
    app_handle: Option<AppHandle>,
}

impl LogReader {
    // Step 1: Initialization
    pub fn new(app_handle: AppHandle) -> Self {
        let log_dir = default_vrchat_log_dir();
        
        Self {
            log_directory: log_dir,
            log_contexts: Arc::new(Mutex::new(HashMap::new())),
            thread_handle: None,
            active: Arc::new(Mutex::new(false)),
            app_handle: Some(app_handle),
        }
    }

    // Start the background thread that polls for log changes
    pub fn start(&mut self) {
        let contexts = Arc::clone(&self.log_contexts);
        let directory = self.log_directory.clone();
        let active = Arc::clone(&self.active);
        let app_handle = self.app_handle.clone().expect("AppHandle not set");
        
        *active.lock().unwrap() = true;
        
        let handle = thread::spawn(move || {
            // Step 2 & 3: File Discovery and Incremental Reading loop
            while *active.lock().unwrap() {
                if let Err(e) = update_logs(&directory, &contexts, &app_handle) {
                    crate::debug_eprintln!("[log_reader] Error updating logs: {}", e);
                }
                
                // Poll every second (mimics VRCX's 1 second polling)
                thread::sleep(Duration::from_secs(1));
            }
        });
        
        self.thread_handle = Some(handle);
    }

    pub fn stop(&mut self) {
        *self.active.lock().unwrap() = false;
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}

// Global log reader instance
lazy_static::lazy_static! {
    static ref LOG_READER: Mutex<Option<LogReader>> = Mutex::new(None);
    static ref MOST_RECENT_LOG_FILE: Mutex<Option<String>> = Mutex::new(None);
}

// Tauri command to start the log reader
#[tauri::command]
pub fn start_log_reader(app_handle: tauri::AppHandle) -> Result<(), String> {
    let mut reader_guard = LOG_READER.lock().map_err(|e| e.to_string())?;
    
    if reader_guard.is_some() {
        return Err("Log reader already started".to_string());
    }
    
    let mut reader = LogReader::new(app_handle);
    reader.start();
    *reader_guard = Some(reader);
    
    Ok(())
}

// Tauri command to stop the log reader
#[tauri::command]
pub fn stop_log_reader() -> Result<(), String> {
    let mut reader_guard = LOG_READER.lock().map_err(|e| e.to_string())?;
    
    if let Some(mut reader) = reader_guard.take() {
        reader.stop();
    }
    
    Ok(())
}

// Tauri command to get the most recently updated log file path
#[tauri::command]
pub fn get_most_recent_log_file() -> Result<Option<String>, String> {
    let reader_guard = LOG_READER.lock().map_err(|e| e.to_string())?;
    
    if let Some(reader) = reader_guard.as_ref() {
        let recent_file = MOST_RECENT_LOG_FILE.lock().map_err(|e| e.to_string())?;
        if let Some(file_name) = recent_file.as_ref() {
            let full_path = reader.log_directory.join(file_name);
            return Ok(Some(full_path.to_string_lossy().to_string()));
        }
    }
    
    Ok(None)
}

// Tauri command to open the most recently updated log file
#[tauri::command]
pub fn open_most_recent_log_file() -> Result<String, String> {
    let reader_guard = LOG_READER.lock().map_err(|e| e.to_string())?;
    
    if let Some(reader) = reader_guard.as_ref() {
        let recent_file = MOST_RECENT_LOG_FILE.lock().map_err(|e| e.to_string())?;
        if let Some(file_name) = recent_file.as_ref() {
            let full_path = reader.log_directory.join(file_name);
            let path_str = full_path.to_string_lossy().to_string();
            
            // Verify file exists
            if !full_path.exists() {
                return Err(format!("Log file does not exist: {}", path_str));
            }
            
            // Open file with system default application
            #[cfg(target_os = "windows")]
            {
                // Use explorer.exe to open the file (more reliable than cmd start)
                let result = std::process::Command::new("explorer.exe")
                    .arg(&path_str)
                    .spawn();
                
                match result {
                    Ok(_) => Ok(format!("Opened: {}", path_str)),
                    Err(e) => {
                        crate::debug_eprintln!("Failed to open file with explorer.exe: {}", e);
                        // Fallback to cmd start
                        let fallback = std::process::Command::new("cmd")
                            .args(["/C", "start", "", &path_str])
                            .spawn();
                        match fallback {
                            Ok(_) => Ok(format!("Opened (fallback): {}", path_str)),
                            Err(e2) => Err(format!("Failed to open file: {} - Error: {} (fallback: {})", path_str, e, e2)),
                        }
                    }
                }
            }
            
            #[cfg(target_os = "macos")]
            {
                let result = std::process::Command::new("open")
                    .arg(&full_path)
                    .spawn();
                
                match result {
                    Ok(_) => Ok(format!("Opened: {}", path_str)),
                    Err(e) => Err(format!("Failed to open file: {} - Error: {}", path_str, e)),
                }
            }
            
            #[cfg(target_os = "linux")]
            {
                let result = std::process::Command::new("xdg-open")
                    .arg(&full_path)
                    .spawn();
                
                match result {
                    Ok(_) => Ok(format!("Opened: {}", path_str)),
                    Err(e) => Err(format!("Failed to open file: {} - Error: {}", path_str, e)),
                }
            }
        } else {
            Err("No log file has been updated yet".to_string())
        }
    } else {
        Err("Log reader is not running".to_string())
    }
}

// Step 2: File Discovery - Find and track all output_log_*.txt files
fn update_logs(
    log_dir: &PathBuf,
    contexts: &Arc<Mutex<HashMap<String, LogContext>>>,
    app_handle: &AppHandle,
) -> Result<(), Box<dyn std::error::Error>> {
    // Refresh directory to get latest files
    let entries = match fs::read_dir(log_dir) {
        Ok(e) => e,
        Err(_) => return Ok(()), // Directory doesn't exist yet, skip
    };

    let mut file_infos: Vec<(String, PathBuf, u64)> = Vec::new();
    
    // Collect all matching log files
    for entry in entries.flatten() {
        let path = entry.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(s) => s,
            None => continue,
        };
        
        // Match pattern: output_log_*.txt
        if !name.starts_with("output_log_") || !name.ends_with(".txt") {
            continue;
        }
        
        // Get file metadata
        // Note: We track ALL active log files, but skip historical content when reading
        // (VRCX filters log LINES by date, not files themselves)
        if let Ok(metadata) = entry.metadata() {
            let length = metadata.len();
            let path_clone = path.clone(); // Clone path before moving
            file_infos.push((name.to_string(), path_clone, length));
        }
    }
    
    // Sort by modification time (oldest first) - similar to VRCX
    file_infos.sort_by_key(|(_, path, _)| {
        fs::metadata(path)
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
    });
    
    // Collect files that need reading (while holding lock briefly)
    let mut files_to_read: Vec<(String, PathBuf)> = Vec::new();
    
    {
        let mut contexts_guard = contexts.lock().unwrap();
        
        // Process each file
        for (name, path, current_length) in file_infos {
            // Check if we already have context for this file
            let needs_reading = if let Some(context) = contexts_guard.get_mut(&name) {
                // File exists in our tracking - check if it grew
                if context.length != current_length {
                    // File has new content, update length
                    context.length = current_length;
                    // Track most recently updated file
                    *MOST_RECENT_LOG_FILE.lock().unwrap() = Some(name.clone());
                    true
                } else {
                    false
                }
            } else {
                // New file discovered - skip historical data, only read NEW lines going forward
                // Set position to current file length to skip all existing content (like VRCX does)
                contexts_guard.insert(
                    name.clone(),
                    LogContext {
                        position: current_length,  // Start at end of file - skip historical logs
                        length: current_length,
                    },
                );
                // Don't read historical data on first discovery - only new lines after this point
                false
            };
            
            if needs_reading {
                files_to_read.push((name, path));
            }
        }
    } // Release mutex lock here
    
    // Now process files that need reading (without holding the lock)
    for (name, path) in files_to_read {
        // Step 3: Incremental Reading - Read new lines from this file
        if let Err(e) = parse_log_file(&path, &name, contexts, app_handle) {
            crate::debug_eprintln!("[log_reader] Error parsing log file {}: {}", name, e);
        }
    }
    
    Ok(())
}

// Step 3: Incremental Reading - Read from last position to end of file
fn parse_log_file(
    file_path: &PathBuf,
    file_name: &str,
    contexts: &Arc<Mutex<HashMap<String, LogContext>>>,
    app_handle: &AppHandle,
) -> Result<(), Box<dyn std::error::Error>> {
    // Get start position and expected file size
    let start_position = {
        let contexts_guard = contexts.lock().unwrap();
        let context = contexts_guard.get(file_name)
            .ok_or("Context not found")?;
        context.position
    };
    
    // Open file with read sharing (allows VRChat to write while we read)
    let mut file = File::open(file_path)?;
    
    // Get actual file size
    let actual_file_size = file.metadata()?.len();
    
    // If we're already at or past the end, nothing to read
    if start_position >= actual_file_size {
        return Ok(());
    }
    
    // Seek to last known position
    file.seek(SeekFrom::Start(start_position))?;
    
    // Read raw bytes to accurately track position and handle incomplete lines
    let mut buffer = vec![0u8; 65536]; // 64KB buffer
    let mut line_count = 0;
    let mut current_file_position = start_position;
    let mut pending_data = Vec::new(); // Data from previous read that didn't end with newline
    let mut pending_start_position = start_position; // File position where pending_data starts
    const MAX_LINES_PER_BATCH: usize = 1000;  // Limit lines per batch to prevent UI blocking
    const MAX_TOTAL_LINES_PER_POLL: usize = 10000;  // Absolute limit per poll cycle to prevent getting stuck
    
    loop {
        // Safety check: Don't process more than MAX_TOTAL_LINES_PER_POLL in one poll cycle
        if line_count >= MAX_TOTAL_LINES_PER_POLL {
            crate::debug_eprintln!("[log_reader] Hit max lines limit ({}) for file {}, will continue next poll", MAX_TOTAL_LINES_PER_POLL, file_name);
            break;
        }
        
        // Check if we've caught up to the end of the file
        if current_file_position >= actual_file_size {
            break;
        }
        
        // If we have pending data, we need to seek back to where it starts
        if !pending_data.is_empty() {
            file.seek(SeekFrom::Start(pending_start_position))?;
        }
        
        // Read a chunk of data
        let bytes_read = match file.read(&mut buffer) {
            Ok(0) => {
                // EOF - if we have pending data, it's an incomplete line, don't process it
                break;
            }
            Ok(n) => n,
            Err(e) => return Err(Box::new(e)),
        };
        
        // Combine pending data with new data
        let mut data_to_process = std::mem::take(&mut pending_data);
        let data_start_position = pending_start_position;
        data_to_process.extend_from_slice(&buffer[..bytes_read]);
        
        // Find the position of the last complete line (ending with \n)
        // Search backwards for the last newline
        let last_complete_line_end = data_to_process.iter().rposition(|&b| b == b'\n')
            .map(|pos| pos + 1); // +1 to include the \n
        
        if let Some(complete_end) = last_complete_line_end {
            // We have at least one complete line
            let complete_data = &data_to_process[..complete_end];
            
            // Split into individual lines and process them
            let mut batch_line_count = 0;
            let mut processed_bytes = 0;
            
            for line_bytes in complete_data.split_inclusive(|&b| b == b'\n') {
                // Check if we've hit the batch limit before processing this line
                if batch_line_count >= MAX_LINES_PER_BATCH {
                    // We've hit the batch limit - save remaining data and continue next iteration
                    let remaining_data_start = processed_bytes;
                    pending_data = data_to_process[remaining_data_start..].to_vec();
                    current_file_position = data_start_position + processed_bytes as u64;
                    pending_start_position = current_file_position;
                    file.seek(SeekFrom::Start(current_file_position))?;
                    // Break inner loop, continue outer loop to process next batch
                    break;
                }
                
                // Convert to string, handling potential UTF-8 errors
                if let Ok(line_str) = std::str::from_utf8(line_bytes) {
                    let trimmed = line_str.trim_end_matches(&['\r', '\n'][..]);
                    if !trimmed.is_empty() {
                        log_parser::emit_log_line(app_handle, trimmed, file_name);
                        line_count += 1;
                        batch_line_count += 1;
                    }
                }
                
                // Track how many bytes we've processed
                processed_bytes += line_bytes.len();
            }
            
            // If we processed all lines in this chunk (didn't hit batch limit)
            if batch_line_count < MAX_LINES_PER_BATCH {
                // Calculate the actual file position after processing complete lines
                current_file_position = data_start_position + complete_end as u64;
                
                // Save any remaining incomplete data
                if complete_end < data_to_process.len() {
                    pending_data = data_to_process[complete_end..].to_vec();
                    pending_start_position = current_file_position;
                } else {
                    // All data was complete, clear pending
                    pending_start_position = current_file_position;
                }
                
                // Seek to the position after processed bytes for next iteration
                file.seek(SeekFrom::Start(current_file_position))?;
            }
            // If we hit batch limit, we already updated position and seeked above, so continue loop
        } else {
            // No complete lines found - this means the data doesn't contain a complete line yet
            // Don't advance position - we'll re-read this data next time when more data is available
            file.seek(SeekFrom::Start(data_start_position))?;
            current_file_position = data_start_position;
            break;
        }
    }
    
    // Update context with the position of the last complete line
    {
        let mut contexts_guard = contexts.lock().unwrap();
        let context = contexts_guard.get_mut(file_name)
            .ok_or("Context not found")?;
        context.position = current_file_position;
    }
    
    Ok(())
}

// Get VRChat log directory (Windows: %LOCALAPPDATA%\..\LocalLow\VRChat\VRChat)
fn default_vrchat_log_dir() -> PathBuf {
    let local_low = std::env::var("LOCALAPPDATA")
        .ok()
        .and_then(|p| PathBuf::from(p).parent().map(|pp| pp.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("C:/Users/Public"));
    local_low.join("LocalLow").join("VRChat").join("VRChat")
}
