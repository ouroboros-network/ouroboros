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
        println!("CONFIG: DEBUG: BatchWriter::new() called");
        let (tx_sender, tx_receiver) = mpsc::channel::<PendingTransaction>(10000);
        println!("CONFIG: DEBUG: Channel created with capacity 10000");

        // Spawn background task to process batches (keep JoinHandle to prevent task from being dropped)
        println!("CONFIG: DEBUG: About to spawn batch_processor task");
        let processor_handle = tokio::spawn(async move {
            println!("CONFIG: DEBUG: Inside spawned task closure");
            if let Err(e) = batch_processor(tx_receiver, rocks_db).await {
                error!("ERROR Batch processor error: {}", e);
                println!("ERROR Batch processor error: {}", e);
            }
        });
        println!("CONFIG: DEBUG: tokio::spawn returned (task spawned)");

        Self {
            tx_sender,
            _processor_handle: processor_handle,
        }
    }

    /// Submit a transaction for batch processing (non-blocking)
    pub async fn submit(&self, tx: PendingTransaction) -> Result<()> {
        println!(" BatchWriter.submit() called for tx: {}", tx.tx_hash);
        self.tx_sender
            .send(tx)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to queue transaction: {}", e))?;
        println!(" Transaction queued successfully");
        Ok(())
    }
}

/// Background task that batches and flushes transactions
async fn batch_processor(
    mut rx: mpsc::Receiver<PendingTransaction>,
    rocks_db: crate::storage::RocksDb,
) -> Result<()> {
    println!("CONFIG: DEBUG: batch_processor() function called - ENTRY POINT");

    let mut batch: Vec<PendingTransaction> = Vec::with_capacity(BATCH_SIZE);
    println!("CONFIG: DEBUG: Batch vector created");

    let mut flush_timer = interval(Duration::from_millis(FLUSH_INTERVAL_MS));
    println!("CONFIG: DEBUG: Flush timer created");

    info!(
        "STARTING: Batch transaction processor started (batch_size={}, flush_interval={}ms)",
        BATCH_SIZE, FLUSH_INTERVAL_MS
    );
    println!(
        "STARTING: Batch transaction processor started (batch_size={}, flush_interval={}ms)",
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
    for tx in batch {
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
