// Oracle Subchain - Dedicated microchain for oracle data
// Uses existing subchain infrastructure for decentralized oracles

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Oracle data submission (stored in oracle subchain)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleData {
    pub feed_id: String,
    pub value: Vec<u8>,
    pub timestamp: u64,
    pub validator: String,
    pub stake: u64,
}

/// Oracle subchain transaction type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OracleTransaction {
    /// Submit data feed
    SubmitData(OracleData),
    /// Aggregate and finalize feed
    FinalizeFeed {
        feed_id: String,
        consensus_value: Vec<u8>,
        validators: Vec<String>,
    },
    /// Slash validator for wrong data
    SlashValidator {
        validator: String,
        reason: String,
        amount: u64,
    },
}

/// Oracle subchain configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleSubchainConfig {
    /// Subchain ID
    pub subchain_id: String,
    /// Minimum stake to submit data
    pub min_stake: u64,
    /// Aggregation window (seconds)
    pub aggregation_window: u64,
    /// Slash amount for wrong data
    pub slash_amount: u64,
}

impl Default for OracleSubchainConfig {
    fn default() -> Self {
        Self {
            subchain_id: "oracle_subchain".to_string(),
            min_stake: 500_000_000_000, // 5,000 OURO - same as subchain deposit
            aggregation_window: 60,     // 1 minute
            slash_amount: 100_000_000_000, // 1,000 OURO slash (20% of stake)
        }
    }
}

/// Oracle feed state (in subchain)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleFeed {
    pub feed_id: String,
    pub current_value: Vec<u8>,
    pub last_update: u64,
    pub submissions: Vec<OracleData>,
    pub confidence: f64,
}

/// Bridge verification data (from oracle subchain)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeVerification {
    /// Source chain name
    pub source_chain: String,
    /// Block hash on source chain
    pub block_hash: Vec<u8>,
    /// Transaction hash on source chain
    pub tx_hash: Vec<u8>,
    /// Verified by oracle validators
    pub validators: Vec<String>,
    /// Timestamp
    pub timestamp: u64,
    /// Is verified
    pub verified: bool,
    /// Confidence (0.0 to 1.0)
    pub confidence: f64,
    /// Number of validators
    pub num_validators: usize,
}

/// Process oracle transaction in subchain
pub fn process_oracle_transaction(
    tx: OracleTransaction,
    current_feeds: &mut HashMap<String, OracleFeed>,
    config: &OracleSubchainConfig,
) -> Result<(), String> {
    match tx {
        OracleTransaction::SubmitData(data) => {
            // Verify stake
            if data.stake < config.min_stake {
                return Err(format!("Insufficient stake: {}", data.stake));
            }

            // Add to pending submissions
            let feed = current_feeds
                .entry(data.feed_id.clone())
                .or_insert_with(|| OracleFeed {
                    feed_id: data.feed_id.clone(),
                    current_value: vec![],
                    last_update: 0,
                    submissions: vec![],
                    confidence: 0.0,
                });

            feed.submissions.push(data);

            Ok(())
        }

        OracleTransaction::FinalizeFeed {
            feed_id,
            consensus_value,
            validators,
        } => {
            // Update feed with consensus
            let feed = current_feeds.get_mut(&feed_id).ok_or("Feed not found")?;

            feed.current_value = consensus_value;
            feed.last_update = current_unix_time();
            feed.confidence = calculate_confidence(&feed.submissions);
            feed.submissions.clear(); // Clear after aggregation

            Ok(())
        }

        OracleTransaction::SlashValidator {
            validator,
            reason,
            amount,
        } => {
            // Slash recorded in subchain state
            // TODO: Integrate with validator registry
            Ok(())
        }
    }
}

/// Aggregate submissions to consensus value
pub fn aggregate_oracle_submissions(submissions: &[OracleData]) -> (Vec<u8>, Vec<String>) {
    if submissions.is_empty() {
        return (vec![], vec![]);
    }

    // Stake-weighted voting
    let mut value_stakes: HashMap<Vec<u8>, u64> = HashMap::new();
    let mut value_validators: HashMap<Vec<u8>, Vec<String>> = HashMap::new();

    for sub in submissions {
        *value_stakes.entry(sub.value.clone()).or_insert(0) += sub.stake;
        value_validators
            .entry(sub.value.clone())
            .or_insert_with(Vec::new)
            .push(sub.validator.clone());
    }

    // Get consensus (highest stake)
    let consensus = value_stakes
        .iter()
        .max_by_key(|(_, stake)| *stake)
        .map(|(value, _)| value.clone())
        .unwrap_or_default();

    let validators = value_validators
        .get(&consensus)
        .cloned()
        .unwrap_or_default();

    (consensus, validators)
}

