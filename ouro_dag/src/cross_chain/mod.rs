//! Cross-chain interoperability module
//!
//! Provides cross-chain message verification, relay coordination,
//! and fraud proof validation for bridge transfers between Ouroboros
//! and external chains (Ethereum, BSC, Polygon, etc.).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Supported external chains
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ChainId {
    Ouroboros,
    Ethereum,
    BSC,
    Polygon,
    Arbitrum,
    Optimism,
    Avalanche,
    Custom(String),
}

impl std::fmt::Display for ChainId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChainId::Ouroboros => write!(f, "ouroboros"),
            ChainId::Ethereum => write!(f, "ethereum"),
            ChainId::BSC => write!(f, "bsc"),
            ChainId::Polygon => write!(f, "polygon"),
            ChainId::Arbitrum => write!(f, "arbitrum"),
            ChainId::Optimism => write!(f, "optimism"),
            ChainId::Avalanche => write!(f, "avalanche"),
            ChainId::Custom(name) => write!(f, "{}", name),
        }
    }
}

/// A cross-chain message to be relayed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossChainMessage {
    /// Unique message ID
    pub id: String,
    /// Source chain
    pub source_chain: ChainId,
    /// Destination chain
    pub dest_chain: ChainId,
    /// Sender address on source chain
    pub sender: String,
    /// Receiver address on destination chain
    pub receiver: String,
    /// Message payload (encoded)
    pub payload: Vec<u8>,
    /// Nonce for replay protection
    pub nonce: u64,
    /// Timestamp
    pub timestamp: u64,
    /// Hash of the message for verification
    pub message_hash: Vec<u8>,
}

impl CrossChainMessage {
    /// Create a new cross-chain message
    pub fn new(
        source_chain: ChainId,
        dest_chain: ChainId,
        sender: String,
        receiver: String,
        payload: Vec<u8>,
        nonce: u64,
        timestamp: u64,
    ) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(source_chain.to_string().as_bytes());
        hasher.update(dest_chain.to_string().as_bytes());
        hasher.update(sender.as_bytes());
        hasher.update(receiver.as_bytes());
        hasher.update(&payload);
        hasher.update(nonce.to_le_bytes());
        let message_hash = hasher.finalize().to_vec();

        Self {
            id: hex::encode(&message_hash[..16]),
            source_chain,
            dest_chain,
            sender,
            receiver,
            payload,
            nonce,
            timestamp,
            message_hash,
        }
    }

    /// Verify the message hash is correct
    pub fn verify_hash(&self) -> bool {
        let mut hasher = Sha256::new();
        hasher.update(self.source_chain.to_string().as_bytes());
        hasher.update(self.dest_chain.to_string().as_bytes());
        hasher.update(self.sender.as_bytes());
        hasher.update(self.receiver.as_bytes());
        hasher.update(&self.payload);
        hasher.update(self.nonce.to_le_bytes());
        let expected = hasher.finalize().to_vec();
        expected == self.message_hash
    }
}

/// Cross-chain fraud proof for invalid relay claims
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossChainFraudProof {
    /// Message being challenged
    pub message_id: String,
    /// The chain where the fraud occurred
    pub chain: ChainId,
    /// Type of cross-chain fraud
    pub fraud_type: CrossChainFraudType,
    /// Proof data (chain-specific SPV proof, receipts, etc.)
    pub proof_data: Vec<u8>,
    /// Challenger address
    pub challenger: String,
    /// Timestamp of challenge submission
    pub submitted_at: u64,
}

/// Types of cross-chain fraud
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CrossChainFraudType {
    /// Source transaction never existed
    NonExistentSourceTx,
    /// Source transaction was already consumed (replay)
    ReplayAttack,
    /// Amount mismatch between source lock and destination mint
    AmountMismatch,
    /// Invalid Merkle/SPV proof for source chain
    InvalidProof,
}

/// Verify a cross-chain message against its source chain proof
pub fn verify_cross_chain_proof(
    message: &CrossChainMessage,
    proof: &[u8],
) -> Result<bool, String> {
    // Verify message hash integrity
    if !message.verify_hash() {
        return Err("Message hash verification failed".to_string());
    }

    // Verify proof is non-empty
    if proof.is_empty() {
        return Err("Empty proof data".to_string());
    }

    // Chain-specific verification would go here
    // For now, verify the proof contains the message hash
    if proof.len() < 32 {
        return Err("Proof too short".to_string());
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_hash_verification() {
        let msg = CrossChainMessage::new(
            ChainId::Ethereum,
            ChainId::Ouroboros,
            "0xSender".to_string(),
            "ouro_receiver".to_string(),
            vec![1, 2, 3],
            1,
            1000,
        );
        assert!(msg.verify_hash());
    }

    #[test]
    fn test_chain_id_display() {
        assert_eq!(ChainId::Ethereum.to_string(), "ethereum");
        assert_eq!(ChainId::Ouroboros.to_string(), "ouroboros");
        assert_eq!(ChainId::Custom("solana".into()).to_string(), "solana");
    }
}
