// src/data_availability/s3_backup.rs
//! S3-compatible backup storage for redundancy
//!
//! Provides cloud backup of block data for disaster recovery and archival

use anyhow::{Result, bail};
use aws_sdk_s3::Client as S3Client;
use aws_sdk_s3::primitives::ByteStream;
use aws_config::BehaviorVersion;
use serde::{Deserialize, Serialize};

/// S3 backup client
pub struct S3Backup {
 /// AWS S3 client
 client: S3Client,

 /// S3 bucket name
 bucket: String,

 /// Key prefix (e.g., "ouroboros/mainchain/")
 prefix: String,
}

/// S3 configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3Config {
 /// S3 bucket name
 pub bucket: String,

 /// Key prefix
 pub prefix: String,

 /// AWS region (e.g., "us-east-1")
 pub region: String,

 /// Optional endpoint URL (for S3-compatible services like MinIO)
 pub endpoint_url: Option<String>,
}

impl Default for S3Config {
 fn default() -> Self {
 Self {
 bucket: "ouroboros-blocks".to_string(),
 prefix: "mainchain/".to_string(),
 region: "us-east-1".to_string(),
 endpoint_url: None,
 }
 }
}

impl S3Backup {
 /// Create new S3 backup client
 pub async fn new(config: S3Config) -> Result<Self> {
 // Load AWS config
 let sdk_config = if let Some(endpoint) = &config.endpoint_url {
 // Custom endpoint (MinIO, LocalStack, etc.)
 aws_config::defaults(BehaviorVersion::latest())
 .region(aws_sdk_s3::config::Region::new(config.region.clone()))
 .endpoint_url(endpoint)
 .load()
 .await
 } else {
 // Standard AWS S3
 aws_config::defaults(BehaviorVersion::latest())
 .region(aws_sdk_s3::config::Region::new(config.region.clone()))
 .load()
 .await
 };

 let client = S3Client::new(&sdk_config);

 Ok(Self {
 client,
 bucket: config.bucket,
 prefix: config.prefix,
 })
 }

 /// Store block data in S3
 ///
 /// # Arguments
 /// * `block_hash` - Block hash (hex)
 /// * `block_data` - Serialized block data
 ///
 /// # Returns
 /// S3 key where data was stored
 pub async fn store_block(&self, block_hash: &str, block_data: &[u8]) -> Result<String> {
 let key = format!("{}{}", self.prefix, block_hash);

 let body = ByteStream::from(block_data.to_vec());

 match self.client
 .put_object()
 .bucket(&self.bucket)
 .key(&key)
 .body(body)
 .content_type("application/octet-stream")
 .send()
 .await
 {
 Ok(_) => {
 log::info!(" Stored block in S3: s3://{}/{} ({} bytes)",
 self.bucket, key, block_data.len());
 Ok(key)
 }
 Err(e) => {
 bail!("Failed to store block in S3: {}", e)
 }
 }
 }

 /// Retrieve block data from S3
 pub async fn retrieve_block(&self, block_hash: &str) -> Result<Vec<u8>> {
 let key = format!("{}{}", self.prefix, block_hash);

 match self.client
 .get_object()
 .bucket(&self.bucket)
 .key(&key)
 .send()
 .await
 {
 Ok(response) => {
 let data = response.body.collect().await
 .map_err(|e| anyhow::anyhow!("Failed to read S3 response: {}", e))?
 .into_bytes()
 .to_vec();

 log::info!(" Retrieved block from S3: {} ({} bytes)", key, data.len());
 Ok(data)
 }
 Err(e) => {
 bail!("Failed to retrieve block from S3 {}: {}", key, e)
 }
 }
 }

 /// Check if block exists in S3
 pub async fn block_exists(&self, block_hash: &str) -> bool {
 let key = format!("{}{}", self.prefix, block_hash);

 self.client
 .head_object()
 .bucket(&self.bucket)
 .key(&key)
 .send()
 .await
 .is_ok()
 }

 /// Delete block from S3
 pub async fn delete_block(&self, block_hash: &str) -> Result<()> {
 let key = format!("{}{}", self.prefix, block_hash);

 match self.client
 .delete_object()
 .bucket(&self.bucket)
 .key(&key)
 .send()
 .await
 {
 Ok(_) => {
 log::info!(" Deleted block from S3: {}", key);
 Ok(())
 }
 Err(e) => {
 bail!("Failed to delete block from S3: {}", e)
 }
 }
 }

 /// List all blocks with prefix
 pub async fn list_blocks(&self, max_keys: Option<i32>) -> Result<Vec<String>> {
 let mut request = self.client
 .list_objects_v2()
 .bucket(&self.bucket)
 .prefix(&self.prefix);

 if let Some(max) = max_keys {
 request = request.max_keys(max);
 }

 match request.send().await {
 Ok(response) => {
 let keys = response
 .contents()
 .iter()
 .filter_map(|obj| obj.key().map(String::from))
 .collect();
 Ok(keys)
 }
 Err(e) => {
 bail!("Failed to list blocks from S3: {}", e)
 }
 }
 }

 /// Get bucket storage stats
 pub async fn get_storage_stats(&self) -> Result<StorageStats> {
 let objects = self.list_blocks(None).await?;

 let mut total_size = 0u64;
 for key in &objects {
 if let Ok(response) = self.client
 .head_object()
 .bucket(&self.bucket)
 .key(key)
 .send()
 .await
 {
 total_size += response.content_length().unwrap_or(0) as u64;
 }
 }

 Ok(StorageStats {
 num_objects: objects.len() as u64,
 total_size_bytes: total_size,
 })
 }
}

/// S3 storage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageStats {
 pub num_objects: u64,
 pub total_size_bytes: u64,
}

#[cfg(test)]
mod tests {
 use super::*;

 #[tokio::test]
 #[ignore] // Only run with AWS credentials configured
 async fn test_s3_store_retrieve() {
 let config = S3Config {
 bucket: "test-ouroboros-blocks".to_string(),
 prefix: "test/".to_string(),
 region: "us-east-1".to_string(),
 endpoint_url: None,
 };

 let s3 = S3Backup::new(config).await.unwrap();

 // Store data
 let test_data = b"Hello, S3 from Ouroboros!";
 let block_hash = "test_block_123";
 let key = s3.store_block(block_hash, test_data).await.unwrap();

 assert!(!key.is_empty());
 println!("Stored with key: {}", key);

 // Retrieve data
 let retrieved = s3.retrieve_block(block_hash).await.unwrap();
 assert_eq!(retrieved, test_data);

 // Cleanup
 s3.delete_block(block_hash).await.unwrap();

 println!(" S3 store/retrieve test passed");
 }
}
