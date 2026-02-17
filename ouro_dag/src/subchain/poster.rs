use crate::PgPool;
// src/subchain/poster.rs
use crate::anchor_service::AnchorService;
use anyhow::Result;
use std::sync::Arc;
use uuid::Uuid;

pub struct AnchorPoster {
    pub svc: Arc<AnchorService>,
    pub pg: Arc<PgPool>,
}

impl AnchorPoster {
    pub fn new(svc: Arc<AnchorService>, pg: Arc<PgPool>) -> Self {
        Self { svc, pg }
    }

    /// Post a batch to the main anchor service.
    /// Accepts subchain as string; when parsable to UUID we forward it, otherwise we use Uuid::nil().
    pub async fn post(&self, subchain: &str, batch_root: &[u8]) -> Result<String> {
        let block_height = chrono::Utc::now().timestamp();
        let subchain_uuid = match Uuid::parse_str(subchain) {
            Ok(u) => u,
            Err(_) => Uuid::nil(),
        };

        let txid = self
            .svc
            .post_anchor(subchain_uuid, block_height, batch_root)
            .await?;
        Ok(txid)
    }
}
