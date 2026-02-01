// src/data_availability/mod.rs
//! Data Availability Layer
//!
//! Provides content-addressed storage and redundancy for blockchain data.
//!
//! # Architecture
//!
//! - **IPFS**: Content-addressed primary storage
//! - **S3**: Cloud backup for redundancy
//! - **Archival**: Long-term historical data retention
//!
//! # Usage
//!
//! ```rust,ignore
//! // Create DA manager
//! let da = DataAvailability::new(ipfs_client, s3_backup, archival_config);
//!
//! // Store block
//! let result = da.store_block("hash123", block_data).await?;
//! println!("Block stored with CID: {}", result.ipfs_cid);
//!
//! // Retrieve block
//! let data = da.retrieve_block("hash123", Some("Qm...")).await?;
//! ```

pub mod ipfs;
pub mod s3_backup;
pub mod archival;
pub mod api;

use anyhow::Result;
use std::sync::Arc;
use serde::{Deserialize, Serialize};

pub use ipfs::{IpfsClient, IpfsConfig};
pub use s3_backup::{S3Backup, S3Config};
pub use archival::{ArchivalNode, ArchivalConfig, ArchiveResult};

/// Data availability manager
pub struct DataAvailability {
 /// IPFS client
 ipfs: Option<Arc<IpfsClient>>,

 /// S3 backup
 s3: Option<Arc<S3Backup>>,

 /// Archival node
 archival: Arc<ArchivalNode>,
}

impl DataAvailability {
 /// Create new data availability manager
 pub fn new(
 ipfs: Option<Arc<IpfsClient>>,
 s3: Option<Arc<S3Backup>>,
 archival_config: ArchivalConfig,
 ) -> Self {
 let archival = Arc::new(ArchivalNode::new(
 archival_config,
 ipfs.clone(),
 s3.clone(),
 ));

 Self { ipfs, s3, archival }
 }

 /// Store block data with redundancy
 ///
 /// Stores in IPFS (primary) and optionally S3 (backup)
 pub async fn store_block(&self, block_hash: &str, block_data: &[u8]) -> Result<StoreResult> {
 let mut result = StoreResult {
 success: false,
 ipfs_cid: None,
 s3_key: None,
 error: None,
 };

 // Primary storage: IPFS
 if let Some(ipfs) = &self.ipfs {
 match ipfs.store_block(block_data).await {
 Ok(cid) => {
 result.ipfs_cid = Some(cid);
 result.success = true;
 }
 Err(e) => {
 log::error!("Failed to store block {} in IPFS: {}", block_hash, e);
 result.error = Some(format!("IPFS: {}", e));
 }
 }
 }

 // Backup storage: S3 (optional)
 if let Some(s3) = &self.s3 {
 match s3.store_block(block_hash, block_data).await {
 Ok(key) => {
 result.s3_key = Some(key);
 result.success = true;
 }
 Err(e) => {
 log::warn!("Failed to backup block {} to S3: {}", block_hash, e);
 // Don't fail if S3 backup fails, IPFS is primary
 }
 }
 }

 if !result.success {
 anyhow::bail!("Failed to store block in any storage backend");
 }

 Ok(result)
 }

 /// Retrieve block data
 ///
 /// Tries IPFS first (if CID provided), then S3
 pub async fn retrieve_block(&self, block_hash: &str, cid: Option<&str>) -> Result<Vec<u8>> {
 // Try IPFS first if CID is provided
 if let (Some(ipfs), Some(cid)) = (&self.ipfs, cid) {
 match ipfs.retrieve_block(cid).await {
 Ok(data) => return Ok(data),
 Err(e) => {
 log::warn!("Failed to retrieve from IPFS {}: {}", cid, e);
 }
 }
 }

 // Fallback to S3
 if let Some(s3) = &self.s3 {
 match s3.retrieve_block(block_hash).await {
 Ok(data) => return Ok(data),
 Err(e) => {
 log::warn!("Failed to retrieve from S3: {}", e);
 }
 }
 }

 anyhow::bail!("Block not found: {}", block_hash)
 }

 /// Archive block for long-term storage
 pub async fn archive_block(&self, block_hash: &str, block_data: &[u8]) -> Result<ArchiveResult> {
 self.archival.archive_block(block_hash, block_data).await
 }

 /// Get archival node reference
 pub fn archival(&self) -> Arc<ArchivalNode> {
 self.archival.clone()
 }
}

/// Result of storing block data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreResult {
 pub success: bool,
 pub ipfs_cid: Option<String>,
 pub s3_key: Option<String>,
 pub error: Option<String>,
}
