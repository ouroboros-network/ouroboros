use crate::PgPool;
// src/controller/mod.rs
use anyhow::Result;
use uuid::Uuid;

/// Minimal controller that assigns microchains to subchains and creates subchains when needed.
pub struct Controller {
    pub pg: PgPool,
    pub micro_per_subchain: i64,
}

impl Controller {
    pub fn new(pg: PgPool, micro_per_subchain: i64) -> Self {
        Self {
            pg,
            micro_per_subchain,
        }
    }

    /// Pick an existing subchain with capacity or create a new one.
    pub async fn pick_or_create_subchain(&self) -> Result<Uuid> {
        // Look up a subchain with capacity
        // TODO_ROCKSDB: Implement subchain assignment with RocksDB
        Ok(Uuid::new_v4())
    }

    /// Assign microchain to an available subchain, creating one if needed.
    pub async fn assign_microchain(&self, microchain_id: Uuid) -> Result<Uuid> {
        let sub_id = self.pick_or_create_subchain().await?;
        // TODO_ROCKSDB: // TODO_ROCKSDB:  sqlx::query(r#"UPDATE microchains SET parent_subchain = $1 WHERE id = $2"#)
        // TODO_ROCKSDB: // TODO_ROCKSDB:  sqlx::query(r#"UPDATE subchains SET current_micro = current_micro + 1 WHERE id = $1"#)
        Ok(sub_id)
    }
}
