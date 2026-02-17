// src/multisig/mod.rs
//! Multi-signature coordinator for decentralized anchor posting
//!
//! This module implements a threshold signature scheme where M-of-N validators
//! must sign an anchor before it's posted to the mainchain. This eliminates
//! the single point of failure of having one anchor operator.
//!
//! Design:
//! - Each validator has an Ed25519 keypair for signing anchors
//! - Anchors require M-of-N signatures (e.g., 3-of-5)
//! - Validators exchange partial signatures via BFT gossip
//! - Once threshold is met, any validator can post the anchor
//!
//! Security properties:
//! - Byzantine fault tolerant: system works even if (N-M) validators are offline/malicious
//! - No single point of failure: requires collusion of M validators to forge
//! - Permissionless verification: anyone can verify the multi-sig

use anyhow::{bail, Result};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Configuration for multi-sig threshold
#[derive(Clone, Debug)]
pub struct MultiSigConfig {
    /// Number of signatures required (M in M-of-N)
    pub threshold: usize,
    /// Total number of validators (N in M-of-N)
    pub total_validators: usize,
    /// Map of validator ID to their Ed25519 public key
    pub validator_pubkeys: HashMap<String, VerifyingKey>,
}

impl MultiSigConfig {
    /// Create a new multi-sig config with threshold validation
    pub fn new(threshold: usize, validator_pubkeys: HashMap<String, VerifyingKey>) -> Result<Self> {
        let total = validator_pubkeys.len();

        if threshold == 0 {
            bail!("Threshold must be at least 1");
        }
        if threshold > total {
            bail!("Threshold {} exceeds total validators {}", threshold, total);
        }
        if total < 3 {
            bail!("Multi-sig requires at least 3 validators for Byzantine fault tolerance");
        }

        // Recommended: threshold should be > 2/3 of total for BFT safety
        let min_recommended = (total * 2 / 3) + 1;
        if threshold < min_recommended {
            log::warn!(
                "WARNING Threshold {} is below recommended minimum {} for {} validators",
                threshold,
                min_recommended,
                total
            );
        }

        Ok(Self {
            threshold,
            total_validators: total,
            validator_pubkeys,
        })
    }
}

/// A partial signature from one validator
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PartialSignature {
    /// Validator ID who created this signature
    pub validator_id: String,
    /// Ed25519 signature bytes
    pub signature: Vec<u8>,
    /// Unix timestamp when signature was created
    pub timestamp: i64,
}

/// Multi-signature aggregate containing all partial signatures
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MultiSignature {
    /// The anchor root hash that was signed
    pub anchor_root: Vec<u8>,
    /// Subchain UUID this anchor is for
    pub subchain: Uuid,
    /// Block height being anchored
    pub block_height: i64,
    /// All partial signatures collected
    pub partial_signatures: Vec<PartialSignature>,
    /// Unix timestamp when threshold was reached
    pub completed_at: Option<i64>,
}

impl MultiSignature {
    /// Create a new empty multi-signature
    pub fn new(anchor_root: Vec<u8>, subchain: Uuid, block_height: i64) -> Self {
        Self {
            anchor_root,
            subchain,
            block_height,
            partial_signatures: Vec::new(),
            completed_at: None,
        }
    }

    /// Add a partial signature from a validator
    pub fn add_signature(&mut self, partial: PartialSignature) -> Result<()> {
        // Check for duplicate validator
        if self
            .partial_signatures
            .iter()
            .any(|p| p.validator_id == partial.validator_id)
        {
            bail!(
                "Duplicate signature from validator {}",
                partial.validator_id
            );
        }

        self.partial_signatures.push(partial);
        Ok(())
    }

    /// Check if threshold is met
    pub fn is_complete(&self, threshold: usize) -> bool {
        self.partial_signatures.len() >= threshold
    }

    /// Verify all signatures against validator public keys
    pub fn verify(&self, config: &MultiSigConfig) -> Result<()> {
        if !self.is_complete(config.threshold) {
            bail!(
                "Insufficient signatures: {} < {}",
                self.partial_signatures.len(),
                config.threshold
            );
        }

        // Create the message that was signed: anchor_root || subchain || block_height
        let mut message = self.anchor_root.clone();
        message.extend_from_slice(self.subchain.as_bytes());
        message.extend_from_slice(&self.block_height.to_le_bytes());

        // Verify each partial signature
        let mut verified_count = 0;
        for partial in &self.partial_signatures {
            let pubkey = config
                .validator_pubkeys
                .get(&partial.validator_id)
                .ok_or_else(|| anyhow::anyhow!("Unknown validator: {}", partial.validator_id))?;

            let signature = Signature::from_bytes(
                partial
                    .signature
                    .as_slice()
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("Invalid signature length"))?,
            );

            pubkey.verify(&message, &signature).map_err(|e| {
                anyhow::anyhow!(
                    "Signature verification failed for {}: {}",
                    partial.validator_id,
                    e
                )
            })?;

            verified_count += 1;
        }

