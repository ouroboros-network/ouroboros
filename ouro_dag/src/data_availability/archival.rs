// src/data_availability/archival.rs
//! Archival node support for long-term block history
//!
//! Archival nodes store complete blockchain history for explorers and auditing

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use super::ipfs::IpfsClient;
use super::s3_backup::S3Backup;

/// Archival node configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchivalConfig {
 /// Enable archival mode
 pub enabled: bool,

 /// Store blocks in IPFS
 pub use_ipfs: bool,

 /// Store blocks in S3
 pub use_s3: bool,

 /// Retention policy (days, 0 = forever)
 pub retention_days: u64,

 /// Prune blocks older than retention period
 pub enable_pruning: bool,
}

impl Default for ArchivalConfig {
 fn default() -> Self {
 Self {
 enabled: false,
 use_ipfs: true,
 use_s3: false,
 retention_days: 0, // Keep forever by default
 enable_pruning: false,
 }
 }
}

/// Archival node manager
pub struct ArchivalNode {
 /// Configuration
 config: ArchivalConfig,

 /// IPFS client (optional)
 ipfs: Option<Arc<IpfsClient>>,

 /// S3 backup (optional)
 s3: Option<Arc<S3Backup>>,
}

impl ArchivalNode {
 /// Create new archival node
 pub fn new(
 config: ArchivalConfig,
 ipfs: Option<Arc<IpfsClient>>,
 s3: Option<Arc<S3Backup>>,
 ) -> Self {
 Self { config, ipfs, s3 }
 }

 /// Archive a block to long-term storage
 pub async fn archive_block(&self, block_hash: &str, block_data: &[u8]) -> Result<ArchiveResult> {
 if !self.config.enabled {
 return Ok(ArchiveResult {
 ipfs_cid: None,
 s3_key: None,
 error: Some("Archival not enabled".to_string()),
 });
 }

 let mut result = ArchiveResult {
 ipfs_cid: None,
 s3_key: None,
 error: None,
 };

 // Store in IPFS
 if self.config.use_ipfs {
 if let Some(ipfs) = &self.ipfs {
 match ipfs.store_block(block_data).await {
 Ok(cid) => {
 result.ipfs_cid = Some(cid.clone());
 log::info!(" Archived block {} to IPFS: {}", block_hash, cid);

 // Pin important blocks
 if let Err(e) = ipfs.pin_block(&cid).await {
 log::warn!("Failed to pin block {}: {}", cid, e);
 }
 }
 Err(e) => {
 log::error!("Failed to archive block {} to IPFS: {}", block_hash, e);
 result.error = Some(format!("IPFS error: {}", e));
 }
 }
 }
 }

 // Store in S3
 if self.config.use_s3 {
 if let Some(s3) = &self.s3 {
 match s3.store_block(block_hash, block_data).await {
 Ok(key) => {
 result.s3_key = Some(key.clone());
 log::info!(" Archived block {} to S3: {}", block_hash, key);
 }
 Err(e) => {
 log::error!("Failed to archive block {} to S3: {}", block_hash, e);
 if result.error.is_none() {
 result.error = Some(format!("S3 error: {}", e));
 }
 }
 }
 }
 }

 Ok(result)
 }

 /// Retrieve block from archival storage
 pub async fn retrieve_block(&self, block_hash: &str, cid: Option<&str>) -> Result<Vec<u8>> {
 // Try IPFS first if CID is provided
 if let (Some(ipfs), Some(cid)) = (&self.ipfs, cid) {
 if let Ok(data) = ipfs.retrieve_block(cid).await {
 return Ok(data);
 }
 }

 // Fallback to S3
 if let Some(s3) = &self.s3 {
 return s3.retrieve_block(block_hash).await;
 }

 anyhow::bail!("Block not found in archival storage: {}", block_hash)
 }

 /// Prune old blocks based on retention policy
 pub async fn prune_old_blocks(&self, current_timestamp: u64) -> Result<PruneResult> {
 if !self.config.enable_pruning || self.config.retention_days == 0 {
 return Ok(PruneResult {
 blocks_pruned: 0,
 bytes_freed: 0,
 });
 }

 let retention_seconds = self.config.retention_days * 24 * 60 * 60;
 let cutoff_timestamp = current_timestamp.saturating_sub(retention_seconds);

 log::info!(" Pruning blocks older than {} days (timestamp < {})",
 self.config.retention_days, cutoff_timestamp);

 // TODO: Implement actual pruning logic
 // This would require:
 // 1. Query database for blocks older than cutoff
 // 2. Delete from IPFS (unpin)
 // 3. Delete from S3
 // 4. Track statistics

 Ok(PruneResult {
 blocks_pruned: 0,
 bytes_freed: 0,
 })
 }
}

/// Result of archiving a block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveResult {
 /// IPFS CID (if stored in IPFS)
 pub ipfs_cid: Option<String>,

 /// S3 key (if stored in S3)
 pub s3_key: Option<String>,

 /// Error message (if any)
 pub error: Option<String>,
}

/// Result of pruning operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PruneResult {
 /// Number of blocks pruned
 pub blocks_pruned: u64,

 /// Bytes freed
 pub bytes_freed: u64,
}
