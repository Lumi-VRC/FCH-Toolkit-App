// Settings: Default sound paths and volume sliders
//
// This module persists application settings (default sound paths and volume levels)
// in a JSON file under the app's data folder.

use std::{fs, path::PathBuf};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct AppSettings {
    #[serde(default = "default_master_volume")]
    pub master_volume: f64, // 0.0 to 1.0
    
    #[serde(default)]
    pub group_notifications: NotificationSettings,
    
    #[serde(default)]
    pub local_notifications: NotificationSettings,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct NotificationSettings {
    #[serde(default)]
    pub default_sound_path: Option<String>, // Path to default sound file
    
    #[serde(default = "default_notification_volume")]
    pub volume: f64, // 0.0 to 1.0
}

fn default_master_volume() -> f64 {
    1.0
}

fn default_notification_volume() -> f64 {
    0.8
}

/// Get the directory where settings are stored
fn settings_dir() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:/Users/Public"));
    base.join("FCHClient")
}

/// Get the path to the settings.json file
fn settings_path() -> PathBuf {
    settings_dir().join("settings.json")
}

/// Load settings from disk
fn load_settings() -> AppSettings {
    let path = settings_path();
    if !path.exists() {
        return AppSettings::default();
    }
    
    match fs::read_to_string(&path) {
        Ok(content) => {
            match serde_json::from_str::<AppSettings>(&content) {
                Ok(settings) => settings,
                Err(e) => {
                    crate::debug_eprintln!("[settings] Failed to parse settings.json: {}", e);
                    AppSettings::default()
                }
            }
        }
        Err(e) => {
            crate::debug_eprintln!("[settings] Failed to read settings.json: {}", e);
            AppSettings::default()
        }
    }
}

/// Save settings to disk
fn save_settings(settings: &AppSettings) -> Result<(), String> {
    let path = settings_path();
    
    // Ensure directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create settings directory: {}", e))?;
    }
    
    let json = serde_json::to_string_pretty(settings)
        .map_err(|e| format!("Failed to serialize settings: {}", e))?;
    
    fs::write(&path, json)
        .map_err(|e| format!("Failed to write settings.json: {}", e))?;
    
    Ok(())
}

/// Initialize settings (create default file if needed)
pub fn init_settings() -> Result<(), String> {
    let path = settings_path();
    if !path.exists() {
        let default = AppSettings::default();
        save_settings(&default)?;
    }
    Ok(())
}

/// Get all settings
#[tauri::command]
pub fn get_settings() -> Result<AppSettings, String> {
    Ok(load_settings())
}

/// Set master volume
#[tauri::command]
pub fn set_master_volume(volume: f64) -> Result<(), String> {
    let volume = volume.max(0.0).min(1.0);
    let mut settings = load_settings();
    settings.master_volume = volume;
    save_settings(&settings)
}

/// Set group notification settings
#[tauri::command]
pub fn set_group_notification_settings(
    default_sound_path: Option<String>,
    volume: f64,
) -> Result<(), String> {
    let volume = volume.max(0.0).min(1.0);
    let mut settings = load_settings();
    settings.group_notifications.default_sound_path = default_sound_path.filter(|s| !s.trim().is_empty());
    settings.group_notifications.volume = volume;
    save_settings(&settings)
}

/// Set local notification settings
#[tauri::command]
pub fn set_local_notification_settings(
    default_sound_path: Option<String>,
    volume: f64,
) -> Result<(), String> {
    let volume = volume.max(0.0).min(1.0);
    let mut settings = load_settings();
    settings.local_notifications.default_sound_path = default_sound_path.filter(|s| !s.trim().is_empty());
    settings.local_notifications.volume = volume;
    save_settings(&settings)
}