/// Calculate confidence score
fn calculate_confidence(submissions: &[OracleData]) -> f64 {
    if submissions.is_empty() {
        return 0.0;
    }

    let total_stake: u64 = submissions.iter().map(|s| s.stake).sum();

    let mut value_stakes: HashMap<Vec<u8>, u64> = HashMap::new();
    for sub in submissions {
        *value_stakes.entry(sub.value.clone()).or_insert(0) += sub.stake;
    }

    let max_stake = value_stakes.values().max().unwrap_or(&0);

    (*max_stake as f64) / (total_stake as f64)
}

/// Read oracle data from subchain
pub async fn read_oracle_from_subchain(
    subchain_id: &str,
    feed_id: &str,
) -> Result<OracleFeed, String> {
    // Query oracle subchain state
    // TODO: Integrate with actual subchain query
    Err("Not implemented - integrate with subchain.rs".to_string())
}

/// Verify bridge transaction using oracle subchain
pub async fn verify_bridge_via_oracle(
    source_chain: &str,
    block_hash: &[u8],
    tx_hash: &[u8],
) -> Result<BridgeVerification, String> {
    // Create feed ID for bridge verification
    let feed_id = format!("bridge_{}_{}", source_chain, hex::encode(tx_hash));

    // Query oracle subchain for verification data
    // In production, this would query the actual subchain state
    // For now, we simulate oracle consensus by checking multiple validators

    // Get oracle feed data
    let oracle_manager = crate::oracle::get_oracle_manager()
        .map_err(|e| format!("Failed to get oracle manager: {}", e))?;

    let oracle_feed = oracle_manager
        .lock()
        .await
        .get_aggregated_feed(&feed_id)
        .await
        .map_err(|e| format!("Failed to get oracle feed: {}", e))?;

    // Parse verification result from oracle data
    let verified = if oracle_feed.value.len() >= 1 {
        oracle_feed.value[0] == 1 // 1 = verified, 0 = not verified
    } else {
        false
    };

    // Calculate confidence based on validator consensus
    let confidence = oracle_feed.consensus;
    let num_validators = oracle_feed.num_validators;

    Ok(BridgeVerification {
        source_chain: source_chain.to_string(),
        block_hash: block_hash.to_vec(),
        tx_hash: tx_hash.to_vec(),
        validators: vec![], // TODO: Extract validator list from oracle feed
        timestamp: current_unix_time(),
        verified,
        confidence,
        num_validators,
    })
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

    #[test]
    fn test_oracle_aggregation() {
        let submissions = vec![
            OracleData {
                feed_id: "BTC_USD".to_string(),
                value: 50000u64.to_le_bytes().to_vec(),
                timestamp: 1000,
                validator: "val1".to_string(),
                stake: 3000,
            },
            OracleData {
                feed_id: "BTC_USD".to_string(),
                value: 50000u64.to_le_bytes().to_vec(),
                timestamp: 1001,
                validator: "val2".to_string(),
                stake: 3000,
            },
            OracleData {
                feed_id: "BTC_USD".to_string(),
                value: 50100u64.to_le_bytes().to_vec(),
                timestamp: 1002,
                validator: "val3".to_string(),
                stake: 1000,
            },
        ];

        let (consensus, validators) = aggregate_oracle_submissions(&submissions);

        // Consensus should be 50000 (6000 stake vs 1000 stake)
        let consensus_value = u64::from_le_bytes(consensus[0..8].try_into().unwrap());
        assert_eq!(consensus_value, 50000);
        assert_eq!(validators.len(), 2);
    }

    #[test]
    fn test_confidence_calculation() {
        let submissions = vec![
            OracleData {
                feed_id: "test".to_string(),
                value: vec![1],
                timestamp: 1000,
                validator: "val1".to_string(),
                stake: 9000,
            },
            OracleData {
                feed_id: "test".to_string(),
                value: vec![2],
                timestamp: 1000,
                validator: "val2".to_string(),
                stake: 1000,
            },
        ];

        let confidence = calculate_confidence(&submissions);
        assert_eq!(confidence, 0.9); // 9000 / 10000
    }
}
