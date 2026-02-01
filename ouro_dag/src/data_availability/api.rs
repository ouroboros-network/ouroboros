// src/data_availability/api.rs
//! REST API endpoints for data availability layer
//!
//! Provides HTTP access to IPFS, S3, and archival storage

use axum::{
 extract::{Path, State},
 http::StatusCode,
 response::Json,
 routing::{get, post},
 Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use super::{DataAvailability, StoreResult, ArchiveResult};

/// API state shared across handlers
#[derive(Clone)]
pub struct DataAvailabilityState {
 pub da: Arc<DataAvailability>,
}

/// Create router for data availability endpoints
pub fn router(da: Arc<DataAvailability>) -> Router {
 let state = DataAvailabilityState { da };

 Router::new()
 .route("/data/:block_hash", get(retrieve_block))
 .route("/data/:block_hash/metadata", get(get_block_metadata))
 .route("/data/archive/:block_hash", post(archive_block))
 .route("/archival/stats", get(get_archival_stats))
 .with_state(state)
}

/// Retrieve block data from DA layer
///
/// GET /data/:block_hash?cid=optional_ipfs_cid
#[derive(Debug, Deserialize)]
struct RetrieveQuery {
 cid: Option<String>,
}

async fn retrieve_block(
 State(state): State<DataAvailabilityState>,
 Path(block_hash): Path<String>,
 query: Option<axum::extract::Query<RetrieveQuery>>,
) -> Result<Json<RetrieveBlockResponse>, (StatusCode, String)> {
 let cid = query.and_then(|q| q.cid.as_deref().map(String::from));

 match state.da.retrieve_block(&block_hash, cid.as_deref()).await {
 Ok(data) => Ok(Json(RetrieveBlockResponse {
 success: true,
 block_hash: block_hash.clone(),
 data_hex: hex::encode(&data),
 data_size: data.len(),
 error: None,
 })),
 Err(e) => Err((
 StatusCode::NOT_FOUND,
 format!("Block not found: {}", e),
 )),
 }
}

#[derive(Debug, Serialize)]
struct RetrieveBlockResponse {
 pub success: bool,
 pub block_hash: String,
 pub data_hex: String,
 pub data_size: usize,
 pub error: Option<String>,
}

/// Get metadata about block storage locations
///
/// GET /data/:block_hash/metadata
async fn get_block_metadata(
 State(state): State<DataAvailabilityState>,
 Path(block_hash): Path<String>,
) -> Result<Json<BlockMetadata>, (StatusCode, String)> {
 // Try to retrieve from IPFS first to get CID
 let ipfs_cid = if let Some(ref ipfs) = state.da.ipfs {
 // We don't have the CID stored, so this is a limitation
 // In production, you'd store CID in database
 None
 } else {
 None
 };

 // Check if exists in S3
 let s3_exists = if let Some(ref s3) = state.da.s3 {
 s3.block_exists(&block_hash).await
 } else {
 false
 };

 Ok(Json(BlockMetadata {
 block_hash: block_hash.clone(),
 ipfs_cid,
 s3_exists,
 locations: vec![
 if state.da.ipfs.is_some() { Some("IPFS".to_string()) } else { None },
 if state.da.s3.is_some() { Some("S3".to_string()) } else { None },
 ]
 .into_iter()
 .flatten()
 .collect(),
 }))
}

#[derive(Debug, Serialize)]
struct BlockMetadata {
 pub block_hash: String,
 pub ipfs_cid: Option<String>,
 pub s3_exists: bool,
 pub locations: Vec<String>,
}

/// Manually trigger block archival
///
/// POST /data/archive/:block_hash
/// Body: { "block_data_hex": "..." }
#[derive(Debug, Deserialize)]
struct ArchiveRequest {
 pub block_data_hex: String,
}

async fn archive_block(
 State(state): State<DataAvailabilityState>,
 Path(block_hash): Path<String>,
 Json(payload): Json<ArchiveRequest>,
) -> Result<Json<ArchiveBlockResponse>, (StatusCode, String)> {
 // Decode hex block data
 let block_data = hex::decode(&payload.block_data_hex)
 .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid hex data: {}", e)))?;

 // Archive the block
 match state.da.archive_block(&block_hash, &block_data).await {
 Ok(result) => Ok(Json(ArchiveBlockResponse {
 success: true,
 block_hash: block_hash.clone(),
 ipfs_cid: result.ipfs_cid,
 s3_key: result.s3_key,
 error: result.error,
 })),
 Err(e) => Err((
 StatusCode::INTERNAL_SERVER_ERROR,
 format!("Archive failed: {}", e),
 )),
 }
}

#[derive(Debug, Serialize)]
struct ArchiveBlockResponse {
 pub success: bool,
 pub block_hash: String,
 pub ipfs_cid: Option<String>,
 pub s3_key: Option<String>,
 pub error: Option<String>,
}

/// Get archival node statistics
///
/// GET /archival/stats
async fn get_archival_stats(
 State(state): State<DataAvailabilityState>,
) -> Result<Json<ArchivalStatsResponse>, (StatusCode, String)> {
 let mut stats = ArchivalStatsResponse {
 archival_enabled: state.da.archival().config.enabled,
 ipfs_enabled: state.da.archival().config.use_ipfs,
 s3_enabled: state.da.archival().config.use_s3,
 retention_days: state.da.archival().config.retention_days,
 pruning_enabled: state.da.archival().config.enable_pruning,
 ipfs_stats: None,
 s3_stats: None,
 };

 // Get IPFS stats if available
 if let Some(ref ipfs) = state.da.ipfs {
 if ipfs.is_online().await {
 match ipfs.get_repo_stats().await {
 Ok(repo_stats) => {
 stats.ipfs_stats = Some(IpfsStats {
 online: true,
 num_objects: repo_stats.num_objects,
 repo_size: repo_stats.repo_size,
 storage_max: repo_stats.storage_max,
 });
 }
 Err(_) => {
 stats.ipfs_stats = Some(IpfsStats {
 online: false,
 num_objects: 0,
 repo_size: 0,
 storage_max: 0,
 });
 }
 }
 }
 }

 // Get S3 stats if available
 if let Some(ref s3) = state.da.s3 {
 match s3.get_storage_stats().await {
 Ok(s3_stats) => {
 stats.s3_stats = Some(S3Stats {
 num_objects: s3_stats.num_objects,
 total_size_bytes: s3_stats.total_size_bytes,
 });
 }
 Err(_) => {
 stats.s3_stats = Some(S3Stats {
 num_objects: 0,
 total_size_bytes: 0,
 });
 }
 }
 }

 Ok(Json(stats))
}

#[derive(Debug, Serialize)]
struct ArchivalStatsResponse {
 pub archival_enabled: bool,
 pub ipfs_enabled: bool,
 pub s3_enabled: bool,
 pub retention_days: u64,
 pub pruning_enabled: bool,
 pub ipfs_stats: Option<IpfsStats>,
 pub s3_stats: Option<S3Stats>,
}

#[derive(Debug, Serialize)]
struct IpfsStats {
 pub online: bool,
 pub num_objects: u64,
 pub repo_size: u64,
 pub storage_max: u64,
}

#[derive(Debug, Serialize)]
struct S3Stats {
 pub num_objects: u64,
 pub total_size_bytes: u64,
}

#[cfg(test)]
mod tests {
 use super::*;
 use crate::data_availability::{
 ipfs::{IpfsClient, IpfsConfig},
 s3_backup::{S3Backup, S3Config},
 archival::ArchivalConfig,
 };

 #[tokio::test]
 async fn test_api_routes_compile() {
 // Just test that router compiles
 let ipfs_config = IpfsConfig::default();
 let ipfs = IpfsClient::new(ipfs_config).ok().map(Arc::new);
 let archival_config = ArchivalConfig::default();

 let da = Arc::new(DataAvailability::new(ipfs, None, archival_config));
 let _router = router(da);
 }
}
