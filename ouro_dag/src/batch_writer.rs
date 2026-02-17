// src/batch_writer.rs
// High-performance batch transaction writer for TPS optimization
// Target: 20,000-50,000 TPS

use crate::dag::transaction::Transaction;
use anyhow::Result;
use chrono::Utc;
use log::{error, info, warn};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};
use uuid::Uuid;

/// Constant prefix for system transaction signatures.
/// System transactions are signed with HMAC-SHA256 using the node's BFT secret seed.
pub const SYSTEM_TX_PREFIX: &str = "system_hmac:";

/// Generate an HMAC signature for a system-generated transaction.
/// Uses SHA-256 with the BFT secret seed as the key.
pub fn sign_system_tx(tx_hash: &str, bft_seed: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(bft_seed.as_bytes())
        .expect("HMAC can take key of any size");
    mac.update(tx_hash.as_bytes());
    let result = mac.finalize();
    format!("{}{}", SYSTEM_TX_PREFIX, hex::encode(result.into_bytes()))
}

/// Verify a system transaction signature.
pub fn verify_system_tx(tx_hash: &str, signature: &str, bft_seed: &str) -> bool {
    if !signature.starts_with(SYSTEM_TX_PREFIX) {
        return false;
    }
    let expected = sign_system_tx(tx_hash, bft_seed);
    // Constant-time comparison to prevent timing attacks
    use subtle::ConstantTimeEq;
    expected.as_bytes().ct_eq(signature.as_bytes()).into()
}

const BATCH_SIZE: usize = 500; // Flush after 500 transactions
const FLUSH_INTERVAL_MS: u64 = 100; // Or flush every 100ms

#[derive(Debug, Clone)]
pub struct PendingTransaction {
    pub tx_id: Uuid,
    pub tx_hash: String,
    pub sender: String,
    pub recipient: String,
    pub payload: Value,
    pub signature: Option<String>,
    pub amount: u64,
    pub fee: u64,
    pub public_key: String,
}

pub struct BatchWriter {
    tx_sender: mpsc::Sender<PendingTransaction>,
    _processor_handle: tokio::task::JoinHandle<()>,
}

impl BatchWriter {
    /// Create a new BatchWriter and spawn the background flusher task
    pub fn new(rocks_db: crate::storage::RocksDb) -> Self {
        let (tx_sender, tx_receiver) = mpsc::channel::<PendingTransaction>(10000);

        // Spawn background task to process batches
        let processor_handle = tokio::spawn(async move {
            if let Err(e) = batch_processor(tx_receiver, rocks_db).await {
                error!("Batch processor error: {}", e);
            }
        });

        Self {
            tx_sender,
            _processor_handle: processor_handle,
        }
    }

    /// Submit a transaction for batch processing (non-blocking)
    pub async fn submit(&self, tx: PendingTransaction) -> Result<()> {
        self.tx_sender
            .send(tx)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to queue transaction: {}", e))?;
        Ok(())
    }
}

/// Background task that batches and flushes transactions
async fn batch_processor(
    mut rx: mpsc::Receiver<PendingTransaction>,
    rocks_db: crate::storage::RocksDb,
) -> Result<()> {
    let mut batch: Vec<PendingTransaction> = Vec::with_capacity(BATCH_SIZE);
    let mut flush_timer = interval(Duration::from_millis(FLUSH_INTERVAL_MS));

    info!(
        "Batch transaction processor started (batch_size={}, flush_interval={}ms)",
        BATCH_SIZE, FLUSH_INTERVAL_MS
    );

    loop {
        tokio::select! {
            // Receive new transaction
            Some(tx) = rx.recv() => {
                batch.push(tx);

                // Flush if batch is full
                if batch.len() >= BATCH_SIZE {
                    flush_batch(&mut batch, &rocks_db).await;
                }
            }

            // Periodic flush timer
            _ = flush_timer.tick() => {
                if !batch.is_empty() {
                    flush_batch(&mut batch, &rocks_db).await;
                }
            }
        }
    }
}

/// Flush a batch of transactions to RocksDB
async fn flush_batch(batch: &mut Vec<PendingTransaction>, rocks_db: &crate::storage::RocksDb) {
    if batch.is_empty() {
        return;
    }

    let batch_size = batch.len();
    let start = std::time::Instant::now();

    // Bulk insert into RocksDB mempool
    if let Err(e) = flush_to_rocks(batch, rocks_db) {
        warn!(
            "WARNING Failed to flush {} transactions to RocksDB: {}",
            batch_size, e
        );
        // Don't clear batch - will retry on next flush
        return;
    }

    let elapsed = start.elapsed();
    info!(
        "Flushed {} transactions in {:.2}ms ({:.0} TPS)",
        batch_size,
        elapsed.as_secs_f64() * 1000.0,
        batch_size as f64 / elapsed.as_secs_f64()
    );

    batch.clear();
}

/// Bulk insert transactions into PostgreSQL using UNNEST (DEPRECATED - use flush_to_rocks instead)
async fn flush_to_postgres(
    _batch: &[PendingTransaction],
    _db_pool: &crate::storage::RocksDb,
) -> Result<()> {
    // TODO_ROCKSDB: This function is deprecated, use flush_to_rocks instead
    Ok(())
}

/// Bulk insert transactions into RocksDB mempool
fn flush_to_rocks(batch: &[PendingTransaction], db: &crate::storage::RocksDb) -> Result<()> {
    let bft_seed = std::env::var("BFT_SECRET_SEED").unwrap_or_default();

    for tx in batch {
        // SECURITY: Verify system transactions have valid HMAC signatures.
        // Prevents forged "system" transactions from minting tokens.
        if tx.sender == "system" {
            let sig = tx.signature.as_deref().unwrap_or("");
            if !verify_system_tx(&tx.tx_hash, sig, &bft_seed) {
                warn!("SECURITY: Rejecting forged system transaction: {}", tx.tx_hash);
                continue;
            }
        }

        let dag_transaction = Transaction {
            id: tx.tx_id,
            sender: tx.sender.clone(),
            recipient: tx.recipient.clone(),
            amount: tx.amount,
            timestamp: Utc::now(),
            parents: vec![],
            signature: tx.signature.clone().unwrap_or_default(),
            public_key: tx.public_key.clone(),
            fee: tx.fee,
            payload: Some(tx.payload.to_string()),
            chain_id: "ouroboros-mainnet-1".to_string(), // Phase 6: replay protection
            nonce: 0,                                    // Phase 6: transaction ordering
        };

        let mempool_key = format!("mempool:{}", tx.tx_id);
        crate::storage::put(db, mempool_key.into_bytes(), &dag_transaction)
            .map_err(|e| anyhow::anyhow!("RocksDB put error: {}", e))?;
    }

    Ok(())
}
