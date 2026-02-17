// Independent Oracle Node (can run separately from validators)
// Hybrid design: semi-independent from Ouroboros blockchain

use ed25519_dalek::{Signature, Signer, SigningKey};
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Global oracle registry instance (initialized once during startup)
static GLOBAL_ORACLE_REGISTRY: OnceCell<Arc<OracleNodeRegistry>> = OnceCell::new();

/// Initialize the global oracle registry instance.
pub fn init_global_oracle_registry(min_stake: u64) -> Arc<OracleNodeRegistry> {
    let registry = Arc::new(OracleNodeRegistry::new(min_stake));
    let _ = GLOBAL_ORACLE_REGISTRY.set(registry.clone());
    registry
}

/// Get the global oracle registry instance.
pub fn get_global_oracle_registry() -> Option<Arc<OracleNodeRegistry>> {
    GLOBAL_ORACLE_REGISTRY.get().cloned()
}

/// Get or create the global oracle registry (with default stake).
pub fn get_or_init_oracle_registry() -> Arc<OracleNodeRegistry> {
    GLOBAL_ORACLE_REGISTRY
        .get_or_init(|| {
            // Default minimum stake: 5,000 OURO (same as subchain deposit)
            Arc::new(OracleNodeRegistry::new(500_000_000_000)) // 5,000 OURO
        })
        .clone()
}

/// Oracle node configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleNodeConfig {
    /// Node operator ID (auto-generated from node identity if not provided)
    #[serde(default)]
    pub operator_id: String,
    /// Stake amount (OURO tokens in smallest units)
    pub stake: u64,
    /// Data sources to fetch from
    pub data_sources: Vec<String>,
    /// Update interval (milliseconds)
    pub update_interval_ms: u64,
    /// Is this node also a validator?
    pub is_validator: bool,
    /// Wallet address for rewards (optional, uses node wallet if not set)
    #[serde(default)]
    pub reward_address: Option<String>,
}

impl OracleNodeConfig {
    /// Create config from node identity (auto-generates operator_id)
    pub fn from_node_identity(identity: &crate::node_identity::NodeIdentity) -> Self {
        Self {
            operator_id: format!("oracle_{}", identity.short_id()),
            stake: 0, // Must be set separately
            data_sources: vec![
                "coingecko".to_string(),
                "binance".to_string(),
                "open-meteo".to_string(),
            ],
            update_interval_ms: 5000,
            is_validator: false,
            reward_address: None,
        }
    }

    /// Load config from file, auto-generating operator_id if missing
    pub fn load_or_create(
        config_path: &std::path::Path,
        identity: &crate::node_identity::NodeIdentity,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        if config_path.exists() {
            let json = std::fs::read_to_string(config_path)?;
            let mut config: OracleNodeConfig = serde_json::from_str(&json)?;

            // Auto-generate operator_id if empty
            if config.operator_id.is_empty() {
                config.operator_id = format!("oracle_{}", identity.short_id());
                // Save updated config
                let updated = serde_json::to_string_pretty(&config)?;
                std::fs::write(config_path, updated)?;
            }

            Ok(config)
        } else {
            // Create default config
            let config = Self::from_node_identity(identity);
            let json = serde_json::to_string_pretty(&config)?;
            std::fs::write(config_path, json)?;
            Ok(config)
        }
    }
}

/// Data source provider
#[derive(Debug, Clone)]
pub enum DataSource {
    /// CoinGecko API
    CoinGecko,
    /// CoinMarketCap API
    CoinMarketCap,
    /// Binance API (first-party)
    Binance,
    /// Coinbase API (first-party)
    Coinbase,
    /// Custom HTTP endpoint
    Custom(String),
}

impl DataSource {
    /// Fetch price data from source
    pub async fn fetch_price(&self, symbol: &str) -> Result<f64, String> {
        match self {
            DataSource::CoinGecko => {
                // Example: https://api.coingecko.com/api/v3/simple/price?ids=bitcoin&vs_currencies=usd
                let url = format!(
                    "https://api.coingecko.com/api/v3/simple/price?ids={}&vs_currencies=usd",
                    symbol.to_lowercase()
                );

                // In production, use actual HTTP client
                // For now, return simulated data
                log::info!("Would fetch from CoinGecko: {}", url);
                Ok(50000.0) // Simulated BTC price
            }
            DataSource::Binance => {
                // Example: https://api.binance.com/api/v3/ticker/price?symbol=BTCUSDT
                let url = format!(
                    "https://api.binance.com/api/v3/ticker/price?symbol={}USDT",
                    symbol.to_uppercase()
                );

                log::info!("Would fetch from Binance: {}", url);
                Ok(50001.0) // Simulated
            }
            DataSource::Coinbase => {
                // Example: https://api.coinbase.com/v2/prices/BTC-USD/spot
                let url = format!(
                    "https://api.coinbase.com/v2/prices/{}-USD/spot",
                    symbol.to_uppercase()
                );

                log::info!("Would fetch from Coinbase: {}", url);
                Ok(49999.0) // Simulated
            }
            DataSource::CoinMarketCap => {
                log::info!("Would fetch from CoinMarketCap");
                Ok(50002.0) // Simulated
            }
            DataSource::Custom(url) => {
                log::info!("Would fetch from custom endpoint: {}", url);
                Ok(50000.0) // Simulated
            }
        }
    }
}

