use crate::config::{load_config, play_configured_sound, play_custom_sound, AppConfig};
use rfd::FileDialog;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
#[tauri::command]
pub fn browse_sound() -> Result<serde_json::Value, String> {
    let file = FileDialog::new()
        .add_filter("Audio", &["wav", "mp3"])
        .pick_file();
    Ok(serde_json::json!({ "path": file.map(|p| p.to_string_lossy().to_string()) }))
}

static PLAYING: OnceLock<AtomicBool> = OnceLock::new();

fn try_begin_play() -> bool {
    let flag = PLAYING.get_or_init(|| AtomicBool::new(false));
    !flag.swap(true, Ordering::SeqCst)
}

fn end_play() {
    if let Some(flag) = PLAYING.get() {
        flag.store(false, Ordering::SeqCst);
    }
}

#[tauri::command]
pub fn preview_watch_sound() -> Result<(), String> {
    preview_sound(false)
}

#[tauri::command]
pub fn preview_group_sound() -> Result<(), String> {
    preview_sound(true)
}

pub fn play_watch_sound() {
    if let Some(cfg) = prepared_config() {
        spawn_playback(cfg, false);
    }
}

pub fn play_group_sound() {
    if let Some(cfg) = prepared_config() {
        spawn_playback(cfg, true);
    }
}

pub fn play_user_sound(path: &str, volume: f32) {
    if !try_begin_play() {
        return;
    }
    let path = path.to_owned();
    let volume = volume.clamp(0.0, 1.0);
    let _ = std::thread::spawn(move || {
        let _ = play_custom_sound(&path, volume);
        end_play();
    });
}

fn preview_sound(group: bool) -> Result<(), String> {
    if !try_begin_play() {
        return Ok(());
    }
    let cfg = load_config();
    spawn_playback(cfg, group);
    Ok(())
}

fn spawn_playback(cfg: AppConfig, group: bool) {
    let _ = std::thread::spawn(move || {
        play_configured_sound(&cfg, group);
        end_play();
    });
}

fn prepared_config() -> Option<AppConfig> {
    if !try_begin_play() {
        return None;
    }
    Some(load_config())
}
