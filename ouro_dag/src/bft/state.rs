use crate::PgPool;
// src/bft/state.rs
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

/// Equivocation evidence record
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Equivocation {
    pub validator: String,
    pub round: u64,
    pub existing: String,
    pub conflicting: String,
    pub observed_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Error, Debug)]
pub enum BFTStateError {
    #[error("equivocation: {0:?}")]
    Equivocation(Equivocation),
    #[error("db error: {0}")]
    Db(String),
}

/// Maximum number of rounds to keep in the seen_signatures map.
/// Older entries are pruned to prevent unbounded memory growth.
const MAX_SIGNATURE_HISTORY_ROUNDS: u64 = 1000;

/// Lightweight BFT state helper. Thread-safe for async contexts.
pub struct BFTState {
    /// key: (validator_id, round) -> block_hash
    pub seen_signatures: AsyncMutex<HashMap<(String, u64), String>>,
    /// optional DB pool for persisting evidence or looking up pubkeys
    pub db_pool: Option<PgPool>,
}

impl BFTState {
    /// Construct with a `PgPool`. If you don't have a DB in your environment pass `None`.
    pub fn new(pool: PgPool) -> Self {
        BFTState {
            seen_signatures: AsyncMutex::new(HashMap::new()),
            db_pool: Some(pool),
        }
    }

    /// Construct without DB.
    pub fn new_no_db() -> Self {
        BFTState {
            seen_signatures: AsyncMutex::new(HashMap::new()),
            db_pool: None,
        }
    }

    /// Return an Option reference to the PgPool for best-effort lookups.
    pub fn get_pg_pool_option(&self) -> Option<&PgPool> {
        self.db_pool.as_ref()
    }

    /// Record a signature seen for (validator, round) and detect equivocation.
    /// On DB errors returns `BFTStateError::Db`.
    pub async fn record_signature(
        &self,
        validator: &str,
        round: u64,
        block_hash: &str,
    ) -> Result<(), BFTStateError> {
        let key = (validator.to_string(), round);
        let mut map = self.seen_signatures.lock().await;
        if let Some(existing) = map.get(&key) {
            if existing != block_hash {
                let ev = Equivocation {
                    validator: validator.to_string(),
                    round,
                    existing: existing.clone(),
                    conflicting: block_hash.to_string(),
                    observed_at: Utc::now(),
                };
                // try persist (if DB exists); on DB error return it wrapped
                if let Some(pg) = &self.db_pool {
                    self.persist_evidence(&ev, pg).await?;
                }
                return Err(BFTStateError::Equivocation(ev));
            } else {
                return Ok(());
            }
        }
        map.insert(key, block_hash.to_string());

        // Prune old entries to prevent unbounded memory growth (C5 fix).
        // Remove all entries for rounds older than (current_round - MAX_HISTORY).
        if round > MAX_SIGNATURE_HISTORY_ROUNDS {
            let cutoff = round - MAX_SIGNATURE_HISTORY_ROUNDS;
            map.retain(|(_v, r), _| *r >= cutoff);
        }

        Ok(())
    }

    /// Persist equivocation evidence to database
    async fn persist_evidence(
        &self,
        _evidence: &Equivocation,
        _pg: &PgPool,
    ) -> Result<(), BFTStateError> {
        // TODO_ROCKSDB: Implement evidence persistence to RocksDB
        Ok(())
    }
}
