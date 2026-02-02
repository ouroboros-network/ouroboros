// src/microchain/integration.rs
//! Integration helpers for microchain security modes
//!
//! This module provides helper functions to integrate security validation
//! into microchain transaction processing.

use super::security::{SecurityMode, TransactionSignature};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Microchain security configuration store
///
/// Maps microchain IDs to their security modes
pub type SecurityModeStore = Arc<RwLock<HashMap<String, SecurityMode>>>;

/// Register a microchain with its security mode
///
/// # Example
///
/// ```rust,ignore
/// // Register a single-owner microchain
/// let mode = SecurityMode::single_owner(owner_pubkey);
/// register_microchain_security(
/// store.clone(),
/// "microchain_123",
/// mode,
/// ).await?;
///
/// // Register a federated microchain (2-of-3)
/// let keys = vec![pubkey1, pubkey2, pubkey3];
/// let mode = SecurityMode::federated(keys, 2)?;
/// register_microchain_security(
/// store.clone(),
/// "microchain_456",
/// mode,
/// ).await?;
/// ```
pub async fn register_microchain_security(
    store: SecurityModeStore,
    microchain_id: &str,
    mode: SecurityMode,
) -> Result<()> {
    let mut security_modes = store.write().await;
    security_modes.insert(microchain_id.to_string(), mode);
    Ok(())
}

/// Validate a microchain transaction
///
/// This should be called before accepting any microchain transaction.
///
/// # Example Integration
///
/// ```rust,ignore
/// // In your microchain transaction processing:
/// async fn process_microchain_transaction(
/// microchain_id: &str,
/// tx_data: &[u8],
/// signatures: Vec<TransactionSignature>,
/// security_store: SecurityModeStore,
/// ) -> Result<()> {
/// // 1. Compute transaction hash
/// let tx_hash = hash_transaction(tx_data);
///
/// // 2. Validate signatures according to security mode
/// validate_microchain_transaction(
/// security_store.clone(),
/// microchain_id,
/// &tx_hash,
/// &signatures,
/// ).await?;
///
/// // 3. Process transaction
/// apply_microchain_transaction(microchain_id, tx_data).await?;
///
/// Ok(())
/// }
/// ```
pub async fn validate_microchain_transaction(
    store: SecurityModeStore,
    microchain_id: &str,
    tx_hash: &[u8],
    signatures: &[TransactionSignature],
) -> Result<()> {
    let security_modes = store.read().await;

    let mode = security_modes.get(microchain_id).ok_or_else(|| {
        anyhow::anyhow!("Microchain {} not found or not registered", microchain_id)
    })?;

    mode.validate_transaction(tx_hash, signatures)?;

    Ok(())
}

/// Get the security mode for a microchain
pub async fn get_security_mode(
    store: SecurityModeStore,
    microchain_id: &str,
) -> Result<SecurityMode> {
    let security_modes = store.read().await;

    security_modes
        .get(microchain_id)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Microchain {} not found", microchain_id))
}

/// Check if a public key is authorized for a microchain
pub async fn is_key_authorized(
    store: SecurityModeStore,
    microchain_id: &str,
    pubkey: &str,
) -> Result<bool> {
    let mode = get_security_mode(store, microchain_id).await?;
    Ok(mode.is_authorized(pubkey))
}

/// Microchain registration request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MicrochainRegistration {
    /// Microchain unique identifier
    pub microchain_id: String,

    /// Security mode configuration
    pub security_mode: SecurityMode,

    /// Owner signature (proves ownership)
    pub owner_signature: String,
}

impl MicrochainRegistration {
    /// Validate the registration request
    pub fn validate(&self) -> Result<()> {
        // Ensure microchain ID is not empty
        if self.microchain_id.is_empty() {
            anyhow::bail!("Microchain ID cannot be empty");
        }

        // Validate security mode
        match &self.security_mode {
            SecurityMode::SingleOwner { owner_pubkey } => {
                if owner_pubkey.is_empty() {
                    anyhow::bail!("Owner public key cannot be empty");
                }
            }
            SecurityMode::Federated {
                authorized_keys,
                threshold,
            } => {
                if authorized_keys.is_empty() {
                    anyhow::bail!("Federated mode requires at least one authorized key");
                }
                if *threshold == 0 || *threshold > authorized_keys.len() {
                    anyhow::bail!(
                        "Invalid threshold: must be between 1 and {}",
                        authorized_keys.len()
                    );
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;

    #[tokio::test]
    async fn test_register_and_validate_single_owner() {
        let store: SecurityModeStore = Arc::new(RwLock::new(HashMap::new()));

        let mut cng = OsRng;
        let signing_key = SigningKey::generate(&mut cng);
        let pubkey = hex::encode(signing_key.verifying_key().to_bytes());

        // Register microchain
        let mode = SecurityMode::single_owner(pubkey.clone());
        register_microchain_security(store.clone(), "mc1", mode)
            .await
            .unwrap();

        // Create and sign transaction
        let tx_hash = b"test transaction";
        let signature = signing_key.sign(tx_hash);
        let signatures = vec![TransactionSignature::new(
            pubkey.clone(),
            hex::encode(signature.to_bytes()),
        )];

        // Validate
        let result =
            validate_microchain_transaction(store.clone(), "mc1", tx_hash, &signatures).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_register_and_validate_federated() {
        let store: SecurityModeStore = Arc::new(RwLock::new(HashMap::new()));

        let mut cng = OsRng;
        let keys: Vec<SigningKey> = (0..3).map(|_| SigningKey::generate(&mut cng)).collect();
        let pubkeys: Vec<String> = keys
            .iter()
            .map(|k| hex::encode(k.verifying_key().to_bytes()))
            .collect();

        // Register 2-of-3 microchain
        let mode = SecurityMode::federated(pubkeys.clone(), 2).unwrap();
        register_microchain_security(store.clone(), "mc2", mode)
            .await
            .unwrap();

        // Sign with 2 keys
        let tx_hash = b"federated transaction";
        let signatures: Vec<TransactionSignature> = keys[0..2]
            .iter()
            .enumerate()
            .map(|(i, key)| {
                let sig = key.sign(tx_hash);
                TransactionSignature::new(pubkeys[i].clone(), hex::encode(sig.to_bytes()))
            })
            .collect();

        // Validate
        let result =
            validate_microchain_transaction(store.clone(), "mc2", tx_hash, &signatures).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_is_key_authorized() {
        let store: SecurityModeStore = Arc::new(RwLock::new(HashMap::new()));

        let authorized_key = "aabbccdd".to_string();
        let unauthorized_key = "11223344".to_string();

        let mode = SecurityMode::single_owner(authorized_key.clone());
        register_microchain_security(store.clone(), "mc3", mode)
            .await
            .unwrap();

        assert!(is_key_authorized(store.clone(), "mc3", &authorized_key)
            .await
            .unwrap());
        assert!(!is_key_authorized(store.clone(), "mc3", &unauthorized_key)
            .await
            .unwrap());
    }

    #[test]
    fn test_microchain_registration_validation() {
        let valid_reg = MicrochainRegistration {
            microchain_id: "mc1".to_string(),
            security_mode: SecurityMode::single_owner("pubkey123".to_string()),
            owner_signature: "sig123".to_string(),
        };
        assert!(valid_reg.validate().is_ok());

        let invalid_reg = MicrochainRegistration {
            microchain_id: "".to_string(), // Empty ID
            security_mode: SecurityMode::single_owner("pubkey123".to_string()),
            owner_signature: "sig123".to_string(),
        };
        assert!(invalid_reg.validate().is_err());
    }
}
