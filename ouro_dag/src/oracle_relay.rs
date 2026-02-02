// Cross-chain Oracle Relay
// Publishes oracle data from Ouro to other blockchains (Ethereum, BSC, etc.)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Supported target chains
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum TargetChain {
    Ethereum,
    BinanceSmartChain,
    Polygon,
    Arbitrum,
    Optimism,
    Avalanche,
}

impl TargetChain {
    /// Get chain ID
    pub fn chain_id(&self) -> u64 {
        match self {
            TargetChain::Ethereum => 1,
            TargetChain::BinanceSmartChain => 56,
            TargetChain::Polygon => 137,
            TargetChain::Arbitrum => 42161,
            TargetChain::Optimism => 10,
            TargetChain::Avalanche => 43114,
        }
    }

    /// Environment variable name for this chain's RPC URL
    pub fn rpc_env_var(&self) -> &str {
        match self {
            TargetChain::Ethereum => "ETH_RPC_URL",
            TargetChain::BinanceSmartChain => "BSC_RPC_URL",
            TargetChain::Polygon => "POLYGON_RPC_URL",
            TargetChain::Arbitrum => "ARBITRUM_RPC_URL",
            TargetChain::Optimism => "OPTIMISM_RPC_URL",
            TargetChain::Avalanche => "AVALANCHE_RPC_URL",
        }
    }

    /// Default RPC endpoint (public, rate-limited)
    fn default_rpc_endpoint(&self) -> &str {
        match self {
            // Public endpoints (rate-limited, for development only)
            TargetChain::Ethereum => "https://eth.llamarpc.com",
            TargetChain::BinanceSmartChain => "https://bsc-dataseed.binance.org",
            TargetChain::Polygon => "https://polygon-rpc.com",
            TargetChain::Arbitrum => "https://arb1.arbitrum.io/rpc",
            TargetChain::Optimism => "https://mainnet.optimism.io",
            TargetChain::Avalanche => "https://api.avax.network/ext/bc/C/rpc",
        }
    }

    /// Get RPC endpoint from environment or use default
    ///
    /// Configure via environment variables:
    /// - ETH_RPC_URL: Ethereum mainnet RPC (e.g., Alchemy, Infura)
    /// - BSC_RPC_URL: Binance Smart Chain RPC
    /// - POLYGON_RPC_URL: Polygon mainnet RPC
    /// - ARBITRUM_RPC_URL: Arbitrum One RPC
    /// - OPTIMISM_RPC_URL: Optimism mainnet RPC
    /// - AVALANCHE_RPC_URL: Avalanche C-Chain RPC
    pub fn rpc_endpoint(&self) -> String {
        std::env::var(self.rpc_env_var())
            .unwrap_or_else(|_| self.default_rpc_endpoint().to_string())
    }

    /// Get oracle contract address (deployed on target chain)
    pub fn oracle_contract_address(&self) -> &str {
        // These would be actual deployed contract addresses
        match self {
            TargetChain::Ethereum => "0x0000000000000000000000000000000000000000",
            TargetChain::BinanceSmartChain => "0x0000000000000000000000000000000000000000",
            TargetChain::Polygon => "0x0000000000000000000000000000000000000000",
            TargetChain::Arbitrum => "0x0000000000000000000000000000000000000000",
            TargetChain::Optimism => "0x0000000000000000000000000000000000000000",
            TargetChain::Avalanche => "0x0000000000000000000000000000000000000000",
        }
    }
}

/// Oracle data update for cross-chain relay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleUpdate {
    /// Feed ID (e.g., "BTC_USD")
    pub feed_id: String,
    /// Value (price in smallest unit)
    pub value: Vec<u8>,
    /// Timestamp
    pub timestamp: u64,
    /// Confidence score (0.0 to 1.0)
    pub confidence: f64,
    /// Number of oracle nodes that submitted
    pub num_nodes: usize,
    /// Signatures from oracle nodes
    pub signatures: Vec<Vec<u8>>,
}

/// Oracle relay (publishes to other chains)
pub struct OracleRelay {
    /// Target chains to relay to
    target_chains: Vec<TargetChain>,
    /// Relay frequency (seconds)
    relay_interval: u64,
    /// Minimum confidence to relay
    min_confidence: f64,
}

impl OracleRelay {
    pub fn new(target_chains: Vec<TargetChain>) -> Self {
        Self {
            target_chains,
            relay_interval: 60,   // Relay every 60 seconds
            min_confidence: 0.67, // 67% confidence minimum
        }
    }

    /// Relay oracle update to target chain
    pub async fn relay_update(
        &self,
        chain: &TargetChain,
        update: &OracleUpdate,
    ) -> Result<String, String> {
        // Check confidence threshold
        if update.confidence < self.min_confidence {
            return Err(format!(
                "Confidence {} below threshold {}",
                update.confidence, self.min_confidence
            ));
        }

        log::info!(
            "Relaying {} to {} (confidence: {:.2}%)",
            update.feed_id,
            format!("{:?}", chain),
            update.confidence * 100.0
        );

        // In production, this would:
        // 1. Connect to target chain via RPC
        // 2. Call updateOracleData() on oracle contract
        // 3. Submit transaction with signatures
        // 4. Wait for confirmation
        // 5. Return transaction hash

        // For now, simulate relay
        let tx_hash = format!(
            "0x{}",
            hex::encode(sha2_hash(&format!(
                "{}{}{}",
                update.feed_id,
                update.timestamp,
                chain.chain_id()
            )))
        );

        log::info!(
            "Relayed to {} - TX: {} (simulated)",
            format!("{:?}", chain),
            tx_hash
        );

        Ok(tx_hash)
    }

