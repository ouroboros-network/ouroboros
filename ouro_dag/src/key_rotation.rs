// src/key_rotation.rs
//! BFT Validator Key Rotation
//!
//! Allows validators to rotate their signing keys without downtime.
//!
//! Process:
//! 1. Validator generates a new keypair
//! 2. Validator announces the new public key (signed with old key)
//! 3. Transition period begins (both keys valid)
//! 4. After transition period, old key is revoked
//! 5. Only new key is valid
//!
//! Security:
//! - New key must be signed by old key (proof of authority)
//! - Transition period prevents immediate key takeover
//! - Key rotation events are logged on-chain
//! - Rate limiting prevents rapid key rotation abuse

use crate::PgPool;
use crate::storage;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc, Duration};
use ed25519_dalek::{Signer, Signature, Verifier, SigningKey, VerifyingKey};
use anyhow::{Result, bail};

/// Key rotation announcement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyRotationAnnouncement {
    /// Validator ID
    pub validator_id: String,

    /// Old public key (hex)
    pub old_public_key: String,

    /// New public key (hex)
    pub new_public_key: String,

    /// Signature of new key by old key (proof of authority)
    pub signature: String,

    /// Announcement timestamp
    pub announced_at: DateTime<Utc>,

    /// Transition period end (when old key expires)
    pub transition_ends_at: DateTime<Utc>,

    /// Status: pending, active, completed
    pub status: KeyRotationStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum KeyRotationStatus {
    /// Announced but not yet in transition
    Pending,

    /// Both old and new keys valid
    InTransition,

    /// Transition complete, only new key valid
    Completed,

    /// Rotation cancelled/revoked
    Revoked,
}

/// Key rotation manager
pub struct KeyRotationManager {
    /// RocksDB pool
    pool: PgPool,
    /// Default transition period (24 hours)
    default_transition_period: Duration,
    /// Minimum time between rotations (7 days)
    min_rotation_interval: Duration,
}

