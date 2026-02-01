// src/data_availability/ipfs.rs
//! IPFS integration for content-addressed block storage
//!
//! Stores block data in IPFS and references it by CID (Content Identifier)

use anyhow::{Result, bail};
use ipfs_api_backend_hyper::{IpfsApi, IpfsClient as HyperIpfsClient, TryFromUri};
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use futures::TryStreamExt;

/// IPFS client for block data storage
pub struct IpfsClient {
 /// Underlying IPFS client
 client: HyperIpfsClient,

 /// IPFS node multiaddr
 node_addr: String,
}

/// IPFS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpfsConfig {
 /// IPFS node address (e.g., "/ip4/127.0.0.1/tcp/5001")
 pub node_addr: String,

 /// Enable pinning (keep data permanently)
 pub enable_pinning: bool,

 /// Timeout for operations (seconds)
 pub timeout_secs: u64,
}

impl Default for IpfsConfig {
 fn default() -> Self {
 Self {
 node_addr: "http://127.0.0.1:5001".to_string(),
 enable_pinning: true,
 timeout_secs: 30,
 }
 }
}

impl IpfsClient {
 /// Create new IPFS client
 pub fn new(config: IpfsConfig) -> Result<Self> {
 let client = HyperIpfsClient::from_str(&config.node_addr)
 .map_err(|e| anyhow::anyhow!("Failed to create IPFS client: {}", e))?;

 Ok(Self {
 client,
 node_addr: config.node_addr,
 })
 }

 /// Store block data in IPFS
 ///
 /// Returns the CID (Content Identifier) which can be used to retrieve the data
 pub async fn store_block(&self, block_data: &[u8]) -> Result<String> {
 let cursor = Cursor::new(block_data);

 match self.client.add(cursor).await {
 Ok(response) => {
 let cid = response.hash;
 log::info!(" Stored block in IPFS: {} ({} bytes)", cid, block_data.len());
 Ok(cid)
 }
 Err(e) => {
 bail!("Failed to store block in IPFS: {}", e)
 }
 }
 }

 /// Retrieve block data from IPFS
 ///
 /// # Arguments
 /// * `cid` - Content Identifier from store_block()
 pub async fn retrieve_block(&self, cid: &str) -> Result<Vec<u8>> {
 match self.client.cat(cid).map_ok(|chunk| chunk.to_vec()).try_concat().await {
 Ok(data) => {
 log::info!(" Retrieved block from IPFS: {} ({} bytes)", cid, data.len());
 Ok(data)
 }
 Err(e) => {
 bail!("Failed to retrieve block from IPFS {}: {}", cid, e)
 }
 }
 }

 /// Pin block data (prevent garbage collection)
 pub async fn pin_block(&self, cid: &str) -> Result<()> {
 match self.client.pin_add(cid, false).await {
 Ok(_) => {
 log::info!(" Pinned block in IPFS: {}", cid);
 Ok(())
 }
 Err(e) => {
 bail!("Failed to pin block {}: {}", cid, e)
 }
 }
 }

 /// Unpin block data (allow garbage collection)
 pub async fn unpin_block(&self, cid: &str) -> Result<()> {
 match self.client.pin_rm(cid, false).await {
 Ok(_) => {
 log::info!(" Unpinned block in IPFS: {}", cid);
 Ok(())
 }
 Err(e) => {
 bail!("Failed to unpin block {}: {}", cid, e)
 }
 }
 }

 /// Check if IPFS node is online
 pub async fn is_online(&self) -> bool {
 self.client.version().await.is_ok()
 }

 /// Get IPFS node version
 pub async fn get_version(&self) -> Result<String> {
 match self.client.version().await {
 Ok(version) => Ok(version.version),
 Err(e) => bail!("Failed to get IPFS version: {}", e),
 }
 }

 /// Get repository stats
 pub async fn get_repo_stats(&self) -> Result<RepoStats> {
 match self.client.stats_repo().await {
 Ok(stats) => Ok(RepoStats {
 num_objects: stats.num_objects,
 repo_size: stats.repo_size,
 storage_max: stats.storage_max,
 }),
 Err(e) => bail!("Failed to get repo stats: {}", e),
 }
 }
}

/// IPFS repository statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoStats {
 pub num_objects: u64,
 pub repo_size: u64,
 pub storage_max: u64,
}

#[cfg(test)]
mod tests {
 use super::*;

 #[tokio::test]
 #[ignore] // Only run if IPFS daemon is running
 async fn test_ipfs_store_retrieve() {
 let config = IpfsConfig::default();
 let client = IpfsClient::new(config).unwrap();

 // Check if IPFS is online
 if !client.is_online().await {
 println!("IPFS daemon not running, skipping test");
 return;
 }

 // Store data
 let test_data = b"Hello, IPFS from Ouroboros!";
 let cid = client.store_block(test_data).await.unwrap();

 assert!(!cid.is_empty());
 println!("Stored with CID: {}", cid);

 // Retrieve data
 let retrieved = client.retrieve_block(&cid).await.unwrap();
 assert_eq!(retrieved, test_data);

 println!(" IPFS store/retrieve test passed");
 }

 #[tokio::test]
 #[ignore]
 async fn test_ipfs_pin_unpin() {
 let config = IpfsConfig::default();
 let client = IpfsClient::new(config).unwrap();

 if !client.is_online().await {
 return;
 }

 let test_data = b"Pinned data";
 let cid = client.store_block(test_data).await.unwrap();

 // Pin
 client.pin_block(&cid).await.unwrap();

 // Unpin
 client.unpin_block(&cid).await.unwrap();

 println!(" IPFS pin/unpin test passed");
 }
}
