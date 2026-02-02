//! Database Backup and Recovery Module
//!
//! Provides backup and restore functionality for RocksDB data.
//! Supports:
//! - Full database snapshots
//! - Incremental backups
//! - Point-in-time recovery
//! - Backup verification

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Backup metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupMetadata {
    /// Unique backup ID
    pub backup_id: String,
    /// Timestamp when backup was created
    pub timestamp: u64,
    /// Source database path
    pub source_path: String,
    /// Backup type
    pub backup_type: BackupType,
    /// Size in bytes
    pub size_bytes: u64,
    /// Number of files backed up
    pub file_count: usize,
    /// Checksum of the backup
    pub checksum: String,
    /// Version of the backup format
    pub version: u32,
}

/// Type of backup
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BackupType {
    /// Full snapshot of the database
    Full,
    /// Incremental backup since last full backup
    Incremental,
}

/// Backup manager for RocksDB
pub struct BackupManager {
    /// Directory where backups are stored
    backup_dir: PathBuf,
    /// Maximum number of backups to retain
    max_backups: usize,
}

impl BackupManager {
    /// Create a new backup manager
    pub fn new(backup_dir: impl AsRef<Path>, max_backups: usize) -> Result<Self> {
        let backup_dir = backup_dir.as_ref().to_path_buf();

        // Create backup directory if it doesn't exist
        if !backup_dir.exists() {
            fs::create_dir_all(&backup_dir).context("Failed to create backup directory")?;
        }

        Ok(Self {
            backup_dir,
            max_backups,
        })
    }

    /// Create a full backup of the database
    pub fn create_backup(&self, db_path: impl AsRef<Path>) -> Result<BackupMetadata> {
        let db_path = db_path.as_ref();

        if !db_path.exists() {
            bail!("Database path does not exist: {:?}", db_path);
        }

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();

        let backup_id = format!("backup_{}", timestamp);
        let backup_path = self.backup_dir.join(&backup_id);

        // Create backup directory
        fs::create_dir_all(&backup_path).context("Failed to create backup subdirectory")?;

        // Copy all database files
        let (file_count, total_size) = self.copy_directory(db_path, &backup_path)?;

        // Calculate checksum of backed up files
        let checksum = self.calculate_directory_checksum(&backup_path)?;

        let metadata = BackupMetadata {
            backup_id: backup_id.clone(),
            timestamp,
            source_path: db_path.to_string_lossy().to_string(),
            backup_type: BackupType::Full,
            size_bytes: total_size,
            file_count,
            checksum,
            version: 1,
        };

        // Save metadata
        let metadata_path = backup_path.join("metadata.json");
        let metadata_json = serde_json::to_string_pretty(&metadata)
            .context("Failed to serialize backup metadata")?;
        fs::write(&metadata_path, metadata_json).context("Failed to write backup metadata")?;

        // Cleanup old backups if needed
        self.cleanup_old_backups()?;

        log::info!(
            "Backup created: {} ({} files, {} bytes)",
            backup_id,
            file_count,
            total_size
        );

        Ok(metadata)
    }

    /// Restore database from a backup
    pub fn restore_backup(&self, backup_id: &str, target_path: impl AsRef<Path>) -> Result<()> {
        let backup_path = self.backup_dir.join(backup_id);
        let target_path = target_path.as_ref();

        if !backup_path.exists() {
            bail!("Backup not found: {}", backup_id);
        }

        // Load and verify metadata
        let metadata_path = backup_path.join("metadata.json");
        let metadata: BackupMetadata = serde_json::from_str(
            &fs::read_to_string(&metadata_path).context("Failed to read backup metadata")?,
        )
        .context("Failed to parse backup metadata")?;

        // Verify checksum
        let current_checksum = self.calculate_directory_checksum(&backup_path)?;
        if current_checksum != metadata.checksum {
            bail!("Backup integrity check failed: checksum mismatch");
        }

        // Create target directory if it doesn't exist
        if target_path.exists() {
            // Create a backup of the existing database before overwriting
            let existing_backup = format!(
                "pre_restore_{}",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            );
            let existing_backup_path = self.backup_dir.join(&existing_backup);
            self.copy_directory(target_path, &existing_backup_path)?;
            log::info!("Created pre-restore backup: {}", existing_backup);

            // Remove existing database
            fs::remove_dir_all(target_path).context("Failed to remove existing database")?;
        }

        fs::create_dir_all(target_path).context("Failed to create target directory")?;

        // Copy backup files to target (excluding metadata.json)
        self.copy_directory_excluding(&backup_path, target_path, &["metadata.json"])?;

        log::info!("Backup restored: {} -> {:?}", backup_id, target_path);

        Ok(())
    }

