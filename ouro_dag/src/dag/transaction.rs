// src/dag/transaction.rs
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub id: Uuid,
    pub sender: String,
    pub recipient: String,
    pub amount: u64,
    pub timestamp: DateTime<Utc>,
    pub parents: Vec<Uuid>,
    pub signature: String,
    pub public_key: String,

    /// Transaction fee (higher fee = higher priority in mempool)
    #[serde(default)]
    pub fee: u64,

    /// Optional JSON payload for contract calls / extended semantics.
    /// Example: {"contract":"sbt","op":"mint","sbt_id":"abc","meta":{...}}
    pub payload: Option<String>,

    /// Chain ID for replay protection (Phase 6 security hardening)
    /// Prevents transactions from one chain being replayed on another chain
    /// Standard value: "ouroboros-mainnet-1"
    #[serde(default = "default_chain_id")]
    pub chain_id: String,

    /// Nonce for transaction ordering and replay protection
    /// Each sender must increment their nonce for each transaction
    /// Prevents double-spending and ensures correct transaction ordering
    #[serde(default)]
    pub nonce: u64,
}

/// Default chain ID for mainnet
fn default_chain_id() -> String {
    "ouroboros-mainnet-1".to_string()
}

impl Transaction {
    /// Verify that transaction has correct chain ID
    /// This prevents replay attacks across different chains (mainnet, testnet, etc.)
    pub fn verify_chain_id(&self, expected_chain_id: &str) -> Result<(), String> {
        if self.chain_id != expected_chain_id {
            return Err(format!(
                "Invalid chain ID: got '{}', expected '{}'",
                self.chain_id, expected_chain_id
            ));
        }
        Ok(())
    }

    /// Get the signing message (includes chain_id and nonce for replay protection)
    /// This message is what gets signed by the sender's private key
    pub fn signing_message(&self) -> Vec<u8> {
        let mut msg = Vec::new();

        // Include chain_id to prevent cross-chain replay
        msg.extend_from_slice(self.chain_id.as_bytes());

        // Include nonce to prevent replay within same chain
        msg.extend_from_slice(&self.nonce.to_le_bytes());

        // Include transaction data
        msg.extend_from_slice(self.id.as_bytes());
        msg.extend_from_slice(self.sender.as_bytes());
        msg.extend_from_slice(self.recipient.as_bytes());
        msg.extend_from_slice(&self.amount.to_le_bytes());
        msg.extend_from_slice(&self.fee.to_le_bytes());

        // Include timestamp
        msg.extend_from_slice(&self.timestamp.timestamp().to_le_bytes());

        // Include parents
        for parent in &self.parents {
            msg.extend_from_slice(parent.as_bytes());
        }

        // Include payload if present
        if let Some(ref payload) = self.payload {
            msg.extend_from_slice(payload.as_bytes());
        }

        msg
    }

    /// Verify transaction nonce against expected nonce
    /// Prevents replay attacks and ensures correct transaction ordering
    pub fn verify_nonce(&self, expected_nonce: u64) -> Result<(), String> {
        if self.nonce != expected_nonce {
            return Err(format!(
                "Invalid nonce: got {}, expected {}",
                self.nonce, expected_nonce
            ));
        }
        Ok(())
    }
}
