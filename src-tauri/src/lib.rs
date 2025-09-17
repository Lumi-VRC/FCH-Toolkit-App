// The backend is split into small modules to keep things readable:
// - notes: simple JSON-based notes/watchlist storage and Tauri commands
// - config: app configuration (notification sound path/volume) and helpers
// - db: SQLite helpers and Tauri commands for join logs
// - watcher: VRChat log tailer and real-time event streaming
mod notes;
mod config;
mod db;
mod watcher;

use std::sync::{Arc, Mutex};
use tauri::Manager; // brings .app_handle() and other manager helpers into scope

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
			// Notes/watchlist operations
			notes::add_note,
			notes::get_notes,
			notes::get_note,
			notes::get_all_notes,
			notes::delete_user,
			notes::set_watch,
			notes::get_watch,
			notes::set_username,
			// App configuration & audio preview
			config::get_config,
			config::set_config,
			config::browse_sound,
			config::preview_sound,
			// SQLite-backed join logs
			db::get_join_logs_page,
			db::get_active_join_logs,
			db::purge_join_log_table,
			db::get_latest_username_for_user,
		])
		// Start the runtime with settings resolved from tauri.conf.json
		.run(tauri::generate_context!())
		.expect("error while running tauri application");
}
