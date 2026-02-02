// src/vm/storage.rs
//! Contract storage layer
//!
//! Stores contract code and state in RocksDB

use super::types::{ContractAddress, ContractMetadata, StorageKey};
use anyhow::{bail, Result};
use parking_lot::RwLock;
use rocksdb::{Options, WriteBatch, DB};
use std::sync::Arc;

/// Contract storage manager
pub struct ContractStorage {
    /// RocksDB instance
    db: Arc<DB>,

    /// In-memory cache for hot storage
    cache: Arc<RwLock<lru::LruCache<StorageKey, Vec<u8>>>>,
}

impl ContractStorage {
    /// Create new storage with existing DB
    pub fn new(db: Arc<DB>) -> Self {
        Self {
            db,
            cache: Arc::new(RwLock::new(lru::LruCache::new(
                std::num::NonZeroUsize::new(10_000).expect("10_000 is non-zero"),
            ))),
        }
    }

    /// Store contract code
    pub fn store_contract_code(&self, address: ContractAddress, code: &[u8]) -> Result<()> {
        let key = Self::contract_code_key(address);
        self.db.put(key, code)?;
        Ok(())
    }

    /// Load contract code
    pub fn load_contract_code(&self, address: ContractAddress) -> Result<Vec<u8>> {
        let key = Self::contract_code_key(address);
        self.db
            .get(key)?
            .ok_or_else(|| anyhow::anyhow!("Contract not found: {}", address))
    }

    /// Check if contract exists
    pub fn contract_exists(&self, address: ContractAddress) -> Result<bool> {
        let key = Self::contract_code_key(address);
        Ok(self.db.get(key)?.is_some())
    }

    /// Store contract metadata
    pub fn store_metadata(&self, metadata: &ContractMetadata) -> Result<()> {
        let key = Self::contract_metadata_key(metadata.address);
        let value = serde_json::to_vec(metadata)?;
        self.db.put(key, value)?;
        Ok(())
    }

    /// Load contract metadata
    pub fn load_metadata(&self, address: ContractAddress) -> Result<Option<ContractMetadata>> {
        let key = Self::contract_metadata_key(address);
        match self.db.get(key)? {
            Some(bytes) => {
                let metadata = serde_json::from_slice(&bytes)?;
                Ok(Some(metadata))
            }
            None => Ok(None),
        }
    }

    /// Set storage value
    pub fn set_storage(&self, key: StorageKey, value: Vec<u8>) -> Result<()> {
        let db_key = Self::storage_key_bytes(&key);

        // Update cache
        self.cache.write().put(key.clone(), value.clone());

        // Persist to disk
        self.db.put(db_key, value)?;
        Ok(())
    }

    /// Get storage value
    pub fn get_storage(&self, key: &StorageKey) -> Result<Option<Vec<u8>>> {
        // Check cache first
        if let Some(value) = self.cache.read().peek(key) {
            return Ok(Some(value.clone()));
        }

        // Load from disk
        let db_key = Self::storage_key_bytes(key);
        if let Some(value) = self.db.get(db_key)? {
            // Update cache
            self.cache.write().put(key.clone(), value.clone());
            Ok(Some(value))
        } else {
            Ok(None)
        }
    }

    /// Clear storage value (set to empty)
    pub fn clear_storage(&self, key: &StorageKey) -> Result<()> {
        let db_key = Self::storage_key_bytes(key);

        // Remove from cache
        self.cache.write().pop(key);

        // Delete from disk
        self.db.delete(db_key)?;
        Ok(())
    }

    /// Batch write multiple storage values (atomic)
    pub fn batch_set_storage(&self, updates: Vec<(StorageKey, Vec<u8>)>) -> Result<()> {
        let mut batch = WriteBatch::default();

        for (key, value) in updates.iter() {
            let db_key = Self::storage_key_bytes(key);
            batch.put(db_key, value);

            // Update cache
            self.cache.write().put(key.clone(), value.clone());
        }

        self.db.write(batch)?;
        Ok(())
    }

