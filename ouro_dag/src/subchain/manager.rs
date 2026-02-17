use crate::PgPool;
// src/subchain/manager.rs
use crate::crypto::verify_bytes;
use crate::subchain::messages::MicroAnchorLeaf;
use crate::subchain::store::{BatchRecord, SubBlockHeader, SubStore};
use anyhow::Result;
use chrono::Utc;
use parking_lot::Mutex as PLMutex;
use serde_json;
use std::fs;
use std::sync::Arc;
use url::Url;
use uuid::Uuid;

/// Lightweight Subchain manager that holds a SubStore and can accept BatchAnchor txs.
///
/// Accept semantics:
/// - If `serialized_leaves_ref` is supplied and a Postgres pool is configured (self.pg),
/// the manager will attempt to synchronously verify each leaf against the `microchains` table.
/// - The batch is persisted to the local SubStore always. If pg is present, an entry is written
/// into the `subchain_batches` table (best-effort).
pub struct SubchainManager {
    pub name: String,
    pub store: Arc<SubStore>,
    lock: Arc<PLMutex<()>>,
    pub pg: Option<Arc<PgPool>>,
}

impl SubchainManager {
    pub fn open(name: &str, pg: Option<Arc<PgPool>>) -> Result<Self> {
        let store = SubStore::open(name)?;
        Ok(Self {
            name: name.to_string(),
            store: Arc::new(store),
            lock: Arc::new(PLMutex::new(())),
            pg,
        })
    }

    /// Open manager with a pre-built SubStore (avoids opening on-disk path).
    pub fn open_with_store(name: &str, store: SubStore, pg: Option<Arc<PgPool>>) -> Result<Self> {
        Ok(Self {
            name: name.to_string(),
            store: Arc::new(store),
            lock: Arc::new(PLMutex::new(())),
            pg,
        })
    }

    /// Accept a BatchAnchor.
    /// - `batch_root` bytes
    /// - `aggregator` id string
    /// - `leaf_count` number of leaves
    /// - `serialized_leaves_ref` optional file:// URL to the serialized leaves JSON
    pub async fn accept_batch(
        &self,
        batch_root: Vec<u8>,
        aggregator: &str,
        leaf_count: usize,
        serialized_leaves_ref: Option<String>,
    ) -> Result<()> {
        let _g = self.lock.lock();

        // prepare SubStore record
        let rec = BatchRecord {
            batch_root: batch_root.clone(),
            aggregator: aggregator.to_string(),
            leaf_count,
            created_at: Utc::now(),
            serialized_leaves_ref: serialized_leaves_ref.clone(),
            verified: false,
        };

        // If caller provided serialized_leaves_ref and we have a DB pool, try to verify now.
        if let (Some(ref sref), Some(ref pool_arc)) = (&serialized_leaves_ref, &self.pg) {
            if sref.starts_with("file://") {
                // parse file:// URL and read JSON
                let u = Url::parse(sref)
                    .map_err(|e| anyhow::anyhow!("invalid serialized_leaves_ref url: {}", e))?;
                if u.scheme() != "file" {
                    return Err(anyhow::anyhow!("unsupported serialized_leaves_ref scheme"));
                }
                let path = u
                    .to_file_path()
                    .map_err(|_| anyhow::anyhow!("invalid file URL"))?;
                let data = fs::read_to_string(&path).map_err(|e| {
                    anyhow::anyhow!(
                        "failed reading serialized leaves file {}: {}",
                        path.display(),
                        e
                    )
                })?;
                let leaves: Vec<MicroAnchorLeaf> = serde_json::from_str(&data).map_err(|e| {
                    anyhow::anyhow!(
                        "failed parsing serialized leaves json {}: {}",
                        path.display(),
                        e
                    )
                })?;

                // verify each leaf signature using microchain pubkeys in DB
                let pool = &**pool_arc;
                for leaf in &leaves {
                    // canonical payload: microchain_id | height BE | micro_root | timestamp BE
                    let mut payload = Vec::new();
                    payload.extend_from_slice(leaf.microchain_id.as_bytes());
                    payload.extend_from_slice(&leaf.height.to_be_bytes());
                    payload.extend_from_slice(&leaf.micro_root);
                    payload.extend_from_slice(&leaf.timestamp.to_be_bytes());

                    // query pubkey for microchain
                    let security_key = format!("microchain_security:{}", leaf.microchain_id);
                    let mode: Option<crate::microchain::SecurityMode> = crate::storage::get(self.store.db(), security_key.as_bytes())
                        .map_err(|e| anyhow::anyhow!("DB error looking up microchain security: {}", e))?;

                    let pubkey_bytes = match mode {
                        Some(crate::microchain::SecurityMode::SingleOwner { owner_pubkey }) => {
                            hex::decode(&owner_pubkey).map_err(|_| anyhow::anyhow!("Invalid hex pubkey"))?
                        },
                        Some(crate::microchain::SecurityMode::Federated { .. }) => {
                            return Err(anyhow::anyhow!("Federated security mode not yet supported for batch verification"));
                        },
                        None => return Err(anyhow::anyhow!("Microchain {} not found or has no security mode", leaf.microchain_id)),
                    };

                    // verify signature
                    if !verify_bytes(&pubkey_bytes, &leaf.sig_micro, &payload) {
                         return Err(anyhow::anyhow!("Invalid signature for microchain {}", leaf.microchain_id));
                    }
                }
            }
        }

        // Persist to SubStore (local sled/rocks)
        if let Err(e) = self.store.put_batch(&rec.batch_root, &rec) {
            log::warn!("SubStore put_batch failed: {}", e);
        }
        Ok(())
    }

    /// Append a subchain header that may include batch roots.
    pub fn append_header(&self, hdr: SubBlockHeader) -> Result<()> {
        let _g = self.lock.lock();
        self.store.put_header(&hdr)?;
        Ok(())
    }
}
