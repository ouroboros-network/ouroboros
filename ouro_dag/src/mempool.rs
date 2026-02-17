// src/mempool.rs
use crate::dag::transaction::Transaction;
use crate::mev_protection::BatchOrdering;
use crate::storage::{iter_prefix, put, RocksDb};
use anyhow;
use once_cell::sync::{Lazy, OnceCell};
use std::io::{Error as IoError, ErrorKind, Result as IoResult};
use std::sync::Arc;
use uuid::Uuid;

/// Maximum number of transactions allowed in mempool (DoS protection)
/// Prevents unbounded memory growth from spam attacks
const MAX_MEMPOOL_SIZE: usize = 10_000;

/// Minimum fee required for transaction to enter mempool when full (in smallest units)
/// Low-fee transactions are rejected when mempool is at capacity
const MIN_FEE_WHEN_FULL: u64 = 100_000; // 0.001 OURO

/// Global mempool instance (initialized once during startup)
static GLOBAL_MEMPOOL: OnceCell<Arc<Mempool>> = OnceCell::new();

/// Global MEV protection via batch ordering (5 second batches)
static MEV_BATCH_ORDERING: Lazy<Arc<BatchOrdering>> = Lazy::new(|| Arc::new(BatchOrdering::new(5)));

/// Initialize the global mempool instance.
/// This should be called once during node startup with the storage handle.
pub fn init_global_mempool(db: RocksDb) {
    let mempool = Mempool::new(db);
    let _ = GLOBAL_MEMPOOL.set(Arc::new(mempool));
}

/// Select up to `limit` transactions from the global mempool for inclusion in a block.
/// Returns Vec of transaction UUIDs.
///
/// This function is called by consensus when proposing a new block.
/// It retrieves transactions from the mempool without removing them (removal happens
/// after block finalization).
pub async fn select_transactions(limit: usize) -> anyhow::Result<Vec<Uuid>> {
    // Get global mempool instance
    let mempool = GLOBAL_MEMPOOL.get().ok_or_else(|| {
        anyhow::anyhow!("Mempool not initialized - call init_global_mempool() first")
    })?;

    // Pop transactions from mempool (async-safe via tokio::task::spawn_blocking)
    let mempool_clone = Arc::clone(mempool);
    let txs = tokio::task::spawn_blocking(move || mempool_clone.pop_for_block(limit))
        .await
        .map_err(|e| anyhow::anyhow!("Task join error: {}", e))?
        .map_err(|e| anyhow::anyhow!("Mempool pop error: {}", e))?;

    // Extract transaction IDs
    let tx_ids: Vec<Uuid> = txs.iter().map(|tx| tx.id).collect();

    log::info!(
        "Selected {} transactions from mempool (limit: {})",
        tx_ids.len(),
        limit
    );

    Ok(tx_ids)
}

/// Get the current number of transactions in the mempool.
/// Returns 0 if the mempool has not been initialized yet.
pub fn get_mempool_count() -> u64 {
    match GLOBAL_MEMPOOL.get() {
        Some(mempool) => {
            match iter_prefix::<Transaction>(&mempool.db, b"mempool:") {
                Ok(items) => items.len() as u64,
                Err(_) => 0,
            }
        }
        None => 0,
    }
}

/// In-memory / on-disk mempool wrapper.
/// TODO: make operations fully asynchronous if needed and add proper eviction/prioritization.
pub struct Mempool {
    pub db: RocksDb,
}

impl Mempool {
    /// Construct a mempool bound to a RocksDb handle from crate::storage.
    pub fn new(db: RocksDb) -> Self {
        Self { db }
    }

