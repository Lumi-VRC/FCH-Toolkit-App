// Updater: Check for updates and download/install setup.exe
//
// This module handles:
// 1. Checking GitHub releases for new versions
// 2. Downloading the setup.exe installer
// 3. Running the installer (with elevation on Windows)

use std::path::PathBuf;
use std::fs;
use tauri::{AppHandle, Emitter};
use serde::{Deserialize, Serialize};

const GITHUB_REPO: &str = "Lumi-VRC/FCH-Toolkit-App";
const GITHUB_API_BASE: &str = "https://api.github.com/repos";

#[derive(Debug, Serialize, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    name: Option<String>,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateInfo {
    pub version: String,
    pub download_url: String,
    pub filename: String,
    pub size: u64,
}

/// Fetch the latest release from GitHub
async fn fetch_latest_release() -> Result<GitHubRelease, String> {
    let url = format!("{}/{}/releases/latest", GITHUB_API_BASE, GITHUB_REPO);
    
    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "FCH-App-Updater")
        .send()
        .await
        .map_err(|e| format!("Failed to fetch release: {}", e))?;
    
    if !response.status().is_success() {
        let status = response.status();
        if status == 404 {
            return Err(format!("No releases found for repository {}. The repository may not exist, be private, or have no releases yet.", GITHUB_REPO));
        }
        return Err(format!("GitHub API returned status: {} - {}", status, status.canonical_reason().unwrap_or("Unknown error")));
    }
    
    let release: GitHubRelease = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse release JSON: {}", e))?;
    
    Ok(release)
}

/// Normalize version string (remove 'v' prefix)
fn normalize_version(version: &str) -> String {
    version.trim_start_matches('v').trim().to_string()
}

/// Compare semantic versions
fn compare_versions(local: &str, remote: &str) -> i32 {
    let local_parts: Vec<u32> = local
        .split('.')
        .map(|x| x.parse::<u32>().unwrap_or(0))
        .collect();
    let remote_parts: Vec<u32> = remote
        .split('.')
        .map(|x| x.parse::<u32>().unwrap_or(0))
        .collect();
    
    let max_len = local_parts.len().max(remote_parts.len());
    for i in 0..max_len {
        let local_val = local_parts.get(i).copied().unwrap_or(0);
        let remote_val = remote_parts.get(i).copied().unwrap_or(0);
        
        if remote_val > local_val {
            return 1;
        } else if remote_val < local_val {
            return -1;
        }
    }
    0
}

/// Check if an update is available
#[tauri::command]
pub async fn check_for_update(local_version: String) -> Result<Option<UpdateInfo>, String> {
    let local_v = normalize_version(&local_version);
    
    let release = fetch_latest_release().await?;
    let remote_v = normalize_version(&release.tag_name);
    
    // Log versions for debugging (only in debug builds)
    crate::debug_eprintln!("[Updater] Comparing versions - Local: {}, Remote: {}", local_v, remote_v);
    
    // Check if remote version is newer
    // compare_versions returns: 1 if remote > local, -1 if remote < local, 0 if equal
    let comparison = compare_versions(&local_v, &remote_v);
    crate::debug_eprintln!("[Updater] Version comparison result: {}", comparison);
    
    if comparison <= 0 {
        // Remote is not newer than local (equal or older)
        return Ok(None); // No update available
    }
    
    // Find the setup.exe asset
    let setup_asset = release
        .assets
        .iter()
        .find(|asset| {
            let name_lower = asset.name.to_lowercase();
            name_lower.contains("setup") && name_lower.ends_with(".exe")
        })
        .ok_or_else(|| "No setup.exe found in release assets".to_string())?;
    
    Ok(Some(UpdateInfo {
        version: remote_v,
        download_url: setup_asset.browser_download_url.clone(),
        filename: setup_asset.name.clone(),
        size: setup_asset.size,
    }))
}

/// Get the downloads directory path
fn get_downloads_dir() -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    {
        use std::env;
        if let Ok(user_profile) = env::var("USERPROFILE") {
            let mut path = PathBuf::from(user_profile);
            path.push("Downloads");
            return Ok(path);
        }
    }
    
    // Fallback to temp directory
    std::env::temp_dir()
        .parent()
        .ok_or_else(|| "Failed to get temp directory".to_string())
        .map(|p| p.to_path_buf())
}

/// Download the setup.exe file
#[tauri::command]
pub async fn download_update(download_url: String, filename: String) -> Result<String, String> {
    let downloads_dir = get_downloads_dir()?;
    let file_path = downloads_dir.join(&filename);
    
    // Create downloads directory if it doesn't exist
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create downloads directory: {}", e))?;
    }
    
    // Download the file
    let client = reqwest::Client::new();
    let response = client
        .get(&download_url)
        .header("User-Agent", "FCH-App-Updater")
        .send()
        .await
        .map_err(|e| format!("Failed to download update: {}", e))?;
    
    if !response.status().is_success() {
        return Err(format!("Download failed with status: {}", response.status()));
    }
    
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read download: {}", e))?;
    
    // Write to file
    fs::write(&file_path, bytes)
        .map_err(|e| format!("Failed to save file: {}", e))?;
    
    // Return the file path as string
    file_path
        .to_str()
        .ok_or_else(|| "Invalid file path".to_string())
        .map(|s| s.to_string())
}

/// Run the installer (with elevation on Windows)
#[tauri::command]
pub async fn run_installer(app_handle: AppHandle, installer_path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        
        // Use PowerShell to run with elevation
        // This will prompt for admin rights if needed
        let ps_command = format!(
            "Start-Process -FilePath '{}' -Verb RunAs -Wait",
            installer_path.replace('\'', "''") // Escape single quotes
        );
        
        let output = Command::new("powershell")
            .arg("-Command")
            .arg(&ps_command)
            .output()
            .map_err(|e| format!("Failed to run installer: {}", e))?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Installer failed: {}", stderr));
        }
        
        // Emit event to notify frontend that installer is running
        app_handle
            .emit("updater:installer-started", ())
            .map_err(|e| format!("Failed to emit event: {}", e))?;
        
        Ok(())
    }
    
    #[cfg(not(target_os = "windows"))]
    {
        // For non-Windows, just try to execute directly
        use std::process::Command;
        
        Command::new(&installer_path)
            .spawn()
            .map_err(|e| format!("Failed to run installer: {}", e))?;
        
        app_handle
            .emit("updater:installer-started", ())
            .map_err(|e| format!("Failed to emit event: {}", e))?;
        
        Ok(())
    }
}

/// Download and run the installer in one step
#[tauri::command]
pub async fn download_and_install_update(
    app_handle: AppHandle,
    download_url: String,
    filename: String,
) -> Result<String, String> {
    // Download the file
    let installer_path = download_update(download_url, filename).await?;
    
    // Run the installer
    run_installer(app_handle, installer_path.clone()).await?;
    
    Ok(installer_path)
}
