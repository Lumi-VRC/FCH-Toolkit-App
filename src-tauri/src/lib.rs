// The backend is split into small modules to keep things readable:
// - notes: simple JSON-based notes/watchlist storage and Tauri commands
// - config: app configuration (notification sound path/volume) and helpers
// - db: SQLite helpers and Tauri commands for join logs
// - watcher: VRChat log tailer and real-time event streaming
mod config;
mod db;
mod debug;
mod notes;
mod sound;
mod watcher;

use std::sync::{Arc, Mutex};

#[tauri::command]
fn greet(name: &str) -> String {
    // Simple example Tauri command used by the template front-end.
    // Tauri exposes commands to JS via 'invoke("greet", { name })'.
    // We keep it as a sanity check that the Rust<->JS bridge works.
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Build the Tauri application.
    tauri::Builder::default()
        // Shared search-cancellation token for the log search command.
        // Using Arc<Mutex<..>> keeps it thread-safe across async tasks.
        .manage(watcher::SearchState(Arc::new(Mutex::new(0))))
        .setup(|_app| {
            if let Err(err) = crate::db::db_init() {
                eprintln!("failed to initialize database: {err:?}");
            }
            Ok(())
        })
        // Small helper plugin that opens URLs/files using the OS.
        .plugin(tauri_plugin_opener::init())
        // Register Rust commands callable from the Svelte front-end via invoke(...)
        .invoke_handler(tauri::generate_handler![
            // Example/template
            greet,
            // Watcher & log reading
            watcher::start_log_watcher,
            watcher::read_log_info,
            watcher::read_log_chunk,
            watcher::search_log_file,
            // Tool authentication support
            watcher::get_tool_authentication_lines,
            // Notes/watchlist operations
            notes::add_note,
            notes::get_notes,
            notes::get_note,
            notes::get_all_notes,
            notes::delete_user,
            notes::set_watch,
            notes::get_watch,
            notes::set_username,
            notes::set_user_sound,
            notes::get_user_sound,
            // App configuration & audio preview
            config::get_config,
            config::set_config,
            sound::browse_sound,
            sound::preview_watch_sound,
            sound::preview_group_sound,
            // SQLite-backed join logs
            db::get_join_logs_page,
            db::get_active_join_logs,
            db::purge_join_log_table,
            db::get_latest_username_for_user,
            db::db_get_avatar_details,
            db::get_latest_avatar_for_user,
            db::insert_avatar_details,
            db::list_recent_avatar_details,
            db::list_distinct_avatar_details,
            db::list_recent_avatar_logs,
            // Startup maintenance and persistence
            db::dedupe_open_joins,
            db::set_group_watchlisted_for_users,
            // Access token storage
            db::add_group_access_token,
            db::list_group_access_tokens,
            db::remove_group_access_token,
        ])
        // Start the runtime with settings resolved from tauri.conf.json
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
// If you're reading this... hiii!
