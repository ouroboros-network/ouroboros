use serde::{Deserialize, Serialize};

/// Consensus type for microchain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConsensusType {
    /// Single validator (fast, centralized)
    SingleValidator,
    /// BFT consensus (slower, decentralized)
    Bft { validator_count: u32 },
}

impl Default for ConsensusType {
    fn default() -> Self {
        ConsensusType::SingleValidator
    }
}

/// How often to anchor to subchain/mainchain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnchorFrequency {
    /// Anchor every N blocks
    EveryNBlocks(u64),
    /// Anchor every N seconds
    EveryNSeconds(u64),
    /// Manual anchoring only
    Manual,
}

impl Default for AnchorFrequency {
    fn default() -> Self {
        AnchorFrequency::EveryNBlocks(100)
    }
}

/// Configuration for creating a microchain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MicrochainConfig {
    /// Microchain name
    pub name: String,

    /// Owner address
    pub owner: String,

    /// Consensus type
    #[serde(default)]
    pub consensus: ConsensusType,

    /// Anchor frequency
    #[serde(default)]
    pub anchor_frequency: AnchorFrequency,

    /// Maximum transactions per block
    #[serde(default = "default_max_txs")]
    pub max_txs_per_block: u32,

    /// Block time in seconds
    #[serde(default = "default_block_time")]
    pub block_time_secs: u64,
}

fn default_max_txs() -> u32 {
    1000
}

fn default_block_time() -> u64 {
    5
}

impl MicrochainConfig {
    pub fn new(name: impl Into<String>, owner: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            owner: owner.into(),
            consensus: ConsensusType::default(),
            anchor_frequency: AnchorFrequency::default(),
            max_txs_per_block: default_max_txs(),
            block_time_secs: default_block_time(),
        }
    }

    pub fn with_consensus(mut self, consensus: ConsensusType) -> Self {
        self.consensus = consensus;
        self
    }

    pub fn with_anchor_frequency(mut self, frequency: AnchorFrequency) -> Self {
        self.anchor_frequency = frequency;
        self
    }

    pub fn with_block_time(mut self, seconds: u64) -> Self {
        self.block_time_secs = seconds;
        self
    }
}

/// Microchain state information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MicrochainState {
    pub id: String,
    pub name: String,
    pub owner: String,
    pub block_height: u64,
    pub tx_count: u64,
    pub last_anchor_height: Option<u64>,
    pub created_at: String,
}

/// Transaction status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TxStatus {
    Pending,
    Confirmed,
    Failed,
    Anchored,
}

/// Balance information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Balance {
    pub address: String,
    pub balance: u64,
    pub pending: u64,
}

/// Block header
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockHeader {
    pub height: u64,
    pub hash: String,
    pub previous_hash: String,
    pub timestamp: String,
    pub tx_count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_microchain_config_builder() {
        let config = MicrochainConfig::new("TestChain", "ouro1abc...")
            .with_block_time(10)
            .with_consensus(ConsensusType::Bft { validator_count: 3 });

        assert_eq!(config.name, "TestChain");
        assert_eq!(config.block_time_secs, 10);
    }
}
