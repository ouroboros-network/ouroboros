// src/microchain/store.rs
use anyhow::Result;
use chrono::{DateTime, Utc};
use rocksdb::{ColumnFamilyDescriptor, Options, DB};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MicroHeader {
    pub id: Uuid,
    pub height: u64,
    pub timestamp: DateTime<Utc>,
    pub leaf_hash: Vec<u8>, // hash of header (leaf for micro merkle)
}

pub struct MicroStore {
    db: DB,
}

impl MicroStore {
    pub fn open(name: &str) -> Result<Self> {
        let base_path = std::env::var("ROCKSDB_PATH").unwrap_or_else(|_| "rocksdb_data".into());
        // Use sibling path, not nested subdirectory
        let p = format!("{}_microchain_{}", base_path, name);

        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);
        opts.set_max_open_files(128);

        let cf_headers = ColumnFamilyDescriptor::new("headers", Options::default());
        let cf_by_height = ColumnFamilyDescriptor::new("by_height", Options::default());

        let db = DB::open_cf_descriptors(&opts, p, vec![cf_headers, cf_by_height])?;
        Ok(Self { db })
    }

    pub fn put_header(&self, hdr: &MicroHeader) -> Result<()> {
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

    pub fn tip(&self) -> Result<Option<MicroHeader>> {
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

    pub fn get_header(&self, id: &Uuid) -> Result<Option<MicroHeader>> {
        let cf_headers = self.db.cf_handle("headers")
            .ok_or_else(|| anyhow::anyhow!("Column family 'headers' not found"))?;
        match self.db.get_cf(cf_headers, id.as_bytes())? {
            Some(v) => {
                let h: MicroHeader = serde_json::from_slice(&v)?;
                Ok(Some(h))
            }
            None => Ok(None),
        }
    }
}
