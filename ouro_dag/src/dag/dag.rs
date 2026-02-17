use super::transaction::Transaction;
#[allow(deprecated)]
use super::validation::validate_transaction_legacy;
use crate::storage::{iter_prefix, put, RocksDb};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Write;
use uuid::Uuid;

pub struct DAG {
    pub transactions: HashMap<Uuid, Transaction>,
    pub db: RocksDb,
}

#[derive(Serialize)]
struct ExportedTxn {
    sender: String,
    recipient: String,
    amount: u64,
}

#[derive(Serialize)]
struct ExportedState {
    balances: HashMap<String, u64>,
    transactions: Vec<ExportedTxn>,
}

impl DAG {
    // New constructor requires a RocksDb (sled-backed) handle.
    pub fn new(db: RocksDb) -> Self {
        let mut transactions = HashMap::new();

        // Load persisted transactions from storage using the prefix "txn:"
        if let Ok(stored) = iter_prefix::<Transaction>(&db, b"txn:") {
            for txn in stored {
                transactions.insert(txn.id, txn);
            }
            println!("Loaded {} transactions from DB", transactions.len());
        } else {
            println!("No persisted transactions found or failed to read prefix");
        }

        DAG { transactions, db }
    }

    // TODO: Update to use validate_transaction() with full security checks
    // This requires passing sender_balance, sender_nonce, and chain_id from state
    #[allow(deprecated)]
    pub fn add_transaction(&mut self, txn: Transaction) -> Result<(), String> {
        let existing_ids: HashSet<_> = self.transactions.keys().cloned().collect();
        validate_transaction_legacy(&txn, &existing_ids)?;

        // Persist to DB as JSON under key "txn:<uuid>"
        let key = format!("txn:{}", txn.id);
        put(&self.db, key.into_bytes(), &txn)?;

        // Insert into in-memory cache
        self.transactions.insert(txn.id, txn);
        Ok(())
    }

    pub fn print_dag(&self) {
        for (id, txn) in &self.transactions {
            println!(
                "Txn ID: {}, From: {}, To: {}, Amount: {}, Parents: {:?}",
                id, txn.sender, txn.recipient, txn.amount, txn.parents
            );
        }
    }

    pub fn export_state(&self) -> std::io::Result<()> {
        let mut balances = HashMap::new();
        let mut transactions = vec![];

        for txn in self.transactions.values() {
            let sender_balance = balances.entry(txn.sender.clone()).or_insert(0u64);
            *sender_balance = sender_balance.saturating_sub(txn.amount);
            *balances.entry(txn.recipient.clone()).or_insert(0u64) += txn.amount;

            transactions.push(ExportedTxn {
                sender: txn.sender.clone(),
                recipient: txn.recipient.clone(),
                amount: txn.amount,
            });
        }

        let state = ExportedState {
            balances,
            transactions,
        };

        let json = serde_json::to_string_pretty(&state)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let mut file = File::create("dag_state.json")?;
        file.write_all(json.as_bytes())?;
        Ok(())
    }
}

// src/dag/dag.rs (append)
use crate::bft::consensus::Block;
use anyhow::Result;
use once_cell::sync::Lazy;
use rocksdb::{Options, DB};
use std::path::PathBuf;
use std::sync::Arc;

/// Global blocks DB - opened once, reused to prevent file descriptor exhaustion
static BLOCKS_DB: Lazy<Arc<DB>> = Lazy::new(|| {
    let base_path = std::env::var("ROCKSDB_PATH").unwrap_or_else(|_| "rocksdb_data".into());
    let p = format!("{}_blocks", base_path);
    std::fs::create_dir_all(&p).expect("create blocks dir");

    let mut opts = Options::default();
    opts.create_if_missing(true);
    opts.set_max_open_files(256);

    let db = DB::open(&opts, p).expect("open blocks DB");
    Arc::new(db)
});

/// Insert block into RocksDB store. Returns assigned block id.
pub async fn insert_block(proposer: &str, _view: u64, tx_ids: Vec<Uuid>) -> Result<Uuid> {
    let db = &**BLOCKS_DB;

    // Get and increment block height
    let height_key = b"block_height";
    let current_height: u64 = match db.get(height_key)? {
        Some(v) => {
            let bytes: [u8; 8] = v.as_slice().try_into().unwrap_or([0u8; 8]);
            u64::from_le_bytes(bytes)
        }
        None => 0,
    };
    let new_height = current_height + 1;
    db.put(height_key, new_height.to_le_bytes())?;

    // Create block with height
    let mut b = Block::new(proposer, tx_ids);
    b.height = new_height;

    let id = b.id;
    let key = id.as_bytes();
    let v = serde_json::to_vec(&b)?;
    db.put(key, v)?;
    Ok(id)
}

/// Backwards-compatible name used earlier by some stubs.
pub async fn insert_block_stub(tx_ids: Vec<Uuid>, proposer: &str, view: u64) -> Result<Uuid> {
    insert_block(proposer, view, tx_ids).await
}

/// Get tx ids for a block (empty vector if not found).
pub async fn get_txids_for_block(block_id: Uuid) -> Result<Vec<Uuid>> {
    let db = &**BLOCKS_DB;
    if let Some(v) = db.get(block_id.as_bytes())? {
        let b: Block = serde_json::from_slice(&v)?;
        Ok(b.tx_ids)
    } else {
        Ok(vec![])
    }
}

/// Get full block by ID
pub async fn get_block(block_id: Uuid) -> Result<Option<Block>> {
    let db = &**BLOCKS_DB;
    if let Some(v) = db.get(block_id.as_bytes())? {
        let b: Block = serde_json::from_slice(&v)?;
        Ok(Some(b))
    } else {
        Ok(None)
    }
}

use std::io;

/// Attempt to load a transaction by id from the store (RocksDB).
/// This is a simple helper used by reconciliation::finalize_block.
/// You must adapt the key-space to match how transactions are stored.
pub fn get_transaction(txid: Uuid) -> Result<Transaction, io::Error> {
    let base_path = std::env::var("ROCKSDB_PATH").unwrap_or_else(|_| "rocksdb_data".into());
    // Use sibling path, not nested subdirectory
    let p = format!("{}_mempool", base_path);
    std::fs::create_dir_all(&p).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    let mut opts = Options::default();
    opts.create_if_missing(true);
    let db = DB::open(&opts, p).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    let key = txid.as_bytes();
    match db
        .get(key)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?
    {
        Some(val) => {
            let txn: Transaction = serde_json::from_slice(&val).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("deserialize err: {}", e),
                )
            })?;
            Ok(txn)
        }
        None => Err(io::Error::new(io::ErrorKind::NotFound, "tx not found")),
    }
}
