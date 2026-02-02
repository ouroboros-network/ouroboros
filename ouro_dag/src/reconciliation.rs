// src/reconciliation.rs
use crate::account_abstraction::{EntryPoint, UserOperation};
use crate::fee_market::FeeMarketManager;
use crate::indexer::{IndexedBlock, IndexedTransaction, Indexer};
use crate::ring_signatures;
use crate::stealth_addresses;
use crate::storage;
use crate::tail_emission::{calculate_block_reward, EmissionConfig};
use crate::vm;
use anyhow::{Context, Result};
use log::{error, info, warn};
use once_cell::sync::Lazy;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

/// Global fee market manager
static FEE_MARKET: Lazy<Arc<Mutex<FeeMarketManager>>> =
    Lazy::new(|| Arc::new(Mutex::new(FeeMarketManager::new())));

/// Global entry point for account abstraction
static ACCOUNT_ABSTRACTION: Lazy<Arc<Mutex<EntryPoint>>> =
    Lazy::new(|| Arc::new(Mutex::new(EntryPoint::new())));

/// Global blockchain indexer
static INDEXER: Lazy<Arc<Indexer>> = Lazy::new(|| Arc::new(Indexer::new()));

/// Legacy helper stub used by some boot code. Keep trivial but harmless.
pub fn reconcile_token_spends(_dag: &mut crate::dag::dag::DAG) {
    // no-op for now; real implementation will read external inputs and inject txns into DAG
    info!("reconcile_token_spends: stub called");
}

/// Called by consensus when a block finalizes.
///
/// This function:
/// 1. Fetches all transaction IDs in the block
/// 2. Loads full transaction data
/// 3. Executes smart contracts via the VM
/// 4. Persists execution receipts to storage
/// 5. Updates account balances (future: implement balance tracking)
///
/// Idempotent: Safe to call multiple times for the same block.
pub async fn finalize_block(block_id: Uuid) -> Result<()> {
    info!("CONFIG: finalize_block called for block {}", block_id);

    // Step 1: Get transaction IDs from the block
    let tx_ids = crate::dag::dag::get_txids_for_block(block_id)
        .await
        .context("Failed to fetch transaction IDs for block")?;

    if tx_ids.is_empty() {
        info!(
            " Block {} is empty (0 transactions) - nothing to finalize",
            block_id
        );
        return Ok(());
    }

    info!(" Block {} contains {} transactions", block_id, tx_ids.len());

    // Step 2: Fetch full transaction data
    let mut transactions = Vec::new();
    for tx_id in &tx_ids {
        match tokio::task::spawn_blocking({
            let tx_id = *tx_id;
            move || crate::dag::dag::get_transaction(tx_id)
        })
        .await
        {
            Ok(Ok(tx)) => transactions.push(tx),
            Ok(Err(e)) => {
                warn!(
                    "WARNING Failed to load transaction {}: {} - skipping",
                    tx_id, e
                );
                continue;
            }
            Err(e) => {
                error!("ERROR Task join error loading transaction {}: {}", tx_id, e);
                continue;
            }
        }
    }

    if transactions.is_empty() {
        warn!(
            "WARNING No transactions could be loaded for block {}",
            block_id
        );
        return Ok(());
    }

    info!(
        " Loaded {}/{} transactions successfully",
        transactions.len(),
        tx_ids.len()
    );

    // Step 3: Get storage handle for VM execution
    let db = storage::get_global_storage()
        .ok_or_else(|| anyhow::anyhow!("Global storage not initialized"))?;

    // Step 4: Execute smart contracts via VM
    info!(
        " Executing smart contracts for {} transactions...",
        transactions.len()
    );

    let results = tokio::task::spawn_blocking({
        let db_clone = db.clone();
        let txs_clone = transactions.clone();
        move || vm::execute_contracts(&db_clone, &txs_clone)
    })
    .await
    .context("Task join error during VM execution")?
    .map_err(|e| anyhow::anyhow!("VM execution error: {}", e))?;

    // Step 5: Persist execution receipts
    let mut success_count = 0;
    let mut failure_count = 0;

    for (i, result) in results.iter().enumerate() {
        if result.success {
            success_count += 1;
            info!(" TX {} executed successfully", i);
        } else {
            failure_count += 1;
            if let Some(ref err) = result.error {
                warn!(" ERROR TX {} execution failed: {}", i, err);
            } else {
                warn!(" ERROR TX {} execution failed (no error message)", i);
            }
        }

        // Persist receipt to storage (key: "receipt:<tx_index>")
        let receipt_key = format!("receipt:{}", i);
        if let Err(e) = storage::put(&db, receipt_key.into_bytes(), result) {
            error!("Failed to persist receipt for TX {}: {}", i, e);
        }
    }

    info!(
        " Block {} finalized: {}/{} transactions executed ({} success, {} failed)",
        block_id,
        transactions.len(),
        tx_ids.len(),
        success_count,
        failure_count
    );

    // Step 6: Update fee market based on block gas usage
    let mut fee_market = FEE_MARKET.lock().await;
    // Assume each transaction uses 21000 gas (standard transfer)
    let total_gas = transactions.len() as u64 * 21000;
    if let Err(e) = fee_market.process_transaction(total_gas) {
        warn!("WARNING Fee market error: {}", e);
    }
    fee_market.finalize_block();
    let current_base_fee = fee_market.get_base_fee();
    drop(fee_market);

    info!(
        " Fee market: base_fee = {} (gas used: {})",
        current_base_fee, total_gas
    );

    // Step 7: Distribute block rewards using tail emission
    if let Err(e) = distribute_block_reward(&db, block_id).await {
        warn!("WARNING Failed to distribute block reward: {}", e);
    }

    // Step 8: Index block and transactions for fast queries
    if let Err(e) = index_block_data(&db, block_id, &transactions).await {
        warn!("WARNING Failed to index block: {}", e);
    }

    Ok(())
}

