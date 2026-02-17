// src/bin/aggregator_runner.rs
// Aggregator for bundling microchain leaves into batches
// Migrated from PostgreSQL to RocksDB

use anyhow::Result;
use chrono::Utc;
use log::{info, warn};
use ouro_dag::crypto::merkle::merkle_root_from_leaves_bytes;
use ouro_dag::storage;
use ouro_dag::subchain::messages::MicroAnchorLeaf;
use serde_json;
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    // Initialize RocksDB
    let db_path = env::var("ROCKSDB_PATH").unwrap_or_else(|_| "rocksdb_data".to_string());

    let db = storage::open_db(&db_path);

    // How many claims to aggregate in one run
    let max_claims = 1000usize;

    // Read provisional claims from RocksDB
    let claims: Vec<(Uuid, String, String, u64)> =
        storage::iter_prefix::<(Uuid, String, String, u64)>(&db, b"provisional_claim:")
            .unwrap_or_default()
            .into_iter()
            .filter(|(_, _, _, finalized)| *finalized == 0) // 0 = not finalized
            .take(max_claims)
            .collect();

    if claims.is_empty() {
        info!("No provisional claims to aggregate");
        return Ok(());
    }

    // Convert claims to leaves
    let mut leaves = Vec::<MicroAnchorLeaf>::new();
    let mut claim_ids: Vec<Uuid> = Vec::new();

    for (id, microchain_id_str, _owner, _amount) in &claims {
        let micro_id: Uuid = Uuid::parse_str(microchain_id_str).unwrap_or(Uuid::nil());

        let leaf = MicroAnchorLeaf {
            microchain_id: micro_id,
            height: 0u64,
            micro_root: vec![],
            timestamp: Utc::now().timestamp(),
            sig_micro: vec![],
            archive_url: None,
        };
        leaves.push(leaf);
        claim_ids.push(*id);
    }

    if leaves.is_empty() {
        warn!("No valid leaves parsed; aborting");
        return Ok(());
    }

    // Serialize leaves to file
    let file_name = format!("aggregated_leaves_{}.json", Uuid::new_v4());
    let mut out = PathBuf::from(env::current_dir()?);
    out.push(&file_name);
    let json = serde_json::to_string_pretty(&leaves)?;
    fs::write(&out, json.as_bytes())?;
    let file_url = format!("file://{}", out.to_string_lossy());

    // Update archive_url in each leaf
    for leaf in &mut leaves {
        leaf.archive_url = Some(file_url.clone());
    }

    // Rewrite file with archive_url populated
    let json = serde_json::to_string_pretty(&leaves)?;
    fs::write(&out, json.as_bytes())?;

    // Compute merkle root
    let leaves_bytes: Vec<Vec<u8>> = leaves
        .iter()
        .map(|l| serde_json::to_vec(l).unwrap_or_default())
        .collect();

    let root = match merkle_root_from_leaves_bytes(&leaves_bytes) {
        Ok(r) => r,
        Err(_) => {
            let mut hasher = Sha256::new();
            for b in &leaves_bytes {
                hasher.update(b);
            }
            hasher.finalize().to_vec()
        }
    };

    // Store batch in RocksDB
    let batch_id = Uuid::new_v4();
    let batch_key = format!("subchain_batch:{}", batch_id);

    #[derive(serde::Serialize)]
    struct BatchRecord {
        id: Uuid,
        batch_root: Vec<u8>,
        aggregator: String,
        leaf_count: usize,
        serialized_leaves_ref: String,
        created_at: i64,
        verified: bool,
    }

    let batch = BatchRecord {
        id: batch_id,
        batch_root: root.clone(),
        aggregator: "aggregator-runner".to_string(),
        leaf_count: leaves.len(),
        serialized_leaves_ref: file_url,
        created_at: Utc::now().timestamp(),
        verified: false,
    };

    storage::put(&db, batch_key.into_bytes(), &batch)
        .map_err(|e| anyhow::anyhow!("Failed to store batch: {}", e))?;

    // Mark claims as finalized
    for claim_id in &claim_ids {
        let claim_key = format!("provisional_claim:{}", claim_id);
        // Delete the claim (or mark as finalized)
        let _ = db.delete(claim_key.as_bytes());
    }

    info!(
        "Aggregated {} claims into batch {} (root {})",
        claim_ids.len(),
        batch_id,
        hex::encode(&root)
    );

    Ok(())
}