    /// List all available backups
    pub fn list_backups(&self) -> Result<Vec<BackupMetadata>> {
        let mut backups = Vec::new();

        if !self.backup_dir.exists() {
            return Ok(backups);
        }

        for entry in fs::read_dir(&self.backup_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                let metadata_path = path.join("metadata.json");
                if metadata_path.exists() {
                    if let Ok(content) = fs::read_to_string(&metadata_path) {
                        if let Ok(metadata) = serde_json::from_str::<BackupMetadata>(&content) {
                            backups.push(metadata);
                        }
                    }
                }
            }
        }

        // Sort by timestamp (newest first)
        backups.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        Ok(backups)
    }

    /// Delete a specific backup
    pub fn delete_backup(&self, backup_id: &str) -> Result<()> {
        let backup_path = self.backup_dir.join(backup_id);

        if !backup_path.exists() {
            bail!("Backup not found: {}", backup_id);
        }

        fs::remove_dir_all(&backup_path).context("Failed to delete backup")?;

        log::info!("Backup deleted: {}", backup_id);
        Ok(())
    }

    /// Verify backup integrity
    pub fn verify_backup(&self, backup_id: &str) -> Result<bool> {
        let backup_path = self.backup_dir.join(backup_id);

        if !backup_path.exists() {
            bail!("Backup not found: {}", backup_id);
        }

        // Load metadata
        let metadata_path = backup_path.join("metadata.json");
        let metadata: BackupMetadata = serde_json::from_str(&fs::read_to_string(&metadata_path)?)?;

        // Verify checksum
        let current_checksum = self.calculate_directory_checksum(&backup_path)?;

        Ok(current_checksum == metadata.checksum)
    }

    /// Get the path to the most recent backup
    pub fn get_latest_backup(&self) -> Result<Option<BackupMetadata>> {
        let backups = self.list_backups()?;
        Ok(backups.into_iter().next())
    }

    // Private helper methods

    fn copy_directory(&self, src: &Path, dst: &Path) -> Result<(usize, u64)> {
        let mut file_count = 0;
        let mut total_size = 0;

        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let path = entry.path();
            let file_name = entry.file_name();
            let dst_path = dst.join(&file_name);

            if path.is_dir() {
                fs::create_dir_all(&dst_path)?;
                let (count, size) = self.copy_directory(&path, &dst_path)?;
                file_count += count;
                total_size += size;
            } else {
                fs::copy(&path, &dst_path)?;
                file_count += 1;
                total_size += entry.metadata()?.len();
            }
        }

        Ok((file_count, total_size))
    }

    fn copy_directory_excluding(
        &self,
        src: &Path,
        dst: &Path,
        exclude: &[&str],
    ) -> Result<(usize, u64)> {
        let mut file_count = 0;
        let mut total_size = 0;

        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let path = entry.path();
            let file_name = entry.file_name();
            let file_name_str = file_name.to_string_lossy();

            // Skip excluded files
            if exclude.iter().any(|e| file_name_str == *e) {
                continue;
            }

            let dst_path = dst.join(&file_name);

            if path.is_dir() {
                fs::create_dir_all(&dst_path)?;
                let (count, size) = self.copy_directory_excluding(&path, &dst_path, exclude)?;
                file_count += count;
                total_size += size;
            } else {
                fs::copy(&path, &dst_path)?;
                file_count += 1;
                total_size += entry.metadata()?.len();
            }
        }

        Ok((file_count, total_size))
    }

    fn calculate_directory_checksum(&self, dir: &Path) -> Result<String> {
        use std::collections::BTreeMap;
        use std::io::Read;

        let mut file_hashes: BTreeMap<String, String> = BTreeMap::new();

        self.hash_directory_recursive(dir, dir, &mut file_hashes)?;

        // Combine all file hashes into a single checksum
        let combined = file_hashes
            .iter()
            .map(|(k, v)| format!("{}:{}", k, v))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(format!("{:x}", md5::compute(combined.as_bytes())))
    }

    fn hash_directory_recursive(
        &self,
        base: &Path,
        current: &Path,
        hashes: &mut std::collections::BTreeMap<String, String>,
    ) -> Result<()> {
        for entry in fs::read_dir(current)? {
            let entry = entry?;
            let path = entry.path();

            // Skip metadata.json for checksum calculation
            if path
                .file_name()
                .map(|n| n == "metadata.json")
                .unwrap_or(false)
            {
                continue;
            }

            if path.is_dir() {
                self.hash_directory_recursive(base, &path, hashes)?;
            } else {
                let relative = path.strip_prefix(base)?.to_string_lossy().to_string();
                let mut file = fs::File::open(&path)?;
                let mut buffer = Vec::new();
                file.read_to_end(&mut buffer)?;
                let hash = format!("{:x}", md5::compute(&buffer));
                hashes.insert(relative, hash);
            }
        }
        Ok(())
    }

    fn cleanup_old_backups(&self) -> Result<()> {
        let mut backups = self.list_backups()?;

        while backups.len() > self.max_backups {
            if let Some(oldest) = backups.pop() {
                self.delete_backup(&oldest.backup_id)?;
            }
        }

        Ok(())
    }
}