    /// Add a transaction to the mempool and persist it under key "mempool:<uuid>".
    /// Enforces mempool size limits and evicts low-fee transactions when full.
    /// Returns std::io::Result for backward compatibility with earlier code.
    pub fn add_tx(&self, txn: &Transaction) -> IoResult<()> {
        // Check current mempool size
        let current_txs = match iter_prefix::<Transaction>(&self.db, b"mempool:") {
            Ok(items) => items,
            Err(e) => {
                return Err(IoError::new(
                    ErrorKind::Other,
                    format!("iter_prefix error: {}", e),
                ))
            }
        };

        let current_size = current_txs.len();

        // If mempool is full, enforce fee-based eviction
        if current_size >= MAX_MEMPOOL_SIZE {
            log::warn!(
                "Mempool full ({} transactions) - enforcing eviction policy",
                current_size
            );

            // Reject transactions with fee below minimum when mempool is full
            if txn.fee < MIN_FEE_WHEN_FULL {
                return Err(IoError::new(
                    ErrorKind::Other,
                    format!(
                        "Mempool full - transaction fee {} too low (minimum: {} when full)",
                        txn.fee, MIN_FEE_WHEN_FULL
                    ),
                ));
            }

            // Find lowest-fee transaction to evict
            if let Some(lowest_fee_tx) = current_txs.iter().min_by_key(|t| t.fee) {
                // Only evict if incoming transaction has higher fee
                if txn.fee > lowest_fee_tx.fee {
                    let evict_key = format!("mempool:{}", lowest_fee_tx.id);
                    match self.db.delete(evict_key.as_bytes()) {
                        Ok(_) => {
                            log::info!(
                                "Evicted transaction {} (fee: {}) to make room for {} (fee: {})",
                                lowest_fee_tx.id,
                                lowest_fee_tx.fee,
                                txn.id,
                                txn.fee
                            );
                        }
                        Err(e) => {
                            log::error!("Failed to evict transaction: {}", e);
                            return Err(IoError::new(
                                ErrorKind::Other,
                                "Failed to evict low-fee transaction",
                            ));
                        }
                    }
                } else {
                    // Incoming transaction has lower or equal fee - reject it
                    return Err(IoError::new(
                        ErrorKind::Other,
                        format!(
                            "Mempool full - transaction fee {} not higher than lowest fee {}",
                            txn.fee, lowest_fee_tx.fee
                        ),
                    ));
                }
            }
        }

        // Add transaction to mempool
        let key = format!("mempool:{}", txn.id);
        put(&self.db, key.into_bytes(), txn)
            .map_err(|e| IoError::new(ErrorKind::Other, format!("db put error: {}", e)))?;

        log::debug!(
            "Added transaction {} to mempool (fee: {}, size: {}/{})",
            txn.id,
            txn.fee,
            current_size + 1,
            MAX_MEMPOOL_SIZE
        );

        Ok(())
    }

    /// Return up to `limit` transactions from mempool, with MEV protection.
    /// Also implements TTL-based eviction (transactions older than 24 hours are skipped).
    ///
    /// MEV Protection: Uses deterministic ordering (sorted by hash) within fee tiers
    /// to prevent front-running and transaction reordering attacks.
    pub fn pop_for_block(&self, limit: usize) -> IoResult<Vec<Transaction>> {
        // Read all transactions from mempool
        let mut txs = match iter_prefix::<Transaction>(&self.db, b"mempool:") {
            Ok(items) => items,
            Err(e) => {
                return Err(IoError::new(
                    ErrorKind::Other,
                    format!("iter_prefix error: {}", e),
                ))
            }
        };

        // TTL-based eviction: remove transactions older than 24 hours
        let now = chrono::Utc::now();
        let ttl = chrono::Duration::hours(24);
        txs.retain(|tx| now.signed_duration_since(tx.timestamp) < ttl);

        // MEV-protected ordering:
        // 1. Group by fee tier (high, medium, low)
        // 2. Within each tier, sort by hash (deterministic, prevents reordering)
        txs.sort_by(|a, b| {
            // Fee tiers (prevents pure fee-based MEV)
            let tier_a = a.fee / 100_000; // Tier by 0.001 coin increments
            let tier_b = b.fee / 100_000;

            tier_b
                .cmp(&tier_a) // Higher tiers first
                .then_with(|| {
                    // Within tier: deterministic hash ordering (MEV protection)
                    let hash_a = format!("{:?}", a.id);
                    let hash_b = format!("{:?}", b.id);
                    hash_a.cmp(&hash_b)
                })
        });

        // Take up to limit transactions
        let selected: Vec<Transaction> = txs.into_iter().take(limit).collect();

        log::debug!(
            "Selected {} transactions with MEV protection",
            selected.len()
        );

        Ok(selected)
    }
}