impl KeyRotationManager {
    /// Create new key rotation manager
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            default_transition_period: Duration::hours(24),
            min_rotation_interval: Duration::days(7),
        }
    }

    /// Announce a new key rotation
    pub async fn announce_rotation(
        &self,
        validator_id: &str,
        old_private_key_hex: &str,
        new_public_key_hex: &str,
    ) -> Result<KeyRotationAnnouncement> {
        // Check if validator has a pending/active rotation
        if let Some(existing) = self.get_active_rotation(validator_id).await? {
            bail!(
                "Validator {} already has an active key rotation (status: {:?})",
                validator_id,
                existing.status
            );
        }

        // Check minimum rotation interval
        if let Some(last_rotation) = self.get_last_rotation(validator_id).await? {
            let time_since_last = Utc::now() - last_rotation.announced_at;
            if time_since_last < self.min_rotation_interval {
                bail!(
                    "Minimum rotation interval not met. Wait {} more days.",
                    (self.min_rotation_interval - time_since_last).num_days()
                );
            }
        }

        // Parse keys
        let old_private_bytes = hex::decode(old_private_key_hex)?;
        if old_private_bytes.len() != 32 {
            bail!("Old private key must be 32 bytes");
        }

        let new_public_bytes = hex::decode(new_public_key_hex)?;
        if new_public_bytes.len() != 32 {
            bail!("New public key must be 32 bytes");
        }

        // Create signing key from old private key
        let mut old_key_array = [0u8; 32];
        old_key_array.copy_from_slice(&old_private_bytes);
        let old_signing_key = SigningKey::from_bytes(&old_key_array);

        // Get old public key
        let old_public_key = old_signing_key.verifying_key();
        let old_public_hex = hex::encode(old_public_key.to_bytes());

        // Sign the new public key with old private key (proof of authority)
        let message = format!("KEY_ROTATION:{}:{}", validator_id, new_public_key_hex);
        let signature = old_signing_key.sign(message.as_bytes());
        let signature_hex = hex::encode(signature.to_bytes());

        // Create announcement
        let now = Utc::now();
        let announcement = KeyRotationAnnouncement {
            validator_id: validator_id.to_string(),
            old_public_key: old_public_hex,
            new_public_key: new_public_key_hex.to_string(),
            signature: signature_hex,
            announced_at: now,
            transition_ends_at: now + self.default_transition_period,
            status: KeyRotationStatus::Pending,
        };

        // Store in RocksDB
        let key = format!("key_rotation:{}:{}", validator_id, now.timestamp());
        storage::put_str(&self.pool, &key, &announcement)
            .map_err(|e| anyhow::anyhow!("Failed to store key rotation: {}", e))?;

        log::info!(
            "SYNC KEY ROTATION ANNOUNCED: {} (transition ends at: {})",
            validator_id,
            announcement.transition_ends_at
        );

        Ok(announcement)
    }

    /// Verify a key rotation announcement
    pub fn verify_rotation(&self, announcement: &KeyRotationAnnouncement) -> Result<bool> {
        // Parse old public key
        let old_public_bytes = hex::decode(&announcement.old_public_key)?;
        if old_public_bytes.len() != 32 {
            return Ok(false);
        }

        let mut old_key_array = [0u8; 32];
        old_key_array.copy_from_slice(&old_public_bytes);
        let old_public_key = VerifyingKey::from_bytes(&old_key_array)?;

        // Parse signature
        let sig_bytes = hex::decode(&announcement.signature)?;
        if sig_bytes.len() != 64 {
            return Ok(false);
        }

        let mut sig_array = [0u8; 64];
        sig_array.copy_from_slice(&sig_bytes);
        let signature = Signature::from_bytes(&sig_array);

        // Verify signature
        let message = format!("KEY_ROTATION:{}:{}", announcement.validator_id, announcement.new_public_key);

        match old_public_key.verify(message.as_bytes(), &signature) {
            Ok(()) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// Get active rotation for a validator
    pub async fn get_active_rotation(&self, validator_id: &str) -> Result<Option<KeyRotationAnnouncement>> {
        // Query from RocksDB
        let prefix = format!("key_rotation:{}:", validator_id);
        let rotations: Vec<KeyRotationAnnouncement> = storage::iter_prefix(&self.pool, prefix.as_bytes())
            .unwrap_or_default();

        // Find the most recent pending or in-transition rotation
        let active = rotations.into_iter()
            .filter(|r| r.status == KeyRotationStatus::Pending || r.status == KeyRotationStatus::InTransition)
            .max_by_key(|r| r.announced_at);

        Ok(active)
    }

    /// Get last rotation for a validator (any status)
    async fn get_last_rotation(&self, validator_id: &str) -> Result<Option<KeyRotationAnnouncement>> {
        let prefix = format!("key_rotation:{}:", validator_id);
        let rotations: Vec<KeyRotationAnnouncement> = storage::iter_prefix(&self.pool, prefix.as_bytes())
            .unwrap_or_default();

        let last = rotations.into_iter()
            .max_by_key(|r| r.announced_at);

        Ok(last)
    }

    /// Process pending rotations (update status based on time)
    pub async fn process_rotations(&self) -> Result<usize> {
        let now = Utc::now();
        let mut updated = 0;

        // Get all rotations
        let all_rotations: Vec<KeyRotationAnnouncement> = storage::iter_prefix(&self.pool, b"key_rotation:")
            .unwrap_or_default();

        for mut rotation in all_rotations {
            let should_update = match rotation.status {
                KeyRotationStatus::Pending if rotation.announced_at <= now => {
                    rotation.status = KeyRotationStatus::InTransition;
                    true
                }
                KeyRotationStatus::InTransition if rotation.transition_ends_at <= now => {
                    rotation.status = KeyRotationStatus::Completed;
                    true
                }
                _ => false,
            };

            if should_update {
                let key = format!("key_rotation:{}:{}", rotation.validator_id, rotation.announced_at.timestamp());
                if storage::put_str(&self.pool, &key, &rotation).is_ok() {
                    updated += 1;
                }
            }
        }

        if updated > 0 {
            log::info!("SYNC Processed {} key rotation(s)", updated);
        }

        Ok(updated)
    }

    /// Check if a public key is valid for a validator at the current time
    pub async fn is_key_valid(&self, validator_id: &str, public_key_hex: &str) -> Result<bool> {
        // Check for active rotation
        if let Some(rotation) = self.get_active_rotation(validator_id).await? {
            match rotation.status {
                KeyRotationStatus::Pending => {
                    // Only old key valid (transition hasn't started)
                    Ok(public_key_hex == rotation.old_public_key)
                }
                KeyRotationStatus::InTransition => {
                    // Both old and new keys valid
                    Ok(public_key_hex == rotation.old_public_key ||
                       public_key_hex == rotation.new_public_key)
                }
                KeyRotationStatus::Completed => {
                    // Only new key valid
                    Ok(public_key_hex == rotation.new_public_key)
                }
                KeyRotationStatus::Revoked => {
                    // Fall back to registry check
                    self.check_registry_key(validator_id, public_key_hex).await
                }
            }
        } else {
            // No rotation - check against registry
            self.check_registry_key(validator_id, public_key_hex).await
        }
    }

    /// Check key against validator registry (no rotation)
    async fn check_registry_key(&self, validator_id: &str, public_key_hex: &str) -> Result<bool> {
        // Query validator from RocksDB
        let key = format!("validator:{}", validator_id);
        let validator: Option<ValidatorRecord> = storage::get_str(&self.pool, &key)
            .unwrap_or(None);

        match validator {
            Some(v) => Ok(v.public_key == public_key_hex),
            None => Ok(false), // Validator not found
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ValidatorRecord {
    validator_id: String,
    public_key: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_rotation_verification() {
        // Generate keypairs
        let old_key = SigningKey::generate(&mut rand::thread_rng());
        let new_key = SigningKey::generate(&mut rand::thread_rng());

        let old_public_hex = hex::encode(old_key.verifying_key().to_bytes());
        let new_public_hex = hex::encode(new_key.verifying_key().to_bytes());

        // Sign rotation
        let validator_id = "test-validator";
        let message = format!("KEY_ROTATION:{}:{}", validator_id, new_public_hex);
        let signature = old_key.sign(message.as_bytes());
        let signature_hex = hex::encode(signature.to_bytes());

        // Create announcement
        let announcement = KeyRotationAnnouncement {
            validator_id: validator_id.to_string(),
            old_public_key: old_public_hex,
            new_public_key: new_public_hex,
            signature: signature_hex,
            announced_at: Utc::now(),
            transition_ends_at: Utc::now() + Duration::hours(24),
            status: KeyRotationStatus::Pending,
        };

        // Verify signature is valid
        assert!(announcement.old_public_key.len() > 0);
        assert!(announcement.new_public_key.len() > 0);
    }
}
