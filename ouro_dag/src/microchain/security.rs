// src/microchain/security.rs
//! Microchain security modes
//!
//! Microchains support different security models:
//! - **SingleOwner**: Single wallet controls all transactions (most efficient)
//! - **Federated**: Multiple authorized keys with threshold signatures (shared control)

use anyhow::{bail, Result};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

/// Security mode for microchain transaction validation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum SecurityMode {
    /// Single owner has full control
    ///
    /// Most efficient mode - no multi-sig overhead.
    /// Suitable for personal wallets and single-user applications.
    SingleOwner {
        /// Owner's Ed25519 public key (32 bytes hex)
        owner_pubkey: String,
    },

    /// Federated security with threshold signatures
    ///
    /// Multiple authorized keys, requires M-of-N signatures.
    /// Suitable for shared wallets, DAOs, and multi-party control.
    Federated {
        /// List of authorized Ed25519 public keys (32 bytes hex each)
        authorized_keys: Vec<String>,

        /// Threshold: minimum number of signatures required
        threshold: usize,
    },
}

impl SecurityMode {
    /// Create a SingleOwner security mode
    pub fn single_owner(owner_pubkey: String) -> Self {
        SecurityMode::SingleOwner { owner_pubkey }
    }

    /// Create a Federated security mode
    pub fn federated(authorized_keys: Vec<String>, threshold: usize) -> Result<Self> {
        if authorized_keys.is_empty() {
            bail!("Federated mode requires at least one authorized key");
        }

        if threshold == 0 || threshold > authorized_keys.len() {
            bail!(
                "Threshold must be between 1 and {} (got {})",
                authorized_keys.len(),
                threshold
            );
        }

        Ok(SecurityMode::Federated {
            authorized_keys,
            threshold,
        })
    }

    /// Validate a transaction signature according to the security mode
    pub fn validate_transaction(
        &self,
        tx_hash: &[u8],
        signatures: &[TransactionSignature],
    ) -> Result<()> {
        match self {
            SecurityMode::SingleOwner { owner_pubkey } => {
                self.validate_single_owner(tx_hash, signatures, owner_pubkey)
            }
            SecurityMode::Federated {
                authorized_keys,
                threshold,
            } => self.validate_federated(tx_hash, signatures, authorized_keys, *threshold),
        }
    }

    /// Validate single owner signature
    fn validate_single_owner(
        &self,
        tx_hash: &[u8],
        signatures: &[TransactionSignature],
        owner_pubkey: &str,
    ) -> Result<()> {
        if signatures.len() != 1 {
            bail!(
                "SingleOwner mode requires exactly 1 signature, got {}",
                signatures.len()
            );
        }

        let sig = &signatures[0];

        // Verify signer matches owner
        if sig.signer_pubkey != owner_pubkey {
            bail!(
                "Signature from unauthorized key: expected {}, got {}",
                owner_pubkey,
                sig.signer_pubkey
            );
        }

        // Verify signature
        self.verify_ed25519_signature(tx_hash, &sig.signature, owner_pubkey)?;

        Ok(())
    }

    /// Validate federated multi-sig
    fn validate_federated(
        &self,
        tx_hash: &[u8],
        signatures: &[TransactionSignature],
        authorized_keys: &[String],
        threshold: usize,
    ) -> Result<()> {
        if signatures.len() < threshold {
            bail!(
                "Insufficient signatures: need {}, got {}",
                threshold,
                signatures.len()
            );
        }

        let mut valid_count = 0;
        let mut seen_keys = std::collections::HashSet::new();

        for sig in signatures {
            // Check if signer is authorized
            if !authorized_keys.contains(&sig.signer_pubkey) {
                bail!("Unauthorized signer: {}", sig.signer_pubkey);
            }

            // Check for duplicate signatures
            if !seen_keys.insert(&sig.signer_pubkey) {
                bail!("Duplicate signature from {}", sig.signer_pubkey);
            }

            // Verify signature
            if self
                .verify_ed25519_signature(tx_hash, &sig.signature, &sig.signer_pubkey)
                .is_ok()
            {
                valid_count += 1;
            } else {
                bail!("Invalid signature from {}", sig.signer_pubkey);
            }
        }

        if valid_count < threshold {
            bail!(
                "Threshold not met: need {} valid signatures, got {}",
                threshold,
                valid_count
            );
        }

        Ok(())
    }

    /// Verify Ed25519 signature
    fn verify_ed25519_signature(
        &self,
        message: &[u8],
        signature_hex: &str,
        pubkey_hex: &str,
    ) -> Result<()> {
        // Parse public key
        let pubkey_bytes = hex::decode(pubkey_hex)?;
        if pubkey_bytes.len() != 32 {
            bail!(
                "Invalid public key length: expected 32, got {}",
                pubkey_bytes.len()
            );
        }

        let mut pk_array = [0u8; 32];
        pk_array.copy_from_slice(&pubkey_bytes);
        let verifying_key = VerifyingKey::from_bytes(&pk_array)?;

        // Parse signature
        let sig_bytes = hex::decode(signature_hex)?;
        if sig_bytes.len() != 64 {
            bail!(
                "Invalid signature length: expected 64, got {}",
                sig_bytes.len()
            );
        }

        let mut sig_array = [0u8; 64];
        sig_array.copy_from_slice(&sig_bytes);
        let signature = Signature::from_bytes(&sig_array);

        // Verify
        verifying_key.verify(message, &signature)?;

        Ok(())
    }

