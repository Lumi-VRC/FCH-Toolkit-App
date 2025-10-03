use tauri::{AppHandle, Emitter};

pub fn emit_debug(app: &AppHandle, message: impl Into<String>) {
    let msg = message.into();
    if msg.is_empty() {
        return;
    }
    let _ = app.emit(
        "debug_log",
        serde_json::json!({
            "message": msg,
            "ts": chrono::Local::now().to_rfc3339(),
        }),
    );
}
