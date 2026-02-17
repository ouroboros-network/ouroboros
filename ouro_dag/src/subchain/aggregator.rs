// src/subchain/aggregator.rs
use crate::subchain::anchor;
use crate::subchain::messages::BatchAnchor;
use crate::subchain::messages::MicroAnchorLeaf;
use anyhow::Result;
use reqwest::Client;
use serde_json;
use sha2::Digest;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

/// Aggregator collects micro-anchor leaves, writes them to disk, builds Merkle batch,
/// and posts to the subchain HTTP endpoint. No external cloud used.
pub struct Aggregator {
    pub id: String,
    pub http_client: Client,
    pub storage_dir: PathBuf,
}

impl Aggregator {
    pub fn new(id: &str) -> Self {
        let base_path = std::env::var("ROCKSDB_PATH")
            .or_else(|_| std::env::var("SLED_PATH"))
            .unwrap_or_else(|_| "sled_data".into());
        let mut p = PathBuf::from(&base_path);
        p.push("serialized_leaves");
        std::fs::create_dir_all(&p).expect("create serialized_leaves dir");
        Self {
            id: id.to_string(),
            http_client: Client::new(),
            storage_dir: p,
        }
    }

    /// Build batch from serialized leaves (the caller must ensure canonical ordering)
    /// and write leaves to file:// path. Return batch_root bytes.
    pub async fn build_and_submit_batch(
        &self,
        leaves: Vec<MicroAnchorLeaf>,
        subchain_endpoint: &str,
    ) -> Result<Vec<u8>> {
        // compute leaf bytes: canonical serialization H(serialized_leaf)
        let mut leaf_hashes: Vec<Vec<u8>> = Vec::with_capacity(leaves.len());
        for l in &leaves {
            let ser = serde_json::to_vec(l)?;
            let h = sha2::Sha256::digest(&ser).to_vec();
            leaf_hashes.push(h);
        }
        // compute merkle root
        let root = anchor::merkle_root(&leaf_hashes);

        // persist leaves as JSON file named by root hex
        let root_hex = hex::encode(&root);
        let mut file_path = self.storage_dir.clone();
        file_path.push(format!("{}.json", root_hex));
        let ser_all = serde_json::to_vec(&leaves)?;
        fs::write(&file_path, &ser_all)?;

        // create file:// URL for serialized_leaves_ref
        let file_url = format!("file://{}", file_path.to_string_lossy());

        let batch = BatchAnchor {
            batch_root: root.clone(),
            aggregator_id: self.id.clone(),
            leaf_count: leaves.len(),
            canonical_order: "provided".into(),
        };

        // POST to subchain endpoint: POST /subchain/batch_anchor with serialized_leaves_ref
        let url = format!("{}/batch_anchor", subchain_endpoint.trim_end_matches('/'));
        let resp = self
            .http_client
            .post(&url)
            .json(&serde_json::json!({
            "batch": batch,
            "serialized_leaves_ref": file_url
            }))
            .timeout(Duration::from_secs(10))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            anyhow::bail!("batch submit failed: {} - {}", status, txt);
        }

        Ok(root)
    }
}
