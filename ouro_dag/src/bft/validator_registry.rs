// src/bft/validator_registry.rs
use crate::storage::RocksDb;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// NodeId type alias expected in bft modules
pub type NodeId = String;

/// Validator info with stake
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ValidatorInfo {
    pub pubkey: Vec<u8>,
    pub stake: u64,
}

/// Thread-safe registry for validators with stake tracking and persistence
#[derive(Clone)]
pub struct ValidatorRegistry {
    inner: Arc<RwLock<HashMap<NodeId, ValidatorInfo>>>,
    db: Option<Arc<RocksDb>>,
}

impl Default for ValidatorRegistry {
    fn default() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            db: None,
        }
    }
}

impl ValidatorRegistry {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            db: None,
        }
    }

    /// Create registry with database persistence
    pub fn with_db(db: Arc<RocksDb>) -> Self {
        let mut registry = Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            db: Some(db),
        };
        // Load existing validators from database
        if let Err(e) = registry.load_from_db() {
            log::warn!("Failed to load validators from DB: {}", e);
        }
        registry
    }

    /// Load validators from database
    fn load_from_db(&mut self) -> Result<(), String> {
        let db = self.db.as_ref().ok_or("No database configured")?;

        // Load validator registry from RocksDB
        let key = "validator_registry";
        if let Ok(Some(validators)) =
            crate::storage::get::<_, HashMap<NodeId, ValidatorInfo>>(db, key.as_bytes())
        {
            *self.inner.write() = validators;
            log::info!(
                "Loaded {} validators from database",
                self.inner.read().len()
            );
        }
        Ok(())
    }

    /// Persist validators to database
    fn persist_to_db(&self) -> Result<(), String> {
        if let Some(db) = &self.db {
            let key = "validator_registry";
            let validators = self.inner.read().clone();
            crate::storage::put(db, key.as_bytes(), &validators)
                .map_err(|e| format!("Failed to persist validators: {}", e))?;
        }
        Ok(())
    }

    /// Register or update a validator with pubkey and stake.
    pub fn register(&self, id: &str, pubkey: Vec<u8>) {
        self.inner.write().insert(
            id.to_string(),
            ValidatorInfo {
                pubkey,
                stake: 0, // Default stake, should be set via register_with_stake
            },
        );
        if let Err(e) = self.persist_to_db() {
            log::error!("Failed to persist validator registration: {}", e);
        }
    }

    /// Register validator with stake amount
    pub fn register_with_stake(&self, id: &str, pubkey: Vec<u8>, stake: u64) {
        self.inner
            .write()
            .insert(id.to_string(), ValidatorInfo { pubkey, stake });
        // Also persist individual stake for slashing manager
        if let Some(db) = &self.db {
            let stake_key = format!("validator_stake:{}", id);
            let _ = crate::storage::put_str(db, &stake_key, &stake.to_string());
        }
        if let Err(e) = self.persist_to_db() {
            log::error!("Failed to persist validator registration: {}", e);
        }
    }

    /// Update stake for existing validator
    pub fn update_stake(&self, id: &str, stake: u64) {
        if let Some(info) = self.inner.write().get_mut(id) {
            info.stake = stake;
            // Also update individual stake for slashing manager
            if let Some(db) = &self.db {
                let stake_key = format!("validator_stake:{}", id);
                let _ = crate::storage::put_str(db, &stake_key, &stake.to_string());
            }
            if let Err(e) = self.persist_to_db() {
                log::error!("Failed to persist stake update: {}", e);
            }
        }
    }

    /// Remove a registered validator
    pub fn remove(&self, id: &str) {
        self.inner.write().remove(id);
        // Also remove individual stake
        if let Some(db) = &self.db {
            let stake_key = format!("validator_stake:{}", id);
            let zero = "0".to_string();
            let _ = crate::storage::put_str(db, &stake_key, &zero);
        }
        if let Err(e) = self.persist_to_db() {
            log::error!("Failed to persist validator removal: {}", e);
        }
    }

    /// Get validator info if present
    pub fn get(&self, id: &str) -> Option<Vec<u8>> {
        self.inner.read().get(id).map(|info| info.pubkey.clone())
    }

    /// Try to parse the validator's public key as a HybridPublicKey
    /// Returns None if the validator doesn't exist or has a legacy (Ed25519-only) key
    pub fn get_hybrid_pubkey(&self, id: &str) -> Option<crate::crypto::hybrid::HybridPublicKey> {
        let pubkey_bytes = self.get(id)?;
        bincode::deserialize(&pubkey_bytes).ok()
    }

    /// Get validator info with stake
    pub fn get_validator_info(&self, id: &str) -> Option<ValidatorInfo> {
        self.inner.read().get(id).cloned()
    }

    /// Get all validators sorted by stake (descending)
    pub fn get_validators_by_stake(&self) -> Vec<(NodeId, u64)> {
        let mut validators: Vec<_> = self
            .inner
            .read()
            .iter()
            .map(|(id, info)| (id.clone(), info.stake))
            .collect();
        validators.sort_by(|a, b| b.1.cmp(&a.1)); // Descending by stake
        validators
    }

    /// Get total stake of all validators
    pub fn get_total_stake(&self) -> u64 {
        self.inner.read().values().map(|info| info.stake).sum()
    }

    /// Get all validator IDs
    pub fn get_all_ids(&self) -> Vec<NodeId> {
        self.inner.read().keys().cloned().collect()
    }

    /// Check if validator exists
    pub fn exists(&self, id: &str) -> bool {
        self.inner.read().contains_key(id)
    }

    /// Get validator count
    pub fn count(&self) -> usize {
        self.inner.read().len()
    }

    /// Force reload from database
    pub fn reload(&mut self) -> Result<(), String> {
        self.load_from_db()
    }
}
