//! Cross-Chain Transfer Module
//!
//! Handles cross-chain message passing, fraud proofs, and bridge operations.

pub mod fraud_proofs;

// Re-export key types
pub use fraud_proofs::{
 FraudProofManager,
 CrossChainMessage,
 RelayedMessage,
 FraudProof,
 FraudProofType,
 RelayStatus,
};