/// Oracle node (independent from validators)
pub struct OracleNode {
    /// Configuration
    config: OracleNodeConfig,
    /// Signing key for submissions
    signing_key: SigningKey,
    /// Data sources
    sources: Vec<DataSource>,
    /// Reputation score (0.0 to 1.0)
    reputation: Arc<RwLock<f64>>,
    /// Submission history
    submission_count: Arc<RwLock<u64>>,
}

impl OracleNode {
    pub fn new(config: OracleNodeConfig, signing_key: SigningKey) -> Self {
        // Configure data sources based on config
        let sources = vec![
            DataSource::Binance,
            DataSource::CoinGecko,
            DataSource::Coinbase,
        ];

        Self {
            config,
            signing_key,
            sources,
            reputation: Arc::new(RwLock::new(1.0)), // Start with full reputation
            submission_count: Arc::new(RwLock::new(0)),
        }
    }

    /// Fetch data from all sources and aggregate using universal fetchers
    pub async fn fetch_and_aggregate(&self, feed_id: &str) -> Result<Vec<u8>, String> {
        // Use universal fetcher to get data from all free APIs
        let data = crate::oracle_fetchers::UniversalFetcher::fetch_by_feed_id(feed_id).await?;

        log::info!("Fetched data for {} ({} bytes)", feed_id, data.len());

        Ok(data)
    }

    /// Create signed oracle submission
    pub async fn create_submission(
        &self,
        feed_id: &str,
        value: Vec<u8>,
    ) -> Result<crate::oracle::OracleSubmission, String> {
        let timestamp = current_unix_time();

        // Create message to sign
        let mut message = Vec::new();
        message.extend_from_slice(feed_id.as_bytes());
        message.extend_from_slice(&value);
        message.extend_from_slice(&timestamp.to_le_bytes());
        message.extend_from_slice(self.config.operator_id.as_bytes());
        message.extend_from_slice(&self.config.stake.to_le_bytes());

        // Sign
        let signature = self.signing_key.sign(&message);

        // Increment submission count
        let mut count = self.submission_count.write().await;
        *count += 1;

        Ok(crate::oracle::OracleSubmission {
            feed_id: feed_id.to_string(),
            value,
            timestamp,
            validator: self.config.operator_id.clone(),
            stake: self.config.stake,
            signature: signature.to_bytes().to_vec(),
        })
    }

