// src/token_bucket.rs
// Rate limiting implementation using Token Bucket algorithm
// Adapted for RocksDB persistence

use crate::PgPool;
use crate::storage::RocksDb;
use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Token bucket state stored in DB
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BucketState {
    pub tokens: f64,
    pub last_refill_unix: u64,
}

pub struct TokenBucketConfig {
    pub capacity: f64,
    pub fill_rate: f64, // tokens per second
}

impl Default for TokenBucketConfig {
    fn default() -> Self {
        Self {
            capacity: 100.0,
            fill_rate: 10.0,
        }
    }
}

pub struct TokenBucketManager {
    db: Arc<PgPool>,
    config: TokenBucketConfig,
}

impl TokenBucketManager {
    pub fn new(db: Arc<PgPool>, config: TokenBucketConfig) -> Self {
        Self { db, config }
    }

    /// Consume tokens for a specific key (e.g., user IP or ID)
    pub fn consume(&self, key: &str, tokens_needed: f64) -> Result<bool, String> {
        let db_key = format!("ratelimit:{}", key);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Load state or create new
        let mut state: BucketState = match crate::storage::get_str(&self.db, &db_key) {
            Ok(Some(s)) => s,
            Ok(None) => BucketState {
                tokens: self.config.capacity,
                last_refill_unix: now,
            },
            Err(e) => return Err(format!("DB error: {}", e)),
        };

        // Refill logic
        let elapsed = now.saturating_sub(state.last_refill_unix) as f64;
        let added = elapsed * self.config.fill_rate;
        state.tokens = (state.tokens + added).min(self.config.capacity);
        state.last_refill_unix = now;

        // Consume logic
        if state.tokens >= tokens_needed {
            state.tokens -= tokens_needed;
            // Persist
            crate::storage::put_str(&self.db, &db_key, &state).map_err(|e| e.to_string())?;
            Ok(true)
        } else {
            // Even if we fail, we persist the refill update
            crate::storage::put_str(&self.db, &db_key, &state).map_err(|e| e.to_string())?;
            Ok(false)
        }
    }
}

// API Module
pub mod api {
    use super::*;

    pub fn router(db: Arc<PgPool>) -> Router {
        let manager = Arc::new(TokenBucketManager::new(db, TokenBucketConfig::default()));
        Router::new()
            .route("/check/:key", post(check_rate_limit))
            .layer(Extension(manager))
    }

    async fn check_rate_limit(
        Path(key): Path<String>,
        Extension(manager): Extension<Arc<TokenBucketManager>>,
    ) -> impl IntoResponse {
        match manager.consume(&key, 1.0) {
            Ok(true) => (StatusCode::OK, Json(serde_json::json!({"allowed": true}))),
            Ok(false) => (
                StatusCode::TOO_MANY_REQUESTS,
                Json(serde_json::json!({"allowed": false, "reason": "rate limit exceeded"})),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e})),
            ),
        }
    }
}
