// Blockchain indexer for fast queries (like The Graph)
// Provides SQL-like queries on blockchain data

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Indexed transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedTransaction {
    pub hash: String,
    pub from: String,
    pub to: String,
    pub amount: u64,
    pub block_height: u64,
    pub timestamp: u64,
    pub status: String,
}

/// Indexed block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedBlock {
    pub height: u64,
    pub hash: String,
    pub timestamp: u64,
    pub validator: String,
    pub tx_count: usize,
}

/// Query filter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryFilter {
    pub from_address: Option<String>,
    pub to_address: Option<String>,
    pub min_amount: Option<u64>,
    pub max_amount: Option<u64>,
    pub from_block: Option<u64>,
    pub to_block: Option<u64>,
    pub from_time: Option<u64>,
    pub to_time: Option<u64>,
}

/// Blockchain indexer
pub struct Indexer {
    /// Indexed transactions
    transactions: Arc<RwLock<Vec<IndexedTransaction>>>,
    /// Indexed blocks
    blocks: Arc<RwLock<HashMap<u64, IndexedBlock>>>,
    /// Address index (fast lookup)
    address_txs: Arc<RwLock<HashMap<String, Vec<String>>>>,
}

impl Indexer {
    pub fn new() -> Self {
        Self {
            transactions: Arc::new(RwLock::new(Vec::new())),
            blocks: Arc::new(RwLock::new(HashMap::new())),
            address_txs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Index new transaction
    pub async fn index_transaction(&self, tx: IndexedTransaction) {
        // Add to transactions
        let mut txs = self.transactions.write().await;
        txs.push(tx.clone());

        // Update address index
        let mut addr_idx = self.address_txs.write().await;

        addr_idx
            .entry(tx.from.clone())
            .or_insert_with(Vec::new)
            .push(tx.hash.clone());

        addr_idx
            .entry(tx.to.clone())
            .or_insert_with(Vec::new)
            .push(tx.hash.clone());
    }

    /// Index new block
    pub async fn index_block(&self, block: IndexedBlock) {
        let mut blocks = self.blocks.write().await;
        blocks.insert(block.height, block);
    }

    /// Query transactions
    pub async fn query_transactions(&self, filter: QueryFilter) -> Vec<IndexedTransaction> {
        let txs = self.transactions.read().await;

        txs.iter()
            .filter(|tx| {
                // Filter by from address
                if let Some(ref from) = filter.from_address {
                    if &tx.from != from {
                        return false;
                    }
                }

                // Filter by to address
                if let Some(ref to) = filter.to_address {
                    if &tx.to != to {
                        return false;
                    }
                }

                // Filter by amount
                if let Some(min) = filter.min_amount {
                    if tx.amount < min {
                        return false;
                    }
                }

                if let Some(max) = filter.max_amount {
                    if tx.amount > max {
                        return false;
                    }
                }

                // Filter by block height
                if let Some(from_block) = filter.from_block {
                    if tx.block_height < from_block {
                        return false;
                    }
                }

                if let Some(to_block) = filter.to_block {
                    if tx.block_height > to_block {
                        return false;
                    }
                }

                // Filter by timestamp
                if let Some(from_time) = filter.from_time {
                    if tx.timestamp < from_time {
                        return false;
                    }
                }

                if let Some(to_time) = filter.to_time {
                    if tx.timestamp > to_time {
                        return false;
                    }
                }

                true
            })
            .cloned()
            .collect()
    }

    /// Get transactions for address (fast)
    pub async fn get_address_transactions(&self, address: &str) -> Vec<IndexedTransaction> {
        let addr_idx = self.address_txs.read().await;

        let tx_hashes = match addr_idx.get(address) {
            Some(hashes) => hashes.clone(),
            None => return Vec::new(),
        };

        let txs = self.transactions.read().await;

        txs.iter()
            .filter(|tx| tx_hashes.contains(&tx.hash))
            .cloned()
            .collect()
    }

    /// Get balance for address
    pub async fn get_address_balance(&self, address: &str) -> u64 {
        let txs = self.get_address_transactions(address).await;

        let mut balance: i64 = 0;

        for tx in txs {
            if tx.from == address {
                balance -= tx.amount as i64;
            }
            if tx.to == address {
                balance += tx.amount as i64;
            }
        }

        balance.max(0) as u64
    }

    /// Get block by height
    pub async fn get_block(&self, height: u64) -> Option<IndexedBlock> {
        let blocks = self.blocks.read().await;
        blocks.get(&height).cloned()
    }

    /// Get latest blocks
    pub async fn get_latest_blocks(&self, count: usize) -> Vec<IndexedBlock> {
        let blocks = self.blocks.read().await;

        let mut heights: Vec<_> = blocks.keys().cloned().collect();
        heights.sort_by(|a, b| b.cmp(a)); // Descending

        heights
            .iter()
            .take(count)
            .filter_map(|h| blocks.get(h).cloned())
            .collect()
    }

    /// Get statistics
    pub async fn get_stats(&self) -> IndexerStats {
        let txs = self.transactions.read().await;
        let blocks = self.blocks.read().await;

        let total_volume: u64 = txs.iter().map(|tx| tx.amount).sum();

        IndexerStats {
            total_transactions: txs.len(),
            total_blocks: blocks.len(),
            total_volume,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IndexerStats {
    pub total_transactions: usize,
    pub total_blocks: usize,
    pub total_volume: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_indexer() {
        let indexer = Indexer::new();

        // Index transactions
        for i in 0..10 {
            let tx = IndexedTransaction {
                hash: format!("tx{}", i),
                from: "alice".to_string(),
                to: "bob".to_string(),
                amount: 100 * (i + 1),
                block_height: i,
                timestamp: 1000 + i,
                status: "confirmed".to_string(),
            };

            indexer.index_transaction(tx).await;
        }

        // Query by amount
        let filter = QueryFilter {
            from_address: None,
            to_address: None,
            min_amount: Some(500),
            max_amount: None,
            from_block: None,
            to_block: None,
            from_time: None,
            to_time: None,
        };

        let results = indexer.query_transactions(filter).await;
        assert!(results.len() < 10);
        assert!(results.iter().all(|tx| tx.amount >= 500));
    }

    #[tokio::test]
    async fn test_address_transactions() {
        let indexer = Indexer::new();

        let tx1 = IndexedTransaction {
            hash: "tx1".to_string(),
            from: "alice".to_string(),
            to: "bob".to_string(),
            amount: 100,
            block_height: 1,
            timestamp: 1000,
            status: "confirmed".to_string(),
        };

        let tx2 = IndexedTransaction {
            hash: "tx2".to_string(),
            from: "charlie".to_string(),
            to: "alice".to_string(),
            amount: 200,
            block_height: 2,
            timestamp: 1001,
            status: "confirmed".to_string(),
        };

        indexer.index_transaction(tx1).await;
        indexer.index_transaction(tx2).await;

        // Alice should have 2 transactions
        let alice_txs = indexer.get_address_transactions("alice").await;
        assert_eq!(alice_txs.len(), 2);

        // Bob should have 1 transaction
        let bob_txs = indexer.get_address_transactions("bob").await;
        assert_eq!(bob_txs.len(), 1);
    }

    #[tokio::test]
    async fn test_balance() {
        let indexer = Indexer::new();

        // Alice receives 100
        let tx1 = IndexedTransaction {
            hash: "tx1".to_string(),
            from: "genesis".to_string(),
            to: "alice".to_string(),
            amount: 100,
            block_height: 1,
            timestamp: 1000,
            status: "confirmed".to_string(),
        };

        // Alice sends 30
        let tx2 = IndexedTransaction {
            hash: "tx2".to_string(),
            from: "alice".to_string(),
            to: "bob".to_string(),
            amount: 30,
            block_height: 2,
            timestamp: 1001,
            status: "confirmed".to_string(),
        };

        indexer.index_transaction(tx1).await;
        indexer.index_transaction(tx2).await;

        // Alice balance = 100 - 30 = 70
        let balance = indexer.get_address_balance("alice").await;
        assert_eq!(balance, 70);
    }
}
