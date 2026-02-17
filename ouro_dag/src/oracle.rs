// Decentralized Oracle Network (own implementation, not Chainlink)
// Provides real-world data feeds for smart contracts

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Data feed (e.g., price, weather, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataFeed {
    pub feed_id: String,
    pub value: Vec<u8>,
    pub timestamp: u64,
    pub provider: String,
    pub signature: Vec<u8>,
}

/// Oracle submission from validator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleSubmission {
    pub feed_id: String,
    pub value: Vec<u8>,
    pub timestamp: u64,
    pub validator: String,
    pub stake: u64,
    pub signature: Vec<u8>,
}

/// Aggregated feed (consensus from multiple validators)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedFeed {
    pub feed_id: String,
    pub value: Vec<u8>,
    pub confidence: f64, // 0.0 to 1.0
    pub consensus: f64,  // 0.0 to 1.0 (for bridge verification)
    pub num_submissions: usize,
    pub num_validators: usize, // Number of unique validators
    pub total_stake: u64,
    pub timestamp: u64,
}

/// Oracle manager
#[derive(Debug)]
pub struct OracleManager {
    /// Active data feeds
    feeds: Arc<RwLock<HashMap<String, AggregatedFeed>>>,
    /// Pending submissions
    submissions: Arc<RwLock<HashMap<String, Vec<OracleSubmission>>>>,
    /// Minimum stake required
    min_stake: u64,
    /// Validator public keys (validator_id -> public_key)
    validator_pubkeys: Arc<RwLock<HashMap<String, VerifyingKey>>>,
}

