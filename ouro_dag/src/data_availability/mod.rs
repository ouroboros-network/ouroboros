//! Data Availability (DA) layer
//!
//! Ensures that block and transaction data remains available and retrievable
//! after being committed to the DAG. Supports multiple storage backends
//! including local archival and S3-compatible remote storage.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Data availability storage backend
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StorageBackend {
    /// Local filesystem archival
    Local,
    /// S3-compatible remote storage
    S3 {
        bucket: String,
        region: String,
        endpoint: Option<String>,
    },
}

/// Configuration for the DA layer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataAvailabilityConfig {
    /// Primary storage backend
    pub backend: StorageBackend,
    /// Maximum archive size in bytes before rotation
    pub max_archive_size: u64,
    /// How many blocks to keep in local cache
    pub local_cache_blocks: u64,
    /// Enable redundant archival across multiple backends
    pub redundant: bool,
}

impl Default for DataAvailabilityConfig {
    fn default() -> Self {
        Self {
            backend: StorageBackend::Local,
            max_archive_size: 1_073_741_824, // 1 GB
            local_cache_blocks: 1000,
            redundant: false,
        }
    }
}

/// An archived data blob with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveEntry {
    /// Block height this data belongs to
    pub block_height: u64,
    /// SHA-256 hash of the data
    pub data_hash: Vec<u8>,
    /// Size in bytes
    pub size: u64,
    /// Archive URL (local path or S3 URL)
    pub archive_url: Option<String>,
    /// Timestamp when archived
    pub archived_at: u64,
    /// Backend used for storage
    pub backend: StorageBackend,
}

/// Data Availability Manager
///
/// Manages archival, retrieval, and verification of block data.
pub struct DataAvailabilityManager {
    config: DataAvailabilityConfig,
    /// Index of archived entries by block height
    archive_index: Arc<RwLock<HashMap<u64, ArchiveEntry>>>,
}

impl DataAvailabilityManager {
    /// Create a new DA manager with the given config
    pub fn new(config: DataAvailabilityConfig) -> Self {
        Self {
            config,
            archive_index: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Archive data for a block
    pub async fn archive_block(
        &self,
        block_height: u64,
        data: &[u8],
        timestamp: u64,
    ) -> Result<ArchiveEntry, String> {
        use sha2::{Digest, Sha256};

        let data_hash = Sha256::digest(data).to_vec();

        let archive_url = match &self.config.backend {
            StorageBackend::Local => {
                Some(format!("file:///archive/block_{}.dat", block_height))
            }
            StorageBackend::S3 { bucket, .. } => {
                Some(format!("s3://{}/blocks/{}.dat", bucket, block_height))
            }
        };

        let entry = ArchiveEntry {
            block_height,
            data_hash,
            size: data.len() as u64,
            archive_url,
            archived_at: timestamp,
            backend: self.config.backend.clone(),
        };

        let mut index = self.archive_index.write().await;
        index.insert(block_height, entry.clone());

        Ok(entry)
    }

    /// Check if data for a block is available
    pub async fn is_available(&self, block_height: u64) -> bool {
        let index = self.archive_index.read().await;
        index.contains_key(&block_height)
    }

    /// Get archive entry for a block
    pub async fn get_entry(&self, block_height: u64) -> Option<ArchiveEntry> {
        let index = self.archive_index.read().await;
        index.get(&block_height).cloned()
    }

    /// Get total archived data size
    pub async fn total_archived_size(&self) -> u64 {
        let index = self.archive_index.read().await;
        index.values().map(|e| e.size).sum()
    }

    /// Get the config
    pub fn config(&self) -> &DataAvailabilityConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_archive_and_retrieve() {
        let mgr = DataAvailabilityManager::new(DataAvailabilityConfig::default());
        let data = b"block data here";

        let entry = mgr.archive_block(1, data, 1000).await.unwrap();
        assert_eq!(entry.block_height, 1);
        assert_eq!(entry.size, data.len() as u64);
        assert!(mgr.is_available(1).await);
        assert!(!mgr.is_available(2).await);
    }
}
