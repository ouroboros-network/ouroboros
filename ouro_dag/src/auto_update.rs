use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs;
use std::path::Path;
use chrono::{DateTime, Utc};

const GITHUB_REPO: &str = "ouroboros-network/ouroboros";
const UPDATE_CHECK_INTERVAL_HOURS: u64 = 24;

/// Configuration for automatic updates
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UpdateConfig {
 /// Whether automatic updates are enabled
 pub auto_update_enabled: bool,

 /// How often to check for updates (in hours)
 pub check_interval_hours: u64,

 /// Update channel: "stable" or "beta"
 pub channel: String,

 /// Last time we checked for updates
 pub last_check: Option<String>,

 /// Currently installed version
 pub current_version: String,
}

impl Default for UpdateConfig {
 fn default() -> Self {
 Self {
 auto_update_enabled: false, // Opt-in by default
 check_interval_hours: UPDATE_CHECK_INTERVAL_HOURS,
 channel: "stable".to_string(),
 last_check: None,
 current_version: env!("CARGO_PKG_VERSION").to_string(),
 }
 }
}

impl UpdateConfig {
 /// Load config from file or create default
 pub fn load_or_create(path: &Path) -> Result<Self, Box<dyn Error>> {
 if path.exists() {
 let json = fs::read_to_string(path)?;
 Ok(serde_json::from_str(&json)?)
 } else {
 let config = Self::default();
 config.save(path)?;
 Ok(config)
 }
 }

 /// Save config to disk
 pub fn save(&self, path: &Path) -> Result<(), Box<dyn Error>> {
 if let Some(parent) = path.parent() {
 fs::create_dir_all(parent)?;
 }
 let json = serde_json::to_string_pretty(self)?;
 fs::write(path, json)?;
 Ok(())
 }

 /// Check if we should check for updates now
 pub fn should_check_now(&self) -> bool {
 match &self.last_check {
 None => true,
 Some(last) => {
 let last_check_time = DateTime::parse_from_rfc3339(last).ok();
 if let Some(last_time) = last_check_time {
 let now = Utc::now();
 let elapsed = now.signed_duration_since(last_time.with_timezone(&Utc));
 elapsed.num_hours() >= self.check_interval_hours as i64
 } else {
 true
 }
 }
 }
 }
}

/// Information about an available update
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UpdateInfo {
 pub version: String,
 pub download_url: String,
 pub release_notes: String,
 pub published_at: String,
 pub checksum: Option<String>,
}

/// Check for updates from GitHub releases
pub async fn check_for_updates(current_version: &str) -> Result<Option<UpdateInfo>, Box<dyn Error>> {
 let url = format!("https://api.github.com/repos/{}/releases/latest", GITHUB_REPO);

 let client = reqwest::Client::builder()
 .user_agent("ouroboros-node")
 .build()?;

 let response = client.get(&url).send().await?;

 if !response.status().is_success() {
 return Err(format!("GitHub API error: {}", response.status()).into());
 }

 let release: GithubRelease = response.json().await?;

 // Compare versions
 if is_newer_version(&release.tag_name, current_version) {
 // Find binary in assets
 let asset = release.assets.iter()
 .find(|a| a.name.ends_with(".exe") || a.name == "ouro_dag")
 .ok_or("No binary found in release")?;

 Ok(Some(UpdateInfo {
 version: release.tag_name.clone(),
 download_url: asset.browser_download_url.clone(),
 release_notes: release.body.unwrap_or_default(),
 published_at: release.published_at.clone(),
 checksum: None, // TODO: Extract from release notes
 }))
 } else {
 Ok(None)
 }
}

/// Download update binary
pub async fn download_update(update: &UpdateInfo, dest_path: &Path) -> Result<(), Box<dyn Error>> {
 if let Some(parent) = dest_path.parent() {
 fs::create_dir_all(parent)?;
 }

 println!(" Downloading update {} from {}", update.version, update.download_url);

 let client = reqwest::Client::builder()
 .user_agent("ouroboros-node")
 .build()?;

 let response = client.get(&update.download_url).send().await?;

 if !response.status().is_success() {
 return Err(format!("Download failed: {}", response.status()).into());
 }

 let bytes = response.bytes().await?;
 fs::write(dest_path, bytes)?;

 println!(" Downloaded to {}", dest_path.display());

 Ok(())
}

/// Verify checksum of downloaded file
pub fn verify_checksum(file_path: &Path, expected: &str) -> Result<bool, Box<dyn Error>> {
 use sha2::{Sha256, Digest};

 let contents = fs::read(file_path)?;
 let mut hasher = Sha256::new();
 hasher.update(&contents);
 let result = hasher.finalize();
 let actual = hex::encode(result);

 Ok(actual.eq_ignore_ascii_case(expected))
}

/// Apply update by replacing current binary
pub fn apply_update(update_path: &Path, current_path: &Path, backup_path: &Path) -> Result<(), Box<dyn Error>> {
 // Backup current binary
 if current_path.exists() {
 fs::copy(current_path, backup_path)?;
 println!(" Backed up current binary to {}", backup_path.display());
 }

 // Replace with new binary
 fs::copy(update_path, current_path)?;
 println!(" Applied update successfully");

 // Clean up update file
 fs::remove_file(update_path)?;

 Ok(())
}

/// Rollback to previous version
pub fn rollback_update(backup_path: &Path, current_path: &Path) -> Result<(), Box<dyn Error>> {
 if !backup_path.exists() {
 return Err("No backup found for rollback".into());
 }

 fs::copy(backup_path, current_path)?;
 println!("↩ Rolled back to previous version");

 Ok(())
}

/// Compare version strings (simplified semver comparison)
fn is_newer_version(new: &str, current: &str) -> bool {
 // Strip 'v' prefix if present
 let new = new.trim_start_matches('v');
 let current = current.trim_start_matches('v');

 // Split into parts
 let new_parts: Vec<u32> = new.split('.').filter_map(|s| s.parse().ok()).collect();
 let current_parts: Vec<u32> = current.split('.').filter_map(|s| s.parse().ok()).collect();

 // Compare major.minor.patch
 for i in 0..3 {
 let new_part = new_parts.get(i).copied().unwrap_or(0);
 let current_part = current_parts.get(i).copied().unwrap_or(0);

 if new_part > current_part {
 return true;
 } else if new_part < current_part {
 return false;
 }
 }

 false
}

// GitHub API types
#[derive(Deserialize)]
struct GithubRelease {
 tag_name: String,
 body: Option<String>,
 published_at: String,
 assets: Vec<GithubAsset>,
}

#[derive(Deserialize)]
struct GithubAsset {
 name: String,
 browser_download_url: String,
}

#[cfg(test)]
mod tests {
 use super::*;

 #[test]
 fn test_version_comparison() {
 assert!(is_newer_version("v0.3.0", "v0.2.0"));
 assert!(is_newer_version("0.3.0", "0.2.0"));
 assert!(is_newer_version("v1.0.0", "v0.9.9"));
 assert!(!is_newer_version("v0.2.0", "v0.3.0"));
 assert!(!is_newer_version("v0.2.0", "v0.2.0"));
 }

 #[test]
 fn test_should_check_now() {
 let mut config = UpdateConfig::default();
 assert!(config.should_check_now()); // No last check

 config.last_check = Some(Utc::now().to_rfc3339());
 assert!(!config.should_check_now()); // Just checked

 // Simulate 25 hours ago
 let past = Utc::now() - chrono::Duration::hours(25);
 config.last_check = Some(past.to_rfc3339());
 assert!(config.should_check_now()); // Should check
 }
}