impl OracleManager {
    pub fn new(min_stake: u64) -> Self {
        Self {
            feeds: Arc::new(RwLock::new(HashMap::new())),
            submissions: Arc::new(RwLock::new(HashMap::new())),
            min_stake,
            validator_pubkeys: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register validator public key for signature verification
    pub async fn register_validator(&self, validator_id: String, pubkey: VerifyingKey) {
        let mut pubkeys = self.validator_pubkeys.write().await;
        pubkeys.insert(validator_id, pubkey);
    }

    /// Submit oracle data
    pub async fn submit_data(&self, submission: OracleSubmission) -> Result<(), String> {
        // Verify stake
        if submission.stake < self.min_stake {
            return Err(format!(
                "Insufficient stake: {} < {}",
                submission.stake, self.min_stake
            ));
        }

        // Verify signature
        if submission.signature.len() != 64 {
            return Err("Invalid signature length".to_string());
        }

        // Get validator public key
        let pubkeys = self.validator_pubkeys.read().await;
        let pubkey = pubkeys
            .get(&submission.validator)
            .ok_or("Validator not registered")?;

        // Create message to verify: feed_id + value + timestamp + validator + stake
        let mut message = Vec::new();
        message.extend_from_slice(submission.feed_id.as_bytes());
        message.extend_from_slice(&submission.value);
        message.extend_from_slice(&submission.timestamp.to_le_bytes());
        message.extend_from_slice(submission.validator.as_bytes());
        message.extend_from_slice(&submission.stake.to_le_bytes());

        // Verify signature
        let sig_bytes: [u8; 64] = submission
            .signature
            .clone()
            .try_into()
            .map_err(|_| "Invalid signature format")?;
        let signature = Signature::from_bytes(&sig_bytes);

        pubkey
            .verify(&message, &signature)
            .map_err(|_| "Signature verification failed")?;

        // Add to pending submissions
        let mut subs = self.submissions.write().await;
        subs.entry(submission.feed_id.clone())
            .or_insert_with(Vec::new)
            .push(submission);

        Ok(())
    }

    /// Aggregate submissions and update feed
    pub async fn aggregate_feed(&self, feed_id: &str) -> Result<AggregatedFeed, String> {
        let mut subs = self.submissions.write().await;

        let submissions = subs.get_mut(feed_id).ok_or("No submissions for feed")?;

        if submissions.is_empty() {
            return Err("No submissions".to_string());
        }

        // Simple median aggregation (stake-weighted)
        let total_stake: u64 = submissions.iter().map(|s| s.stake).sum();

        // Count value occurrences weighted by stake
        let mut value_stakes: HashMap<Vec<u8>, u64> = HashMap::new();
        for sub in submissions.iter() {
            *value_stakes.entry(sub.value.clone()).or_insert(0) += sub.stake;
        }

        // Find value with highest stake
        let (consensus_value, consensus_stake) = value_stakes
            .iter()
            .max_by_key(|(_, stake)| *stake)
            .ok_or("No consensus")?;

        // Confidence = consensus_stake / total_stake
        let confidence = (*consensus_stake as f64) / (total_stake as f64);

        // Count unique validators
        let mut unique_validators = std::collections::HashSet::new();
        for sub in submissions.iter() {
            unique_validators.insert(sub.validator.clone());
        }

        let aggregated = AggregatedFeed {
            feed_id: feed_id.to_string(),
            value: consensus_value.clone(),
            confidence,
            consensus: confidence, // Same as confidence for compatibility
            num_submissions: submissions.len(),
            num_validators: unique_validators.len(),
            total_stake,
            timestamp: submissions[0].timestamp,
        };

        // Update feed
        let mut feeds = self.feeds.write().await;
        feeds.insert(feed_id.to_string(), aggregated.clone());

        // Clear submissions
        submissions.clear();

        Ok(aggregated)
    }

    /// Get latest feed value
    pub async fn get_feed(&self, feed_id: &str) -> Option<AggregatedFeed> {
        let feeds = self.feeds.read().await;
        feeds.get(feed_id).cloned()
    }

    /// List all feeds
    pub async fn list_feeds(&self) -> Vec<String> {
        let feeds = self.feeds.read().await;
        feeds.keys().cloned().collect()
    }

    /// Get aggregated feed (alias for compatibility)
    pub async fn get_aggregated_feed(&self, feed_id: &str) -> Result<AggregatedFeed, String> {
        self.get_feed(feed_id)
            .await
            .ok_or_else(|| "Feed not found".to_string())
    }
}

/// Global oracle manager instance
use once_cell::sync::Lazy;
use tokio::sync::Mutex as TokioMutex;

static GLOBAL_ORACLE_MANAGER: Lazy<Arc<TokioMutex<OracleManager>>> = Lazy::new(|| {
    Arc::new(TokioMutex::new(OracleManager::new(10000))) // 10k min stake
});

/// Get global oracle manager instance
pub fn get_oracle_manager() -> Result<Arc<TokioMutex<OracleManager>>, String> {
    Ok(GLOBAL_ORACLE_MANAGER.clone())
}

/// Price feed helper
#[derive(Debug, Clone)]
pub struct PriceFeed {
    manager: Arc<OracleManager>,
    feed_id: String,
}

impl PriceFeed {
    pub fn new(manager: Arc<OracleManager>, pair: &str) -> Self {
        let feed_id = format!("price_{}", pair);

        Self { manager, feed_id }
    }

    /// Get current price
    pub async fn get_price(&self) -> Option<u64> {
        let feed = self.manager.get_feed(&self.feed_id).await?;

        if feed.value.len() < 8 {
            return None;
        }

        Some(u64::from_le_bytes(feed.value[0..8].try_into().ok()?))
    }

    /// Submit price with cryptographic signature
    pub async fn submit_price(
        &self,
        price: u64,
        validator: String,
        stake: u64,
        signing_key: &SigningKey,
    ) -> Result<(), String> {
        let value = price.to_le_bytes().to_vec();
        let timestamp = current_unix_time();

        // Create message to sign: feed_id + value + timestamp + validator + stake
        let mut message = Vec::new();
        message.extend_from_slice(self.feed_id.as_bytes());
        message.extend_from_slice(&value);
        message.extend_from_slice(&timestamp.to_le_bytes());
        message.extend_from_slice(validator.as_bytes());
        message.extend_from_slice(&stake.to_le_bytes());

        // Sign the message
        let signature = signing_key.sign(&message);

        let submission = OracleSubmission {
            feed_id: self.feed_id.clone(),
            value,
            timestamp,
            validator,
            stake,
            signature: signature.to_bytes().to_vec(),
        };

        self.manager.submit_data(submission).await
    }
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
    use rand::rngs::OsRng;

    #[tokio::test]
    async fn test_oracle_submission() {
        let oracle = OracleManager::new(1000);

        // Generate validator keys
        let mut csprng = OsRng {};
        let signing_key = SigningKey::generate(&mut csprng);
        let verifying_key = signing_key.verifying_key();

        // Register validator
        oracle
            .register_validator("validator1".to_string(), verifying_key)
            .await;

        // Create signed submission
        let feed_id = "price_BTC_USD".to_string();
        let value = 50000u64.to_le_bytes().to_vec();
        let timestamp = current_unix_time();
        let validator = "validator1".to_string();
        let stake = 2000u64;

        // Sign the message
        let mut message = Vec::new();
        message.extend_from_slice(feed_id.as_bytes());
        message.extend_from_slice(&value);
        message.extend_from_slice(&timestamp.to_le_bytes());
        message.extend_from_slice(validator.as_bytes());
        message.extend_from_slice(&stake.to_le_bytes());
        let signature = signing_key.sign(&message);

        let submission = OracleSubmission {
            feed_id,
            value,
            timestamp,
            validator,
            stake,
            signature: signature.to_bytes().to_vec(),
        };

        assert!(oracle.submit_data(submission).await.is_ok());
    }

    #[tokio::test]
    async fn test_oracle_aggregation() {
        let oracle = Arc::new(OracleManager::new(1000));
        let mut csprng = OsRng {};

        // Multiple validators submit same price
        for i in 0..5 {
            // Generate keys for each validator
            let signing_key = SigningKey::generate(&mut csprng);
            let verifying_key = signing_key.verifying_key();
            let validator_id = format!("validator{}", i);

            // Register validator
            oracle
                .register_validator(validator_id.clone(), verifying_key)
                .await;

            // Create signed submission
            let feed_id = "price_ETH_USD".to_string();
            let value = 3000u64.to_le_bytes().to_vec();
            let timestamp = current_unix_time();
            let stake = 2000u64;

            let mut message = Vec::new();
            message.extend_from_slice(feed_id.as_bytes());
            message.extend_from_slice(&value);
            message.extend_from_slice(&timestamp.to_le_bytes());
            message.extend_from_slice(validator_id.as_bytes());
            message.extend_from_slice(&stake.to_le_bytes());
            let signature = signing_key.sign(&message);

            let submission = OracleSubmission {
                feed_id,
                value,
                timestamp,
                validator: validator_id,
                stake,
                signature: signature.to_bytes().to_vec(),
            };

            oracle.submit_data(submission).await.unwrap();
        }

        // Aggregate
        let aggregated = oracle.aggregate_feed("price_ETH_USD").await.unwrap();

        assert_eq!(aggregated.num_submissions, 5);
        assert!(aggregated.confidence > 0.9);
    }

    #[tokio::test]
    async fn test_price_feed() {
        let oracle = Arc::new(OracleManager::new(1000));
        let price_feed = PriceFeed::new(oracle.clone(), "BTC_USD");
        let mut csprng = OsRng {};

        // Generate keys for 3 validators
        let key1 = SigningKey::generate(&mut csprng);
        let key2 = SigningKey::generate(&mut csprng);
        let key3 = SigningKey::generate(&mut csprng);

        // Register validators
        oracle
            .register_validator("val1".to_string(), key1.verifying_key())
            .await;
        oracle
            .register_validator("val2".to_string(), key2.verifying_key())
            .await;
        oracle
            .register_validator("val3".to_string(), key3.verifying_key())
            .await;

        // Submit prices
        price_feed
            .submit_price(50000, "val1".to_string(), 3000, &key1)
            .await
            .unwrap();
        price_feed
            .submit_price(50000, "val2".to_string(), 3000, &key2)
            .await
            .unwrap();
        price_feed
            .submit_price(50100, "val3".to_string(), 1000, &key3)
            .await
            .unwrap();

        // Aggregate
        oracle.aggregate_feed("price_BTC_USD").await.unwrap();

        // Get price (should be 50000, higher stake)
        let price = price_feed.get_price().await.unwrap();
        assert_eq!(price, 50000);
    }
}