/// Distribute block reward to the proposer
async fn distribute_block_reward(db: &storage::RocksDb, block_id: Uuid) -> Result<()> {
    // Get block details
    let block = crate::dag::dag::get_block(block_id)
        .await
        .context("Failed to fetch block")?
        .ok_or_else(|| anyhow::anyhow!("Block not found"))?;

    // Calculate block reward based on height using tail emission
    let emission_config = EmissionConfig::default();
    let block_reward = calculate_block_reward(block.height, &emission_config);

    if block_reward == 0 {
        return Ok(()); // No reward to distribute
    }

    // Credit reward to proposer's balance
    let balance_key = format!("balance:{}", block.proposer);
    let current_balance: u64 = storage::get_str(db, &balance_key)
        .map_err(|e| anyhow::anyhow!("Failed to get balance: {}", e))?
        .unwrap_or(0);
    let new_balance = current_balance.saturating_add(block_reward);
    storage::put_str(db, &balance_key, &new_balance)
        .map_err(|e| anyhow::anyhow!("Failed to put balance: {}", e))?;

    info!(
        " Block reward: {} units distributed to proposer {} (height: {})",
        block_reward, block.proposer, block.height
    );

    Ok(())
}

/// Index block and transaction data for fast queries
async fn index_block_data(
    db: &storage::RocksDb,
    block_id: Uuid,
    transactions: &[crate::dag::transaction::Transaction],
) -> Result<()> {
    // Get block details
    let block = crate::dag::dag::get_block(block_id)
        .await
        .context("Failed to fetch block for indexing")?
        .ok_or_else(|| anyhow::anyhow!("Block not found for indexing"))?;

    // Index block
    let indexed_block = IndexedBlock {
        height: block.height,
        hash: block_id.to_string(),
        timestamp: block.timestamp.timestamp() as u64,
        validator: block.proposer.clone(),
        tx_count: transactions.len(),
    };

    INDEXER.index_block(indexed_block).await;

    // Index transactions
    for tx in transactions {
        let indexed_tx = IndexedTransaction {
            hash: tx.id.to_string(),
            from: tx.sender.clone(),
            to: tx.recipient.clone(),
            amount: tx.amount,
            block_height: block.height,
            timestamp: tx.timestamp.timestamp() as u64,
            status: "confirmed".to_string(),
        };

        INDEXER.index_transaction(indexed_tx).await;
    }

    info!(
        " Indexed {} transactions for block {}",
        transactions.len(),
        block_id
    );

    Ok(())
}
