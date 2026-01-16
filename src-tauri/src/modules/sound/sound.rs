// Sound: Play notification sounds with priority logic
//
// Priority order:
// 1. Custom sound (from local_db for specific user)
// 2. Group default sound (from settings)
// 3. Local default sound (from settings)
// 4. Windows system sound fallback (SystemExclamation for group, SystemAsterisk for local)
// 5. None (no sound played)

use crate::modules::local_db::localdb;
use crate::modules::settings::settings;

/// Play sound for a user based on notification type and priority
#[tauri::command]
pub fn play_user_notification_sound(
    user_id: String,
    has_group_notifications: bool,
    has_local_notifications: bool,
) -> Result<(), String> {
    // Load settings to get volumes and default sounds
    let settings = settings::get_settings().map_err(|e| format!("Failed to load settings: {}", e))?;
    
    // Determine which notification type to use
    let (sound_path, volume, is_group) = if has_group_notifications {
        // Group notifications take priority
        let path = get_sound_path_for_user(&user_id, &settings.group_notifications.default_sound_path)?;
        let vol = settings.master_volume * settings.group_notifications.volume;
        (path, vol, true)
    } else if has_local_notifications {
        // Fall back to local notifications
        let path = get_sound_path_for_user(&user_id, &settings.local_notifications.default_sound_path)?;
        let vol = settings.master_volume * settings.local_notifications.volume;
        (path, vol, false)
    } else {
        // No notifications, don't play sound
        return Ok(());
    };
    
    // If no sound path found, try Windows system sound fallback
    let Some(sound_path) = sound_path else {
        // Fall back to Windows system sounds
        play_windows_system_sound(is_group);
        return Ok(());
    };
    
    // Play the sound (spawn async task to avoid blocking)
    let sound_path_clone = sound_path.clone();
    let is_group_clone = is_group;
    tauri::async_runtime::spawn(async move {
        if let Err(e) = play_sound_file(&sound_path_clone, volume).await {
            crate::debug_eprintln!("[sound] Failed to play sound: {}", e);
            // If custom sound fails, fall back to system sound
            play_windows_system_sound(is_group_clone);
        }
    });
    
    Ok(())
}

/// Get sound path for a user following priority: custom -> default
fn get_sound_path_for_user(
    user_id: &str,
    default_sound: &Option<String>,
) -> Result<Option<String>, String> {
    // First, check for custom sound in local_db
    match localdb::get_user_sound(user_id.to_string()) {
        Ok(value) => {
            // value is serde_json::Value, check for soundPath field
            if let Some(obj) = value.as_object() {
                if let Some(sound_path_val) = obj.get("soundPath") {
                    if let Some(sound_path) = sound_path_val.as_str() {
                        if !sound_path.is_empty() {
                            return Ok(Some(sound_path.to_string()));
                        }
                    }
                }
            }
        }
        Err(_) => {
            // User might not have a custom sound, continue to default
        }
    }
    
    // Fall back to default sound
    Ok(default_sound.clone())
}

/// Play Windows system sound as fallback
fn play_windows_system_sound(is_group: bool) {
    #[cfg(target_os = "windows")]
    {
        unsafe {
            use windows::core::w;
            use windows::Win32::Media::Audio::{PlaySoundW, SND_ALIAS, SND_ASYNC};
            let alias = if is_group {
                w!("SystemExclamation")  // For group watchlist notifications
            } else {
                w!("SystemAsterisk")     // For local watchlist notifications
            };
            let _ = PlaySoundW(alias, None, SND_ALIAS | SND_ASYNC);
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        // No-op on non-Windows platforms
        let _ = is_group;
    }
}

/// Preview group notification sound (for testing in settings)
#[tauri::command]
pub fn preview_group_notification_sound() -> Result<(), String> {
    let settings = settings::get_settings().map_err(|e| format!("Failed to load settings: {}", e))?;
    
    let sound_path = settings.group_notifications.default_sound_path.clone();
    let volume = settings.master_volume * settings.group_notifications.volume;
    
    if let Some(path) = sound_path {
        let path_clone = path.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(_) = play_sound_file(&path_clone, volume).await {
                // If custom sound fails, fall back to system sound
                play_windows_system_sound(true);
            }
        });
    } else {
        // No custom sound, use Windows system sound
        play_windows_system_sound(true);
    }
    
    Ok(())
}

/// Preview local notification sound (for testing in settings)
#[tauri::command]
pub fn preview_local_notification_sound() -> Result<(), String> {
    let settings = settings::get_settings().map_err(|e| format!("Failed to load settings: {}", e))?;
    
    let sound_path = settings.local_notifications.default_sound_path.clone();
    let volume = settings.master_volume * settings.local_notifications.volume;
    
    if let Some(path) = sound_path {
        let path_clone = path.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(_) = play_sound_file(&path_clone, volume).await {
                // If custom sound fails, fall back to system sound
                play_windows_system_sound(false);
            }
        });
    } else {
        // No custom sound, use Windows system sound
        play_windows_system_sound(false);
    }
    
    Ok(())
}

/// Play a sound file with specified volume
async fn play_sound_file(path: &str, volume: f64) -> Result<(), String> {
    use rodio::{Decoder, OutputStream, Sink};
    use std::fs::File;
    use std::io::BufReader;
    
    // Get output stream (keep it alive)
    let (_stream, stream_handle) = OutputStream::try_default()
        .map_err(|e| format!("Failed to create audio output stream: {}", e))?;
    
    // Open file
    let file = File::open(path)
        .map_err(|e| format!("Failed to open sound file {}: {}", path, e))?;
    
    let source = Decoder::new(BufReader::new(file))
        .map_err(|e| format!("Failed to decode sound file {}: {}", path, e))?;
    
    // Create sink and set volume
    let sink = Sink::try_new(&stream_handle)
        .map_err(|e| format!("Failed to create audio sink: {}", e))?;
    
    sink.set_volume(volume as f32);
    sink.append(source);
    
    // Wait for playback to finish
    sink.sleep_until_end();
    
    Ok(())
}