    /// Run oracle node (continuous loop)
    pub async fn run(&self) -> Result<(), String> {
        log::info!(
            "Starting oracle node: {} (stake: {}, validator: {})",
            self.config.operator_id,
            self.config.stake,
            self.config.is_validator
        );

        let interval = tokio::time::Duration::from_millis(self.config.update_interval_ms);

        loop {
            // Fetch and submit data for common feeds
            let feeds = vec!["BTC_USD", "ETH_USD", "OURO_USD"];

            for feed_id in feeds {
                match self.fetch_and_aggregate(feed_id).await {
                    Ok(value) => {
                        match self.create_submission(feed_id, value).await {
                            Ok(submission) => {
                                log::info!("Created submission for {}", feed_id);

                                // Submit to oracle manager (via API in production)
                                // For now, just log
                                // TODO: HTTP POST to /api/oracle/submit
                            }
                            Err(e) => {
                                log::error!("Failed to create submission: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to fetch {}: {}", feed_id, e);
                    }
                }
            }

            // Wait for next interval
            tokio::time::sleep(interval).await;
        }
    }

    /// Get reputation score
    pub async fn get_reputation(&self) -> f64 {
        *self.reputation.read().await
    }

    /// Update reputation (called by oracle manager after verification)
    pub async fn update_reputation(&self, correct: bool) {
        let mut rep = self.reputation.write().await;

        if correct {
            // Increase reputation (slowly)
            *rep = (*rep * 0.99 + 1.0 * 0.01).min(1.0);
        } else {
            // Decrease reputation (quickly)
            *rep = (*rep * 0.9).max(0.0);
        }

        log::info!("Reputation updated: {} (correct: {})", *rep, correct);
    }
}

/// Oracle node registry (tracks all oracle nodes)
pub struct OracleNodeRegistry {
    /// Registered nodes
    nodes: Arc<RwLock<HashMap<String, OracleNodeInfo>>>,
    /// Minimum stake required
    min_stake: u64,
}

#[derive(Debug, Clone)]
pub struct OracleNodeInfo {
    pub operator_id: String,
    pub stake: u64,
    pub reputation: f64,
    pub is_validator: bool,
    pub total_submissions: u64,
    pub correct_submissions: u64,
    pub registration_time: u64,
}

impl OracleNodeRegistry {
    pub fn new(min_stake: u64) -> Self {
        Self {
            nodes: Arc::new(RwLock::new(HashMap::new())),
            min_stake,
        }
    }

    /// Register oracle node
    pub async fn register_node(&self, info: OracleNodeInfo) -> Result<(), String> {
        if info.stake < self.min_stake {
            return Err(format!(
                "Insufficient stake: {} < {}",
                info.stake, self.min_stake
            ));
        }

        let mut nodes = self.nodes.write().await;
        nodes.insert(info.operator_id.clone(), info);

        Ok(())
    }

    /// Get node info
    pub async fn get_node(&self, operator_id: &str) -> Option<OracleNodeInfo> {
        let nodes = self.nodes.read().await;
        nodes.get(operator_id).cloned()
    }

    /// Get top nodes by reputation
    pub async fn get_top_nodes(&self, limit: usize) -> Vec<OracleNodeInfo> {
        let nodes = self.nodes.read().await;
        let mut node_list: Vec<_> = nodes.values().cloned().collect();

        // Sort by reputation (descending), then by stake
        node_list.sort_by(|a, b| {
            b.reputation
                .partial_cmp(&a.reputation)
                .unwrap()
                .then_with(|| b.stake.cmp(&a.stake))
        });

        node_list.into_iter().take(limit).collect()
    }

    /// Update node statistics
    pub async fn update_node_stats(&self, operator_id: &str, correct: bool) -> Result<(), String> {
        let mut nodes = self.nodes.write().await;
        let node = nodes.get_mut(operator_id).ok_or("Node not found")?;

        node.total_submissions += 1;
        if correct {
            node.correct_submissions += 1;
        }

        // Update reputation: correct_submissions / total_submissions
        node.reputation = (node.correct_submissions as f64) / (node.total_submissions as f64);

        Ok(())
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

    #[tokio::test]
    async fn test_oracle_node() {
        let config = OracleNodeConfig {
            operator_id: "oracle_node_1".to_string(),
            stake: 10000,
            data_sources: vec![],
            update_interval_ms: 5000,
            is_validator: false,
            reward_address: None,
        };

        let signing_key = SigningKey::generate(&mut rand::thread_rng());
        let node = OracleNode::new(config, signing_key);

        // Test data fetching
        let value = node.fetch_and_aggregate("BTC_USD").await.unwrap();
        assert_eq!(value.len(), 8); // f64 = 8 bytes

        // Test submission creation
        let submission = node.create_submission("BTC_USD", value).await.unwrap();
        assert_eq!(submission.feed_id, "BTC_USD");
        assert_eq!(submission.signature.len(), 64);
    }

    #[tokio::test]
    async fn test_reputation_system() {
        let config = OracleNodeConfig {
            operator_id: "oracle_node_2".to_string(),
            stake: 5000,
            data_sources: vec![],
            update_interval_ms: 5000,
            is_validator: true,
            reward_address: None,
        };

        let signing_key = SigningKey::generate(&mut rand::thread_rng());
        let node = OracleNode::new(config, signing_key);

        // Initial reputation
        assert_eq!(node.get_reputation().await, 1.0);

        // Correct submission
        node.update_reputation(true).await;
        assert!(node.get_reputation().await >= 0.99);

        // Incorrect submission
        node.update_reputation(false).await;
        assert!(node.get_reputation().await < 0.95);
    }

    #[tokio::test]
    async fn test_node_registry() {
        let registry = OracleNodeRegistry::new(1000);

        let node_info = OracleNodeInfo {
            operator_id: "node1".to_string(),
            stake: 5000,
            reputation: 0.95,
            is_validator: false,
            total_submissions: 100,
            correct_submissions: 95,
            registration_time: current_unix_time(),
        };

        registry.register_node(node_info).await.unwrap();

        let retrieved = registry.get_node("node1").await.unwrap();
        assert_eq!(retrieved.stake, 5000);
        assert_eq!(retrieved.reputation, 0.95);
    }
}
