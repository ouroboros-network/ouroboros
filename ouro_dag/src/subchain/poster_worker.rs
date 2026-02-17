use crate::PgPool;
// src/subchain/poster_worker.rs
use crate::anchor_service::AnchorService;
use crate::subchain::poster::AnchorPoster;
use anyhow::Result;
use log::{error, info, warn};
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use uuid::Uuid;

// Configuration constants
const MAX_RETRIES: i32 = 10;
const BASE_BACKOFF_SECS: u64 = 5; // Base delay: 5 seconds
const MAX_BACKOFF_SECS: u64 = 3600; // Cap at 1 hour
const ALERT_THRESHOLD: i32 = 5; // Alert after 5 failed attempts

pub async fn run_poster(pg: PgPool, anchor_svc: AnchorService) -> Result<()> {
    // AnchorPoster wants Arc<AnchorService>, Arc<PgPool>
    let poster = AnchorPoster::new(Arc::new(anchor_svc), Arc::new(pg.clone()));

    loop {
        // Query with exponential backoff logic:
        // - Skip batches that have exceeded MAX_RETRIES
        // - Skip batches where last_attempt is too recent (exponential backoff)
        // - Use FOR UPDATE SKIP LOCKED for safe concurrent workers
        // TODO_ROCKSDB: Implement batch posting with RocksDB
        log::info!("Poster worker: RocksDB not yet implemented");
        sleep(Duration::from_secs(30)).await;
    }
}