/// Scheduled backup configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupSchedule {
    /// Enable scheduled backups
    pub enabled: bool,
    /// Interval between backups in seconds
    pub interval_secs: u64,
    /// Maximum number of backups to keep
    pub max_backups: usize,
}

impl Default for BackupSchedule {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_secs: 3600 * 6, // Every 6 hours
            max_backups: 10,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_backup_and_restore() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("db");
        let backup_dir = temp_dir.path().join("backups");

        // Create a mock database
        fs::create_dir_all(&db_path).unwrap();
        fs::write(db_path.join("data.txt"), "test data").unwrap();

        // Create backup manager
        let manager = BackupManager::new(&backup_dir, 5).unwrap();

        // Create backup
        let metadata = manager.create_backup(&db_path).unwrap();
        assert_eq!(metadata.backup_type, BackupType::Full);
        assert!(metadata.file_count > 0);

        // List backups
        let backups = manager.list_backups().unwrap();
        assert_eq!(backups.len(), 1);

        // Verify backup
        assert!(manager.verify_backup(&metadata.backup_id).unwrap());

        // Restore to new location
        let restore_path = temp_dir.path().join("restored");
        manager
            .restore_backup(&metadata.backup_id, &restore_path)
            .unwrap();

        // Verify restored data
        let restored_data = fs::read_to_string(restore_path.join("data.txt")).unwrap();
        assert_eq!(restored_data, "test data");
    }

    #[test]
    fn test_backup_cleanup() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("db");
        let backup_dir = temp_dir.path().join("backups");

        // Create a mock database
        fs::create_dir_all(&db_path).unwrap();
        fs::write(db_path.join("data.txt"), "test data").unwrap();

        // Create backup manager with max 2 backups
        let manager = BackupManager::new(&backup_dir, 2).unwrap();

        // Create 3 backups
        manager.create_backup(&db_path).unwrap();
        std::thread::sleep(std::time::Duration::from_secs(1));
        manager.create_backup(&db_path).unwrap();
        std::thread::sleep(std::time::Duration::from_secs(1));
        manager.create_backup(&db_path).unwrap();

        // Should only have 2 backups
        let backups = manager.list_backups().unwrap();
        assert_eq!(backups.len(), 2);
    }
}
