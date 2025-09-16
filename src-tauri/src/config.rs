// App configuration (currently: notification sound path/volume).
//
// We persist a tiny JSON file under the same app data directory used by
// notes/watchlist. Keeping it as JSON makes it simple to debug and edit.
//
// ...
// ...
// Also because SQL makes my head hurt.
use std::path::PathBuf;
use std::sync::{OnceLock};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)]
pub struct AppConfig {
	// Optional absolute path to a custom audio file for watchlist joins. Fart reverb is always funny.
	#[serde(default)]
	pub sound_path: Option<String>,
	// Volume in range [0,1]. If None, we default to 1.0 on playback.
	#[serde(default)]
	pub sound_volume: Option<f32>,
}

// Where the config JSON lives on disk.
fn config_path() -> PathBuf { super::notes::notes_dir().join("config.json") }

pub fn load_config() -> AppConfig {
	// Best-effort read. Any error (missing or malformed file) returns default.
	// I totally stole that big one-liner from stackoverflow. Sue me.
	let p = config_path();
	if let Ok(d) = std::fs::read(&p) { if let Ok(c) = serde_json::from_slice::<AppConfig>(&d) { return c; } }
	AppConfig::default()
}

pub fn save_config(c: &AppConfig) -> Result<(), String> {
	// Ensure the directory exists, then write a pretty JSON snapshot.
	// Three bottles of alcohol consumed before I figured out namespace usage properly.
	// I'm not sure if this is the best way to do it but it works for now. Better this than like 30 extra lines of code.
	let dir = super::notes::notes_dir();
	if let Err(e) = std::fs::create_dir_all(&dir) { return Err(e.to_string()); }
	let p = config_path();
	let data = serde_json::to_vec_pretty(c).map_err(|e| e.to_string())?;
	std::fs::write(p, data).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_config() -> Result<serde_json::Value, String> {
	// Read config and return JSON with camel case keys the front-end expects.
	// I didn't know it was called 'camel case' until I spent 5 hours getting this tiny fking function functional.
	let c = load_config();
	Ok(serde_json::json!({ "soundPath": c.sound_path, "soundVolume": c.sound_volume }))
	// ok :D
}

#[tauri::command]
pub fn set_config(sound_path: Option<String>, sound_volume: Option<f32>) -> Result<(), String> {
	// Partial updates are supported: either field may be None.
	// Allowing "None" to prevent future errors if I allow unique sounds per watchlisted user. (Staff join notifs?)
	let mut c = load_config();
	c.sound_path = sound_path;
	if let Some(v) = sound_volume { c.sound_volume = Some(v.clamp(0.0, 1.0)); }
	save_config(&c)
}

pub fn play_custom_sound(path: &str, volume: f32) -> Result<(), String> {
	// Simple blocking playback helper used by preview and watchlist notifications
	let file = std::fs::File::open(path).map_err(|_| "cannot open custom sound file".to_string())?;
	let (stream, handle) = rodio::OutputStream::try_default().map_err(|_| "rodio output stream failed".to_string())?;
	let decoder = rodio::Decoder::new(std::io::BufReader::new(file)).map_err(|_| "rodio decode failed".to_string())?;
	let sink = rodio::Sink::try_new(&handle).map_err(|_| "rodio sink create failed".to_string())?;
	// Clamp the provided volume defensively
	sink.set_volume(volume.max(0.0).min(1.0));
	sink.append(decoder);
	// Block this thread until playback completes; safe in a short-lived thread
	sink.sleep_until_end();
	// Explicitly drop the stream so the device closes cleanly
	drop(stream);
	Ok(())
}

// Guard to avoid overlapping sound playback
static PLAYING: OnceLock<AtomicBool> = OnceLock::new();

fn try_begin_play() -> bool {
	let flag = PLAYING.get_or_init(|| AtomicBool::new(false));
	!flag.swap(true, Ordering::SeqCst)
}

fn end_play() {
	if let Some(flag) = PLAYING.get() { flag.store(false, Ordering::SeqCst); }
}

#[tauri::command]
pub fn browse_sound() -> Result<serde_json::Value, String> {
	// Show a native file picker for WAV/MP3 and return the selected path.
	let file = rfd::FileDialog::new().add_filter("Audio", &["wav", "mp3"]).pick_file();
	Ok(serde_json::json!({ "path": file.map(|p| p.to_string_lossy().to_string()) }))
}

#[tauri::command]
pub fn preview_sound() -> Result<(), String> {
	// Respect the guard: discard if another sound is currently playing
	if !try_begin_play() { return Ok(()); }
	let cfg = load_config();
	let _ = std::thread::spawn(move || {
		match cfg.sound_path.as_deref().filter(|s| !s.is_empty()) {
			Some(p) => {
				let vol = cfg.sound_volume.unwrap_or(1.0).clamp(0.0, 1.0);
				let _ = play_custom_sound(p, vol);
			}
			None => {
				#[cfg(target_os = "windows")]
				unsafe {
					use windows::Win32::Media::Audio::{PlaySoundW, SND_ALIAS, SND_ASYNC};
					use windows::core::w;
					let _ = PlaySoundW(w!("SystemAsterisk"), None, SND_ALIAS | SND_ASYNC);
				}
			}
		}
		end_play();
	});
	Ok(())
}
