// src/subchain/store.rs
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rocksdb::{ColumnFamilyDescriptor, Options, DB};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SubBlockHeader {
    pub id: Uuid,
    pub height: u64,
    pub timestamp: DateTime<Utc>,
    pub batch_anchors: Vec<Vec<u8>>, // batch roots included in this header
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BatchRecord {
    pub batch_root: Vec<u8>,
    pub aggregator: String,
    pub leaf_count: usize,
    pub created_at: DateTime<Utc>,
    pub serialized_leaves_ref: Option<String>,
    pub verified: bool,
}

pub struct SubStore {
    db: Arc<DB>,
}

impl SubStore {
    /// Get a reference to the underlying database
    pub fn db(&self) -> &Arc<DB> {
        &self.db
    }

    pub fn open(name: &str) -> Result<Self> {
        let base_path = std::env::var("ROCKSDB_PATH").unwrap_or_else(|_| "rocksdb_data".into());
        // Use sibling path, not nested subdirectory
        let p = format!("{}_subchain_{}", base_path, name);

        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);
        opts.set_max_open_files(128);

        let cf_headers = ColumnFamilyDescriptor::new("headers", Options::default());
        let cf_batches = ColumnFamilyDescriptor::new("batches", Options::default());
        let cf_by_height = ColumnFamilyDescriptor::new("by_height", Options::default());

        let db = DB::open_cf_descriptors(&opts, p, vec![cf_headers, cf_batches, cf_by_height])
            .context("open RocksDB")?;
        Ok(Self { db: Arc::new(db) })
    }

    /// Construct a SubStore from an existing RocksDB (for ephemeral runs)
    pub fn from_db(db: Arc<DB>) -> Result<Self> {
        Ok(SubStore { db })
    }

    pub fn put_header(&self, hdr: &SubBlockHeader) -> Result<()> {
        let cf_headers = self.db.cf_handle("headers")
            .ok_or_else(|| anyhow::anyhow!("Column family 'headers' not found"))?;
        let cf_by_height = self.db.cf_handle("by_height")
            .ok_or_else(|| anyhow::anyhow!("Column family 'by_height' not found"))?;

        let key = hdr.id.as_bytes();
        let v = serde_json::to_vec(hdr)?;
        self.db.put_cf(cf_headers, key, v)?;
        self.db
            .put_cf(cf_by_height, hdr.height.to_be_bytes(), hdr.id.as_bytes())?;
        Ok(())
    }

    pub fn tip(&self) -> Result<Option<SubBlockHeader>> {
        let cf_by_height = self.db.cf_handle("by_height")
            .ok_or_else(|| anyhow::anyhow!("Column family 'by_height' not found"))?;
        let iter = self
            .db
            .iterator_cf(cf_by_height, rocksdb::IteratorMode::End);

        if let Some(Ok((_, v))) = iter.take(1).next() {
            let id = uuid::Uuid::from_slice(&v)?;
            return self.get_header(&id);
        }
        Ok(None)
    }

    pub fn get_header(&self, id: &Uuid) -> Result<Option<SubBlockHeader>> {
        let cf_headers = self.db.cf_handle("headers")
            .ok_or_else(|| anyhow::anyhow!("Column family 'headers' not found"))?;
        match self.db.get_cf(cf_headers, id.as_bytes())? {
            Some(v) => {
                let h: SubBlockHeader = serde_json::from_slice(&v)?;
                Ok(Some(h))
            }
            None => Ok(None),
        }
    }

    pub fn put_batch(&self, root: &[u8], rec: &BatchRecord) -> Result<()> {
        let cf_batches = self.db.cf_handle("batches")
            .ok_or_else(|| anyhow::anyhow!("Column family 'batches' not found"))?;
        let key = root;
        let v = serde_json::to_vec(rec)?;
        self.db.put_cf(cf_batches, key, v)?;
        Ok(())
    }

    pub fn get_batch(&self, root: &[u8]) -> Result<Option<BatchRecord>> {
        let cf_batches = self.db.cf_handle("batches")
            .ok_or_else(|| anyhow::anyhow!("Column family 'batches' not found"))?;
        match self.db.get_cf(cf_batches, root)? {
            Some(v) => {
                let b: BatchRecord = serde_json::from_slice(&v)?;
                Ok(Some(b))
            }
            None => Ok(None),
        }
    }
}
