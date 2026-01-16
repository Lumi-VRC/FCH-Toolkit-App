// Debug log helper - emits debug messages to frontend via Tauri events
// Use this instead of println!/eprintln! to send logs to the debug panel

use tauri::AppHandle;
use tauri::Emitter;

/// Emit a debug log message to the frontend
pub fn emit_debug_log(app_handle: &AppHandle, message: &str, level: &str) {
    let _ = app_handle.emit("debug_log", serde_json::json!({
        "message": message,
        "ts": chrono::Utc::now().to_rfc3339(),
        "level": level,
        "source": "backend"
    }));
}

/// Convenience macros for different log levels
#[macro_export]
macro_rules! debug_log {
    ($app:expr, $($arg:tt)*) => {
        $crate::modules::debug::debug_log::emit_debug_log($app, &format!($($arg)*), "log");
    };
}

#[macro_export]
macro_rules! debug_info {
    ($app:expr, $($arg:tt)*) => {
        $crate::modules::debug::debug_log::emit_debug_log($app, &format!($($arg)*), "info");
    };
}

#[macro_export]
macro_rules! debug_warn {
    ($app:expr, $($arg:tt)*) => {
        $crate::modules::debug::debug_log::emit_debug_log($app, &format!($($arg)*), "warn");
    };
}

#[macro_export]
macro_rules! debug_error {
    ($app:expr, $($arg:tt)*) => {
        $crate::modules::debug::debug_log::emit_debug_log($app, &format!($($arg)*), "error");
    };
}