        if verified_count < config.threshold {
            bail!(
                "Only {} valid signatures, need {}",
                verified_count,
                config.threshold
            );
        }

        Ok(())
    }
}

/// Multi-sig coordinator for managing anchor signatures
#[derive(Clone, Debug)]
pub struct MultiSigCoordinator {
    pub config: MultiSigConfig,
    /// Pending multi-signatures indexed by anchor root hash
    pending: std::sync::Arc<parking_lot::RwLock<HashMap<Vec<u8>, MultiSignature>>>,
}

impl MultiSigCoordinator {
    pub fn new(config: MultiSigConfig) -> Self {
        Self {
            config,
            pending: std::sync::Arc::new(parking_lot::RwLock::new(HashMap::new())),
        }
    }

    /// Load validator public keys from database
    pub async fn load_validator_keys() -> Result<HashMap<String, VerifyingKey>> {
        // TODO_ROCKSDB: Query validator keys from RocksDB
        Ok(HashMap::new())
    }

    /// Submit a partial signature for an anchor
    pub fn submit_partial_signature(
        &self,
        anchor_root: Vec<u8>,
        subchain: Uuid,
        block_height: i64,
        partial: PartialSignature,
    ) -> Result<bool> {
        let mut pending = self.pending.write();

        let multisig = pending
            .entry(anchor_root.clone())
            .or_insert_with(|| MultiSignature::new(anchor_root.clone(), subchain, block_height));

        multisig.add_signature(partial)?;

        let is_complete = multisig.is_complete(self.config.threshold);
        if is_complete && multisig.completed_at.is_none() {
            multisig.completed_at = Some(chrono::Utc::now().timestamp());
            log::info!(
                " Multi-sig threshold reached for anchor {} (height {}): {}/{} signatures",
                hex::encode(&anchor_root[..8]),
                block_height,
                multisig.partial_signatures.len(),
                self.config.total_validators
            );
        }

        Ok(is_complete)
    }

    /// Get a completed multi-signature if threshold is met
    pub fn get_completed_multisig(&self, anchor_root: &[u8]) -> Option<MultiSignature> {
        let pending = self.pending.read();
        pending
            .get(anchor_root)
            .filter(|ms| ms.is_complete(self.config.threshold))
            .cloned()
    }

    /// Remove a completed multi-signature (after posting to mainchain)
    pub fn remove_completed(&self, anchor_root: &[u8]) {
        self.pending.write().remove(anchor_root);
    }

    /// Get number of signatures collected for an anchor
    pub fn get_signature_count(&self, anchor_root: &[u8]) -> usize {
        self.pending
            .read()
            .get(anchor_root)
            .map(|ms| ms.partial_signatures.len())
            .unwrap_or(0)
    }

    /// Verify a multi-signature
    pub fn verify_multisig(&self, multisig: &MultiSignature) -> Result<()> {
        multisig.verify(&self.config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;

    fn create_test_config() -> (MultiSigConfig, Vec<SigningKey>) {
        let mut keypairs = Vec::new();
        let mut pubkeys = HashMap::new();

        for i in 0..5 {
            let mut csprng = OsRng;
            let signing_key = SigningKey::generate(&mut csprng);
            pubkeys.insert(format!("validator-{}", i), signing_key.verifying_key());
            keypairs.push(signing_key);
        }

        let config = MultiSigConfig::new(3, pubkeys).unwrap();
        (config, keypairs)
    }

    #[test]
    fn test_multisig_threshold() {
        let (config, keypairs) = create_test_config();
        let coordinator = MultiSigCoordinator::new(config);

        let anchor_root = vec![1, 2, 3, 4];
        let subchain = Uuid::new_v4();
        let block_height: i64 = 100;

        // Create message to sign
        let mut message = anchor_root.clone();
        message.extend_from_slice(subchain.as_bytes());
        message.extend_from_slice(&block_height.to_le_bytes());

        // Submit signatures from 3 validators (threshold)
        for i in 0..3 {
            let signature = keypairs[i].sign(&message);
            let partial = PartialSignature {
                validator_id: format!("validator-{}", i),
                signature: signature.to_bytes().to_vec(),
                timestamp: chrono::Utc::now().timestamp(),
            };

            let is_complete = coordinator
                .submit_partial_signature(anchor_root.clone(), subchain, block_height, partial)
                .unwrap();

            if i < 2 {
                assert!(!is_complete, "Should not be complete yet");
            } else {
                assert!(is_complete, "Should be complete at threshold");
            }
        }

        // Verify the multi-sig
        let multisig = coordinator.get_completed_multisig(&anchor_root).unwrap();
        coordinator.verify_multisig(&multisig).unwrap();
    }
}
