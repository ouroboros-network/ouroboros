// src/zk_proofs/state_proof.rs
//! ZK-Light Compression - State Proofs
//!
//! Generates compact cryptographic commitments to the global state that
//! Light nodes can download and verify without replaying the full chain.
//!
//! Current implementation: SHA-256 hash chain commitment.
//! Future: Replace with recursive SNARK/STARK proofs via arkworks or halo2.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Represents a compressed state proof
///
/// Light nodes download this from Heavy nodes to sync state without
/// replaying the entire blockchain history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateProof {
    /// Merkle root of the state tree at this height
    pub root_hash: [u8; 32],
    /// Cryptographic proof data (hash-chain commitment)
    pub proof_data: Vec<u8>,
    /// Block height this proof covers
    pub block_height: u64,
    /// When this proof was generated (unix timestamp)
    pub timestamp: u64,
}

impl StateProof {
    /// Verify the proof's integrity by checking the commitment chain.
    ///
    /// Verifies that proof_data contains a valid SHA-256 commitment
    /// that binds the root_hash to the block_height.
    pub fn verify(&self) -> bool {
        if self.proof_data.len() < 32 {
            return false;
        }

        // Extract the commitment from proof_data
        // Structure: [32-byte commitment | 8-byte height | 8-byte timestamp]
        if self.proof_data.len() < 48 {
            return false;
        }

        let commitment = &self.proof_data[..32];
        let encoded_height = &self.proof_data[32..40];
        let encoded_timestamp = &self.proof_data[40..48];

        // Verify height matches
        let proof_height = u64::from_le_bytes(
            encoded_height.try_into().unwrap_or([0u8; 8]),
        );
        if proof_height != self.block_height {
            return false;
        }

        // Verify timestamp matches
        let proof_timestamp = u64::from_le_bytes(
            encoded_timestamp.try_into().unwrap_or([0u8; 8]),
        );
        if proof_timestamp != self.timestamp {
            return false;
        }

        // Recompute commitment: SHA256(root_hash || height || timestamp)
        let expected = compute_commitment(&self.root_hash, self.block_height, self.timestamp);
        commitment == expected.as_slice()
    }

    /// Unique identifier for this proof (for dedup and caching)
    pub fn id(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.root_hash);
        hasher.update(self.block_height.to_le_bytes());
        hex::encode(hasher.finalize())
    }
}

/// Compute a SHA-256 commitment binding root_hash to block_height and timestamp
fn compute_commitment(root_hash: &[u8; 32], height: u64, timestamp: u64) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"ouroboros-state-proof-v1");
    hasher.update(root_hash);
    hasher.update(height.to_le_bytes());
    hasher.update(timestamp.to_le_bytes());
    let result = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&result);
    out
}

/// Manages the generation of state proofs on Heavy nodes
pub struct ProofGenerator {
    /// Circuit parameters (reserved for future SNARK setup)
    pub circuit_params: Vec<u8>,
}

impl ProofGenerator {
    pub fn new() -> Self {
        Self {
            circuit_params: Vec::new(),
        }
    }

    /// Generate a state proof for the given root hash and block height.
    ///
    /// Creates a SHA-256 commitment chain that Light nodes can verify
    /// without needing the full state tree.
    pub fn generate_state_proof(&self, root: [u8; 32], height: u64) -> StateProof {
        let timestamp = chrono::Utc::now().timestamp() as u64;

        // Build proof_data: commitment || height || timestamp
        let commitment = compute_commitment(&root, height, timestamp);
        let mut proof_data = Vec::with_capacity(48);
        proof_data.extend_from_slice(&commitment);
        proof_data.extend_from_slice(&height.to_le_bytes());
        proof_data.extend_from_slice(&timestamp.to_le_bytes());

        StateProof {
            root_hash: root,
            proof_data,
            block_height: height,
            timestamp,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_and_verify() {
        let gen = ProofGenerator::new();
        let root = [42u8; 32];
        let proof = gen.generate_state_proof(root, 1000);

        assert_eq!(proof.block_height, 1000);
        assert_eq!(proof.root_hash, root);
        assert_eq!(proof.proof_data.len(), 48);
        assert!(proof.verify(), "Valid proof should verify");
    }

    #[test]
    fn test_tampered_height_fails() {
        let gen = ProofGenerator::new();
        let mut proof = gen.generate_state_proof([1u8; 32], 500);
        proof.block_height = 501; // Tamper with height
        assert!(!proof.verify(), "Tampered height should fail verification");
    }

    #[test]
    fn test_tampered_proof_data_fails() {
        let gen = ProofGenerator::new();
        let mut proof = gen.generate_state_proof([1u8; 32], 500);
        proof.proof_data[0] ^= 0xFF; // Flip a byte in commitment
        assert!(!proof.verify(), "Tampered proof data should fail verification");
    }

    #[test]
    fn test_empty_proof_fails() {
        let proof = StateProof {
            root_hash: [0u8; 32],
            proof_data: vec![],
            block_height: 0,
            timestamp: 0,
        };
        assert!(!proof.verify(), "Empty proof should fail");
    }

    #[test]
    fn test_proof_id_deterministic() {
        let gen = ProofGenerator::new();
        let proof1 = gen.generate_state_proof([5u8; 32], 100);
        let proof2 = gen.generate_state_proof([5u8; 32], 100);
        assert_eq!(proof1.id(), proof2.id(), "Same root+height should give same ID");
    }
}