    /// Get the authorized keys for this security mode
    pub fn authorized_keys(&self) -> Vec<String> {
        match self {
            SecurityMode::SingleOwner { owner_pubkey } => vec![owner_pubkey.clone()],
            SecurityMode::Federated {
                authorized_keys, ..
            } => authorized_keys.clone(),
        }
    }

    /// Check if a public key is authorized
    pub fn is_authorized(&self, pubkey: &str) -> bool {
        self.authorized_keys().contains(&pubkey.to_string())
    }
}

/// Transaction signature with signer identification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionSignature {
    /// Signer's Ed25519 public key (32 bytes hex)
    pub signer_pubkey: String,

    /// Ed25519 signature (64 bytes hex)
    pub signature: String,
}

impl TransactionSignature {
    /// Create a new transaction signature
    pub fn new(signer_pubkey: String, signature: String) -> Self {
        Self {
            signer_pubkey,
            signature,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;

    #[test]
    fn test_single_owner_valid() {
        let mut cng = OsRng;
        let signing_key = SigningKey::generate(&mut cng);
        let verifying_key = signing_key.verifying_key();
        let pubkey_hex = hex::encode(verifying_key.to_bytes());

        let mode = SecurityMode::single_owner(pubkey_hex.clone());

        let message = b"test transaction";
        let signature = signing_key.sign(message);
        let sig_hex = hex::encode(signature.to_bytes());

        let sigs = vec![TransactionSignature::new(pubkey_hex, sig_hex)];

        assert!(mode.validate_transaction(message, &sigs).is_ok());
    }

    #[test]
    fn test_single_owner_unauthorized() {
        let mut cng = OsRng;
        let owner_key = SigningKey::generate(&mut cng);
        let owner_pubkey = hex::encode(owner_key.verifying_key().to_bytes());

        let attacker_key = SigningKey::generate(&mut cng);
        let attacker_pubkey = hex::encode(attacker_key.verifying_key().to_bytes());

        let mode = SecurityMode::single_owner(owner_pubkey);

        let message = b"test transaction";
        let signature = attacker_key.sign(message);
        let sig_hex = hex::encode(signature.to_bytes());

        let sigs = vec![TransactionSignature::new(attacker_pubkey, sig_hex)];

        assert!(mode.validate_transaction(message, &sigs).is_err());
    }

    #[test]
    fn test_federated_valid_threshold() {
        let mut cng = OsRng;

        // Create 3 authorized keys
        let keys: Vec<SigningKey> = (0..3).map(|_| SigningKey::generate(&mut cng)).collect();
        let pubkeys: Vec<String> = keys
            .iter()
            .map(|k| hex::encode(k.verifying_key().to_bytes()))
            .collect();

        // 2-of-3 threshold
        let mode = SecurityMode::federated(pubkeys.clone(), 2).unwrap();

        let message = b"test transaction";

        // Sign with first 2 keys
        let sigs: Vec<TransactionSignature> = keys[0..2]
            .iter()
            .enumerate()
            .map(|(i, key)| {
                let signature = key.sign(message);
                TransactionSignature::new(pubkeys[i].clone(), hex::encode(signature.to_bytes()))
            })
            .collect();

        assert!(mode.validate_transaction(message, &sigs).is_ok());
    }

    #[test]
    fn test_federated_insufficient_signatures() {
        let mut cng = OsRng;

        let keys: Vec<SigningKey> = (0..3).map(|_| SigningKey::generate(&mut cng)).collect();
        let pubkeys: Vec<String> = keys
            .iter()
            .map(|k| hex::encode(k.verifying_key().to_bytes()))
            .collect();

        // 2-of-3 threshold
        let mode = SecurityMode::federated(pubkeys.clone(), 2).unwrap();

        let message = b"test transaction";

        // Only 1 signature (need 2)
        let sigs = vec![{
            let signature = keys[0].sign(message);
            TransactionSignature::new(pubkeys[0].clone(), hex::encode(signature.to_bytes()))
        }];

        assert!(mode.validate_transaction(message, &sigs).is_err());
    }

    #[test]
    fn test_federated_duplicate_signature() {
        let mut cng = OsRng;

        let keys: Vec<SigningKey> = (0..3).map(|_| SigningKey::generate(&mut cng)).collect();
        let pubkeys: Vec<String> = keys
            .iter()
            .map(|k| hex::encode(k.verifying_key().to_bytes()))
            .collect();

        let mode = SecurityMode::federated(pubkeys.clone(), 2).unwrap();

        let message = b"test transaction";

        // Same signature twice
        let sig = {
            let signature = keys[0].sign(message);
            TransactionSignature::new(pubkeys[0].clone(), hex::encode(signature.to_bytes()))
        };
        let sigs = vec![sig.clone(), sig];

        assert!(mode.validate_transaction(message, &sigs).is_err());
    }

    #[test]
    fn test_is_authorized() {
        let pubkey1 = "aabbccdd".to_string();
        let pubkey2 = "11223344".to_string();

        let mode = SecurityMode::single_owner(pubkey1.clone());
        assert!(mode.is_authorized(&pubkey1));
        assert!(!mode.is_authorized(&pubkey2));

        let mode2 = SecurityMode::federated(vec![pubkey1.clone(), pubkey2.clone()], 2).unwrap();
        assert!(mode2.is_authorized(&pubkey1));
        assert!(mode2.is_authorized(&pubkey2));
        assert!(!mode2.is_authorized("99999999"));
    }
}
