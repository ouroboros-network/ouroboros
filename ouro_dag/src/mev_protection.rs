// MEV (Maximal Extractable Value) Protection
// Prevents front-running and transaction reordering attacks

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Transaction commitment (encrypted)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxCommitment {
    /// Commitment hash (hides transaction content)
    pub commitment: Vec<u8>,
    /// Timestamp when committed
    pub timestamp: u64,
    /// Sender signature
    pub signature: Vec<u8>,
}

/// Revealed transaction (after commit period)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevealedTx {
    /// Original commitment
    pub commitment: Vec<u8>,
    /// Actual transaction data
    pub tx_data: Vec<u8>,
    /// Nonce used in commitment
    pub nonce: Vec<u8>,
}

/// MEV protection manager
pub struct MevProtection {
    /// Pending commitments (commit phase)
    commitments: Arc<RwLock<HashMap<Vec<u8>, TxCommitment>>>,
    /// Reveal queue (after commit phase)
    reveal_queue: Arc<RwLock<VecDeque<RevealedTx>>>,
    /// Commit-reveal window (seconds)
    commit_window: u64,
}

impl MevProtection {
    pub fn new(commit_window_secs: u64) -> Self {
        Self {
            commitments: Arc::new(RwLock::new(HashMap::new())),
            reveal_queue: Arc::new(RwLock::new(VecDeque::new())),
            commit_window: commit_window_secs,
        }
    }

    /// Phase 1: Commit to transaction (hide content)
    pub async fn commit_transaction(
        &self,
        tx_hash: Vec<u8>,
        commitment: Vec<u8>,
        signature: Vec<u8>,
    ) -> Result<(), String> {
        let mut commits = self.commitments.write().await;

        if commits.contains_key(&tx_hash) {
            return Err("Transaction already committed".to_string());
        }

        let now = current_unix_time();

        commits.insert(
            tx_hash,
            TxCommitment {
                commitment,
                timestamp: now,
                signature,
            },
        );

        Ok(())
    }

    /// Phase 2: Reveal transaction (after commit window)
    pub async fn reveal_transaction(
        &self,
        commitment: Vec<u8>,
        tx_data: Vec<u8>,
        nonce: Vec<u8>,
    ) -> Result<(), String> {
        // Verify commitment matches
        let expected = hash_commitment(&tx_data, &nonce);

        if expected != commitment {
            return Err("Invalid reveal: commitment mismatch".to_string());
        }

        let mut commits = self.commitments.write().await;
        let tx_hash = hash_data(&tx_data);

        // Check if commitment exists (without removing yet)
        let commit_data = commits.get(&tx_hash).ok_or("No commitment found")?;

        // Check if enough time has passed BEFORE removing
        let now = current_unix_time();
        if now < commit_data.timestamp + self.commit_window {
            return Err("Commit window not elapsed yet".to_string());
        }

        // Now safe to remove since time check passed
        let commit_data = commits.remove(&tx_hash).unwrap();

        // Add to reveal queue (FIFO ordering)
        let mut queue = self.reveal_queue.write().await;
        queue.push_back(RevealedTx {
            commitment: commit_data.commitment,
            tx_data,
            nonce,
        });

        Ok(())
    }

    /// Get next batch of transactions (fair ordering)
    pub async fn get_next_batch(&self, batch_size: usize) -> Vec<RevealedTx> {
        let mut queue = self.reveal_queue.write().await;
        let mut batch = Vec::new();

        for _ in 0..batch_size {
            if let Some(tx) = queue.pop_front() {
                batch.push(tx);
            } else {
                break;
            }
        }

        batch
    }

    /// Remove expired commitments
    pub async fn cleanup_expired(&self, max_age_secs: u64) {
        let mut commits = self.commitments.write().await;
        let now = current_unix_time();

        commits.retain(|_, commit| now - commit.timestamp < max_age_secs);
    }
}

/// Batch transaction ordering (prevents MEV)
pub struct BatchOrdering {
    /// Transactions in current batch
    batch: Arc<RwLock<Vec<Vec<u8>>>>,
    /// Batch interval (seconds)
    interval: u64,
    /// Last batch time
    last_batch: Arc<RwLock<u64>>,
}

