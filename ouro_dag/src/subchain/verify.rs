// src/subchain/verify.rs
// Subchain batch verifier - migrated from PostgreSQL to RocksDB

use anyhow::Result;
use std::time::Duration;
use tokio::time::sleep;
use crate::subchain::store::SubStore;
use crate::subchain::manager::SubchainManager;
use crate::subchain::messages::MicroAnchorLeaf;
use crate::storage::{self, RocksDb};
use crate::PgPool;
use crate::keys;
use std::sync::Arc;
use url::Url;
use tokio::fs;
use uuid::Uuid;
use chrono::Utc;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BatchRecord {
    id: Uuid,
    subchain: Uuid,
    batch_root: Vec<u8>,
    serialized_leaves_ref: Option<String>,
    verified: bool,
    verified_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MicrochainRecord {
    id: Uuid,
    pubkey: Option<String>,
}

/// Background verifier: polls subchain_batches for unverified batches that have a serialized_leaves_ref.
/// Supports only file:// refs in this implementation.
/// On success marks batch as verified in both the SubStore and RocksDB.
pub async fn run_verifier(db: PgPool, _substore: Arc<SubStore>, manager: Arc<SubchainManager>, poll_interval_secs: u64) -> Result<()> {
    loop {
        // Fetch unverified batches with a serialized_leaves_ref from RocksDB
        let batches: Vec<BatchRecord> = storage::iter_prefix(&db, b"subchain_batch:")
            .unwrap_or_default()
            .into_iter()
            .filter(|b: &BatchRecord| b.serialized_leaves_ref.is_some() && !b.verified)
            .take(20)
            .collect();

        for mut batch in batches {
            let serialized_ref = match batch.serialized_leaves_ref.clone() {
                Some(s) => s,
                None => continue,
            };

            // Support file:// only for now
            if !serialized_ref.starts_with("file://") {
                log::warn!("Unsupported serialized_leaves_ref {}", serialized_ref);
                continue;
            }

            let url = match Url::parse(&serialized_ref) {
                Ok(u) => u,
                Err(e) => {
                    log::error!("Invalid serialized_leaves_ref url {}: {}", serialized_ref, e);
                    continue;
                }
            };

            let path = match url.to_file_path() {
                Ok(p) => p,
                Err(_) => {
                    log::error!("Failed to convert file URL to path: {}", serialized_ref);
                    continue;
                }
            };

            let data = match fs::read_to_string(&path).await {
                Ok(d) => d,
                Err(e) => {
                    log::error!("Failed to read serialized leaves file {}: {}", path.display(), e);
                    continue;
                }
            };

            let leaves: Vec<MicroAnchorLeaf> = match serde_json::from_str(&data) {
                Ok(v) => v,
                Err(e) => {
                    log::error!("Failed to parse serialized leaves json {}: {}", path.display(), e);
                    continue;
                }
            };

            // Verify each leaf signature against pubkeys in RocksDB
            let mut all_ok = true;
            for leaf in &leaves {
                // Canonical payload: microchain_id | height BE | micro_root | timestamp BE
                let mut payload = Vec::new();
                payload.extend_from_slice(leaf.microchain_id.as_bytes());
                payload.extend_from_slice(&leaf.height.to_be_bytes());
                payload.extend_from_slice(&leaf.micro_root);
                payload.extend_from_slice(&leaf.timestamp.to_be_bytes());

                // Get pubkey from RocksDB
                let microchain_key = format!("microchain:{}", leaf.microchain_id);
                let microchain: Option<MicrochainRecord> = storage::get_str(&db, &microchain_key)
                    .unwrap_or(None);

                let pubkey = match microchain.and_then(|m| m.pubkey) {
                    Some(p) => p,
                    None => {
                        log::warn!("No pubkey registered for microchain {}", leaf.microchain_id);
                        all_ok = false;
                        break;
                    }
                };

                if !keys::verify_bytes(&pubkey, &payload, &leaf.sig_micro) {
                    log::warn!("Invalid signature for microchain {} leaf height {}", leaf.microchain_id, leaf.height);
                    all_ok = false;
                    break;
                }
            }

            if all_ok {
                // Mark verified in RocksDB
                batch.verified = true;
                batch.verified_at = Some(Utc::now().timestamp());

                let batch_key = format!("subchain_batch:{}", batch.id);
                if let Err(e) = storage::put_str(&db, &batch_key, &batch) {
                    log::error!("Failed to update batch verification status: {}", e);
                    continue;
                }

                // Try to persist into SubStore (best-effort)
                let _ = manager.store.put_batch(&batch.batch_root, &crate::subchain::store::BatchRecord {
                    batch_root: batch.batch_root.clone(),
                    aggregator: "verifier".into(),
                    leaf_count: leaves.len(),
                    created_at: Utc::now(),
                    serialized_leaves_ref: Some(serialized_ref.clone()),
                    verified: true,
                });

                log::info!("Verified batch root {}", hex::encode(&batch.batch_root));
            } else {
                log::warn!("Batch {} failed verification", hex::encode(&batch.batch_root));
            }
        }

        sleep(Duration::from_secs(poll_interval_secs)).await;
    }
}
