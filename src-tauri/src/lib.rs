// Main library file that binds all Rust modules together
//
// This file serves as the central entry point for all backend functionality.
// Modules will be added here as they are implemented.

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

// Macro to conditionally print only in debug builds
// Export these macros so they're available to all modules
#[macro_export]
#[cfg(debug_assertions)]
macro_rules! debug_println {
    ($($arg:tt)*) => {
        println!($($arg)*);
    };
}

#[macro_export]
#[cfg(not(debug_assertions))]
macro_rules! debug_println {
    ($($arg:tt)*) => {
        // No-op in release builds
    };
}

#[macro_export]
#[cfg(debug_assertions)]
macro_rules! debug_eprintln {
    ($($arg:tt)*) => {
        eprintln!($($arg)*);
    };
}

#[macro_export]
#[cfg(not(debug_assertions))]
macro_rules! debug_eprintln {
    ($($arg:tt)*) => {
        // No-op in release builds
    };
}

// Module declarations - uncomment as modules are added
mod modules;
// mod db;
// mod notes;
// mod config;
// mod watcher;
// mod debug;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        // Register Tauri commands here as modules are added
            .invoke_handler(tauri::generate_handler![
                crate::modules::log_reader::log_reader::start_log_reader,
                crate::modules::log_reader::log_reader::stop_log_reader,
                crate::modules::log_reader::log_reader::get_most_recent_log_file,
                crate::modules::log_reader::log_reader::open_most_recent_log_file,
                crate::modules::log_reader::log_reader::open_most_recent_log_folder,
                crate::modules::log_reader::log_parser::manual_refresh_scan,
                crate::modules::log_reader::log_parser::get_current_location,
                crate::modules::log_reader::log_parser::get_instance_history,
                crate::modules::local_db::localdb::add_note,
                crate::modules::local_db::localdb::get_note,
                crate::modules::local_db::localdb::get_all_notes,
                crate::modules::local_db::localdb::delete_user,
                crate::modules::local_db::localdb::set_watch,
                crate::modules::local_db::localdb::get_watch,
                crate::modules::local_db::localdb::set_user_sound,
                crate::modules::local_db::localdb::get_user_sound,
                crate::modules::local_db::localdb::set_username,
                crate::modules::local_db::localdb::browse_sound,
                crate::modules::world_mod::world_mod_logs::add_ban_log_entry,
                crate::modules::world_mod::world_mod_logs::get_all_ban_log_entries,
                crate::modules::world_mod::world_mod_logs::search_ban_log_entries,
                crate::modules::group_auth::group_access_tokens::add_group_access_token,
                crate::modules::group_auth::group_access_tokens::list_group_access_tokens,
                crate::modules::group_auth::group_access_tokens::remove_group_access_token,
                crate::modules::instance_monitor::batcher::add_user_to_batch_command,
                crate::modules::instance_monitor::batcher::flush_user_batch,
                crate::modules::settings::settings::get_settings,
                crate::modules::settings::settings::set_master_volume,
                crate::modules::settings::settings::set_group_notification_settings,
                crate::modules::settings::settings::set_local_notification_settings,
                crate::modules::sound::sound::play_user_notification_sound,
                crate::modules::sound::sound::preview_group_notification_sound,
                crate::modules::sound::sound::preview_local_notification_sound,
                crate::modules::updater::updater::check_for_update,
                crate::modules::updater::updater::download_update,
                crate::modules::updater::updater::run_installer,
                crate::modules::updater::updater::download_and_install_update,
            ])
        .setup(|app| {
            // Initialize modules here
            // Initialize notes database - create file if it doesn't exist
            if let Err(err) = crate::modules::local_db::localdb::init_notes_db() {
                crate::debug_eprintln!("failed to initialize notes database: {err:?}");
            }
            // Initialize world mod logs database - create file if it doesn't exist
            if let Err(err) = crate::modules::world_mod::world_mod_logs::init_db() {
                crate::debug_eprintln!("failed to initialize world mod logs database: {err:?}");
            }
            // Initialize group access tokens database - create file if it doesn't exist
            if let Err(err) = crate::modules::group_auth::group_access_tokens::init_db() {
                crate::debug_eprintln!("failed to initialize group access tokens database: {err:?}");
            }
            // Initialize group watchlist batcher
            if let Err(err) = crate::modules::instance_monitor::batcher::init_batcher(app.handle().clone()) {
                crate::debug_eprintln!("failed to initialize group watchlist batcher: {err:?}");
            }
            // Initialize settings
            if let Err(err) = crate::modules::settings::settings::init_settings() {
                crate::debug_eprintln!("failed to initialize settings: {err:?}");
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