impl BatchOrdering {
    pub fn new(interval_secs: u64) -> Self {
        Self {
            batch: Arc::new(RwLock::new(Vec::new())),
            interval: interval_secs,
            last_batch: Arc::new(RwLock::new(current_unix_time())),
        }
    }

    /// Add transaction to batch
    pub async fn add_transaction(&self, tx_data: Vec<u8>) {
        let mut batch = self.batch.write().await;
        batch.push(tx_data);
    }

    /// Seal batch and return transactions (deterministic order)
    pub async fn seal_batch(&self) -> Option<Vec<Vec<u8>>> {
        let now = current_unix_time();
        let mut last = self.last_batch.write().await;

        if now - *last < self.interval {
            return None;
        }

        let mut batch = self.batch.write().await;

        if batch.is_empty() {
            return None;
        }

        // Deterministic ordering: sort by hash
        batch.sort_by_key(|tx| hash_data(tx));

        let sealed = batch.drain(..).collect();
        *last = now;

        Some(sealed)
    }
}

/// Priority gas auction (fair)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasBid {
    pub tx_hash: Vec<u8>,
    pub gas_price: u64,
    pub timestamp: u64,
}

/// Fair gas auction (prevents MEV via high gas)
pub struct FairGasAuction {
    bids: Arc<RwLock<Vec<GasBid>>>,
}

impl FairGasAuction {
    pub fn new() -> Self {
        Self {
            bids: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Submit gas bid
    pub async fn submit_bid(&self, tx_hash: Vec<u8>, gas_price: u64) {
        let mut bids = self.bids.write().await;

        bids.push(GasBid {
            tx_hash,
            gas_price,
            timestamp: current_unix_time(),
        });
    }

    /// Select transactions (fair algorithm)
    pub async fn select_transactions(&self, max_count: usize) -> Vec<Vec<u8>> {
        let mut bids = self.bids.write().await;

        // Sort by: 1) timestamp (earlier first), 2) gas price
        bids.sort_by(|a, b| match a.timestamp.cmp(&b.timestamp) {
            std::cmp::Ordering::Equal => b.gas_price.cmp(&a.gas_price),
            other => other,
        });

        bids.iter()
            .take(max_count)
            .map(|bid| bid.tx_hash.clone())
            .collect()
    }
}

fn hash_commitment(tx_data: &[u8], nonce: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(tx_data);
    hasher.update(nonce);
    hasher.finalize().to_vec()
}

fn hash_data(data: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().to_vec()
}

fn current_unix_time() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_commit_reveal() {
        let mev = MevProtection::new(2); // 2 second window

        let tx_data = b"transfer 100 tokens".to_vec();
        let nonce = b"random_nonce_12345".to_vec();

        let commitment = hash_commitment(&tx_data, &nonce);
        let tx_hash = hash_data(&tx_data);

        // Commit
        mev.commit_transaction(tx_hash.clone(), commitment.clone(), vec![])
            .await
            .unwrap();

        // Try to reveal immediately (should fail)
        let result = mev
            .reveal_transaction(commitment.clone(), tx_data.clone(), nonce.clone())
            .await;
        assert!(result.is_err());

        // Wait for commit window
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

        // Now reveal should work
        mev.reveal_transaction(commitment, tx_data, nonce)
            .await
            .unwrap();

        // Check revealed transaction
        let batch = mev.get_next_batch(10).await;
        assert_eq!(batch.len(), 1);
    }

    #[tokio::test]
    async fn test_batch_ordering() {
        let ordering = BatchOrdering::new(1); // 1 second batches

        ordering.add_transaction(b"tx3".to_vec()).await;
        ordering.add_transaction(b"tx1".to_vec()).await;
        ordering.add_transaction(b"tx2".to_vec()).await;

        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        let batch = ordering.seal_batch().await.unwrap();

        // Should be deterministically ordered
        assert_eq!(batch.len(), 3);

        // Verify ordering is deterministic (by hash)
        let hash1 = hash_data(&batch[0]);
        let hash2 = hash_data(&batch[1]);
        let hash3 = hash_data(&batch[2]);

        assert!(hash1 <= hash2 && hash2 <= hash3);
    }
}