    /// Relay to all configured chains
    pub async fn relay_to_all(
        &self,
        update: &OracleUpdate,
    ) -> HashMap<TargetChain, Result<String, String>> {
        let mut results = HashMap::new();

        for chain in &self.target_chains {
            let result = self.relay_update(chain, update).await;
            results.insert(chain.clone(), result);
        }

        results
    }

    /// Run continuous relay loop
    pub async fn run(&self, oracle_manager: std::sync::Arc<crate::oracle::OracleManager>) {
        log::info!(
            "Starting oracle relay to {} chains",
            self.target_chains.len()
        );

        let interval = tokio::time::Duration::from_secs(self.relay_interval);

        loop {
            // Get all feeds
            let feeds = oracle_manager.list_feeds().await;

            for feed_id in feeds {
                if let Some(aggregated) = oracle_manager.get_feed(&feed_id).await {
                    // Create update
                    let update = OracleUpdate {
                        feed_id: feed_id.clone(),
                        value: aggregated.value.clone(),
                        timestamp: aggregated.timestamp,
                        confidence: aggregated.confidence,
                        num_nodes: aggregated.num_submissions,
                        signatures: vec![], // TODO: Collect signatures
                    };

                    // Relay to all chains
                    let results = self.relay_to_all(&update).await;

                    // Log results
                    for (chain, result) in results {
                        match result {
                            Ok(tx_hash) => {
                                log::info!("✓ Relayed {} to {:?}: {}", feed_id, chain, tx_hash);
                            }
                            Err(e) => {
                                log::error!("✗ Failed to relay {} to {:?}: {}", feed_id, chain, e);
                            }
                        }
                    }
                }
            }

            // Wait for next interval
            tokio::time::sleep(interval).await;
        }
    }
}

/// Oracle contract interface (Solidity-style ABI)
/// This is what gets deployed on target chains
pub struct OracleContractInterface;

impl OracleContractInterface {
    /// Generate Solidity contract code
    pub fn solidity_contract() -> &'static str {
        r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract OuroOracle {
    struct OracleData {
        bytes value;
        uint256 timestamp;
        uint256 confidence; // Basis points (6700 = 67%)
        uint256 numNodes;
    }

    mapping(string => OracleData) public feeds;
    mapping(address => bool) public relayers;
    address public owner;

    event OracleUpdated(
        string indexed feedId,
        bytes value,
        uint256 timestamp,
        uint256 confidence
    );

    modifier onlyRelayer() {
        require(relayers[msg.sender] || msg.sender == owner, "Not authorized");
        _;
    }

    constructor() {
        owner = msg.sender;
        relayers[msg.sender] = true;
    }

    function addRelayer(address relayer) external {
        require(msg.sender == owner, "Not owner");
        relayers[relayer] = true;
    }

    function updateOracleData(
        string memory feedId,
        bytes memory value,
        uint256 timestamp,
        uint256 confidence,
        uint256 numNodes
    ) external onlyRelayer {
        require(confidence >= 6700, "Confidence too low"); // 67% minimum
        require(timestamp > feeds[feedId].timestamp, "Stale data");

        feeds[feedId] = OracleData({
            value: value,
            timestamp: timestamp,
            confidence: confidence,
            numNodes: numNodes
        });

        emit OracleUpdated(feedId, value, timestamp, confidence);
    }

    function getPrice(string memory feedId) external view returns (uint256) {
        require(feeds[feedId].timestamp > 0, "Feed not found");
        require(block.timestamp - feeds[feedId].timestamp < 300, "Data too old");

        // Decode bytes to uint256 (assuming 8-byte little-endian float)
        bytes memory valueBytes = feeds[feedId].value;
        require(valueBytes.length >= 8, "Invalid value");

        // Convert 8-byte little-endian to uint256
        uint256 price = 0;
        for (uint i = 0; i < 8; i++) {
            price |= uint256(uint8(valueBytes[i])) << (i * 8);
        }

        return price;
    }

    function getOracleData(string memory feedId) external view returns (
        bytes memory value,
        uint256 timestamp,
        uint256 confidence,
        uint256 numNodes
    ) {
        OracleData memory data = feeds[feedId];
        return (data.value, data.timestamp, data.confidence, data.numNodes);
    }
}
"#
    }
}

fn sha2_hash(data: &str) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data.as_bytes());
    let result = hasher.finalize();
    result.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_target_chains() {
        assert_eq!(TargetChain::Ethereum.chain_id(), 1);
        assert_eq!(TargetChain::BinanceSmartChain.chain_id(), 56);
        assert_eq!(TargetChain::Polygon.chain_id(), 137);
    }

    #[tokio::test]
    async fn test_oracle_relay() {
        let relay = OracleRelay::new(vec![TargetChain::Ethereum, TargetChain::BinanceSmartChain]);

        let update = OracleUpdate {
            feed_id: "BTC_USD".to_string(),
            value: 50000u64.to_le_bytes().to_vec(),
            timestamp: 1234567890,
            confidence: 0.95,
            num_nodes: 10,
            signatures: vec![],
        };

        let result = relay.relay_update(&TargetChain::Ethereum, &update).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_low_confidence_rejection() {
        let relay = OracleRelay::new(vec![TargetChain::Ethereum]);

        let update = OracleUpdate {
            feed_id: "BTC_USD".to_string(),
            value: vec![],
            timestamp: 1234567890,
            confidence: 0.5, // Below 67% threshold
            num_nodes: 3,
            signatures: vec![],
        };

        let result = relay.relay_update(&TargetChain::Ethereum, &update).await;
        assert!(result.is_err());
    }
}