    /// Get all storage keys for a contract (expensive, use sparingly)
    pub fn get_contract_storage_keys(&self, contract: ContractAddress) -> Result<Vec<StorageKey>> {
        let prefix = Self::storage_prefix(contract);
        let mut keys = Vec::new();

        let iter = self.db.prefix_iterator(&prefix);
        for item in iter {
            let (key_bytes, _) = item?;
            if key_bytes.len() >= 64 {
                // Parse storage key
                let mut contract_bytes = [0u8; 32];
                let mut key_bytes_arr = [0u8; 32];
                contract_bytes.copy_from_slice(&key_bytes[9..41]); // Skip prefix
                key_bytes_arr.copy_from_slice(&key_bytes[41..73]);

                if contract_bytes == contract.0 {
                    keys.push(StorageKey::new(contract, key_bytes_arr));
                }
            }
        }

        Ok(keys)
    }

    /// Clear all storage for a contract (dangerous!)
    pub fn clear_contract_storage(&self, contract: ContractAddress) -> Result<()> {
        let keys = self.get_contract_storage_keys(contract)?;

        let mut batch = WriteBatch::default();
        for key in keys {
            let db_key = Self::storage_key_bytes(&key);
            batch.delete(db_key);

            // Remove from cache
            self.cache.write().pop(&key);
        }

        self.db.write(batch)?;
        Ok(())
    }

    // Internal key generation methods

    fn contract_code_key(address: ContractAddress) -> Vec<u8> {
        let mut key = b"contract_code:".to_vec();
        key.extend_from_slice(&address.0);
        key
    }

    fn contract_metadata_key(address: ContractAddress) -> Vec<u8> {
        let mut key = b"contract_meta:".to_vec();
        key.extend_from_slice(&address.0);
        key
    }

    fn storage_key_bytes(key: &StorageKey) -> Vec<u8> {
        let mut bytes = b"contract_storage:".to_vec();
        bytes.extend_from_slice(&key.to_bytes());
        bytes
    }

    fn storage_prefix(contract: ContractAddress) -> Vec<u8> {
        let mut prefix = b"contract_storage:".to_vec();
        prefix.extend_from_slice(&contract.0);
        prefix
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn create_test_storage() -> ContractStorage {
        let dir = tempdir().unwrap();
        let mut opts = Options::default();
        opts.create_if_missing(true);
        let db = Arc::new(DB::open(&opts, dir.path()).unwrap());
        ContractStorage::new(db)
    }

    #[test]
    fn test_store_and_load_code() {
        let storage = create_test_storage();
        let address = ContractAddress([1u8; 32]);
        let code = b"test contract code";

        storage.store_contract_code(address, code).unwrap();
        let loaded = storage.load_contract_code(address).unwrap();

        assert_eq!(&loaded, code);
    }

    #[test]
    fn test_contract_exists() {
        let storage = create_test_storage();
        let address = ContractAddress([1u8; 32]);

        assert!(!storage.contract_exists(address).unwrap());

        storage.store_contract_code(address, b"code").unwrap();

        assert!(storage.contract_exists(address).unwrap());
    }

    #[test]
    fn test_storage_operations() {
        let storage = create_test_storage();
        let contract = ContractAddress([1u8; 32]);
        let key = StorageKey::new(contract, [2u8; 32]);
        let value = b"test value".to_vec();

        // Set
        storage.set_storage(key.clone(), value.clone()).unwrap();

        // Get
        let loaded = storage.get_storage(&key).unwrap().unwrap();
        assert_eq!(loaded, value);

        // Clear
        storage.clear_storage(&key).unwrap();
        assert!(storage.get_storage(&key).unwrap().is_none());
    }

    #[test]
    fn test_batch_storage() {
        let storage = create_test_storage();
        let contract = ContractAddress([1u8; 32]);

        let updates = vec![
            (StorageKey::new(contract, [1u8; 32]), b"value1".to_vec()),
            (StorageKey::new(contract, [2u8; 32]), b"value2".to_vec()),
            (StorageKey::new(contract, [3u8; 32]), b"value3".to_vec()),
        ];

        storage.batch_set_storage(updates.clone()).unwrap();

        for (key, expected_value) in updates {
            let value = storage.get_storage(&key).unwrap().unwrap();
            assert_eq!(value, expected_value);
        }
    }

    #[test]
    fn test_cache() {
        let storage = create_test_storage();
        let contract = ContractAddress([1u8; 32]);
        let key = StorageKey::new(contract, [2u8; 32]);
        let value = b"cached value".to_vec();

        // Set value
        storage.set_storage(key.clone(), value.clone()).unwrap();

        // Get should hit cache
        let loaded = storage.get_storage(&key).unwrap().unwrap();
        assert_eq!(loaded, value);

        // Verify cache contains the key
        assert!(storage.cache.read().contains(&key));
    }
}
