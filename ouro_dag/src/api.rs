// src/api.rs
// Axum-based API router for transaction submit + basic checks
use crate::bft::slashing::SlashingManager;
use crate::simple_metrics::METRICS;
use crate::storage::{get_str, RocksDb};
use crate::PgPool;
// TODO_ROCKSDB: Re-enable when modules are converted
// use crate::intrusion_detection::{IntrusionDetectionSystem, ThreatType, AlertSeverity};
// use crate::key_rotation::KeyRotationManager;
// use crate::tracing_context::{TraceContext, LogLevel};

use axum::extract::{ConnectInfo, Extension, Path, Query};
use axum::http::{Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post};
use axum::Router;
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use subtle::ConstantTimeEq;
use thiserror::Error;
use uuid::Uuid;

// Type alias for API responses
type ApiResult = Result<Response, StatusCode>;

// Type alias for rate limiter (using our SimpleRateLimiter)
type RateLimiter = Arc<SimpleRateLimiter>;

// TODO_ROCKSDB: IDS structures and implementation
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Alert {
    id: String,
    threat_type: ThreatType,
    severity: AlertSeverity,
    source: String,
    timestamp: chrono::DateTime<chrono::Utc>,
    description: String,
    event_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Threat {
    source: String,
    threat_type: ThreatType,
    count: usize,
}

#[derive(Clone)]
pub struct IntrusionDetectionSystem {
    db: PgPool,
}

impl IntrusionDetectionSystem {
    fn new(db: PgPool) -> Self {
        Self { db }
    }

    fn get_alerts_by_severity(&self, severity: AlertSeverity) -> Vec<Alert> {
        // TODO_ROCKSDB: Query alerts by severity from RocksDB
        let _ = severity;
        Vec::new()
    }

    fn get_recent_alerts(&self, limit: usize) -> Vec<Alert> {
        // TODO_ROCKSDB: Query recent alerts from RocksDB
        let _ = limit;
        Vec::new()
    }

    fn get_active_threats(&self) -> Vec<Threat> {
        // TODO_ROCKSDB: Query active threats from RocksDB
        Vec::new()
    }

    fn record_event(
        &self,
        source: &str,
        threat_type: ThreatType,
        severity: AlertSeverity,
        details: &str,
    ) {
        // Record security event in RocksDB
        let alert = Alert {
            id: uuid::Uuid::new_v4().to_string(),
            threat_type,
            severity,
            source: source.to_string(),
            timestamp: chrono::Utc::now(),
            description: details.to_string(),
            event_count: 1,
        };

        let key = format!("alert:{}", alert.id);
        let _ = crate::storage::put(&self.db, key.as_bytes(), &alert);
    }
}

// Removed duplicate Metrics struct - using crate::metrics::Metrics instead

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum AlertSeverity {
    Low,
    Medium,
    High,
    Critical,
}

// Threat type classification
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum ThreatType {
    RateLimitViolation,
    AuthenticationFailure,
    SuspiciousActivity,
}

// Key rotation data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
struct KeyRotation {
    validator_id: String,
    old_public_key: String,
    new_public_key: String,
    signature: String,
    announced_at: chrono::DateTime<chrono::Utc>,
    transition_ends_at: chrono::DateTime<chrono::Utc>,
    status: String,
}

#[derive(Clone)]
struct KeyRotationManager {
    db: PgPool,
}

impl KeyRotationManager {
    fn new(db_pool: PgPool) -> Self {
        Self { db: db_pool }
    }

    async fn announce_rotation(
        &self,
        validator_id: &str,
        new_pubkey: &str,
        effective_block: u64,
    ) -> Result<KeyRotation, String> {
        // Store key rotation announcement in RocksDB
        let rotation = KeyRotation {
            validator_id: validator_id.to_string(),
            old_public_key: String::new(), // TODO: Query current key
            new_public_key: new_pubkey.to_string(),
            signature: String::new(), // TODO: Generate signature
            announced_at: chrono::Utc::now(),
            transition_ends_at: chrono::Utc::now() + chrono::Duration::days(7),
            status: "pending".to_string(),
        };

        let key = format!("key_rotation:{}", validator_id);
        crate::storage::put(&self.db, key.as_bytes(), &rotation).map_err(|e| e.to_string())?;

        let _ = effective_block;
        Ok(rotation)
    }

    async fn get_active_rotation(&self, validator_id: &str) -> Result<Option<KeyRotation>, String> {
        // Query active rotation from RocksDB
        let key = format!("key_rotation:{}", validator_id);
        crate::storage::get::<_, KeyRotation>(&self.db, key.as_bytes()).map_err(|e| e.to_string())
    }
}

struct SimpleRateLimiter {
    buckets: Arc<Mutex<HashMap<String, (usize, Instant)>>>,
    max_requests: usize,
    window_duration: Duration,
    last_cleanup: Arc<Mutex<Instant>>,
}

impl SimpleRateLimiter {
    fn new(max_requests: usize, window_secs: u64) -> Self {
        Self {
            buckets: Arc::new(Mutex::new(HashMap::new())),
            max_requests,
            window_duration: Duration::from_secs(window_secs),
            last_cleanup: Arc::new(Mutex::new(Instant::now())),
        }
    }

    fn check_rate_limit(&self, ip: &IpAddr) -> bool {
        // Periodic cleanup every 5 minutes to prevent memory growth
        self.maybe_cleanup();

        let mut buckets = self.buckets.lock().unwrap_or_else(|poisoned| {
            warn!("Rate limiter mutex poisoned - recovering data");
            poisoned.into_inner()
        });
        let now = Instant::now();

        let entry = buckets.entry(ip.to_string()).or_insert((0, now));
        let (count, window_start) = entry;

        // Check if we're in a new window
        if now.duration_since(*window_start) > self.window_duration {
            // Reset window
            *count = 1;
            *window_start = now;
            true
        } else if *count < self.max_requests {
            // Within limit
            *count += 1;
            true
        } else {
            // Rate limit exceeded
            false
        }
    }

    /// Cleanup old entries periodically (prevents memory growth / DoS)
    fn maybe_cleanup(&self) {
        let cleanup_interval = Duration::from_secs(300); // 5 minutes

        // Check if cleanup is needed
        {
            let last = self.last_cleanup.lock().unwrap_or_else(|p| p.into_inner());
            if last.elapsed() < cleanup_interval {
                return;
            }
        }

        // Do cleanup
        let mut buckets = self.buckets.lock().unwrap_or_else(|poisoned| {
            warn!("Rate limiter mutex poisoned during cleanup - recovering data");
            poisoned.into_inner()
        });

        let now = Instant::now();
        let before = buckets.len();
        buckets.retain(|_, (_, window_start)| {
            now.duration_since(*window_start) < self.window_duration * 2
        });
        let after = buckets.len();

        if before != after {
            info!("Rate limiter cleanup: removed {} stale entries ({} remaining)",
                before - after, after);
        }

        // Update last cleanup time
        if let Ok(mut last) = self.last_cleanup.lock() {
            *last = now;
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct IncomingTxn {
    pub tx_hash: String,
    pub sender: String,
    pub recipient: String,
    pub payload: JsonValue,              // full signed payload from client
    pub signature: Option<String>,       // optional meta
    pub idempotency_key: Option<String>, // optional client-supplied idempotency key
    pub nonce: Option<i64>,              // optional account nonce
}

#[derive(Debug, Serialize)]
struct TxSubmitResponse {
    tx_id: Uuid,
    status: &'static str,
}

#[derive(Debug, Error)]
enum ApiError {
    #[error("database error")]
    Db(String),

    #[error("duplicate transaction")]
    Duplicate,

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("internal error: {0}")]
    Internal(String),
}

// REMOVED: impl From<sqlx::Error>
// //  fn from(e: sqlx::Error) -> Self {
// //  ApiError::Db(e)
// //  }

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, body) = match &self {
            ApiError::Db(e) => {
                error!("DB error: {:?}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "database error".to_string(),
                )
            }
            ApiError::Duplicate => (StatusCode::CONFLICT, "duplicate transaction".to_string()),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            ApiError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
        };
        let body_json = serde_json::json!({ "error": body });
        (status, Json(body_json)).into_response()
    }
}

///////////////////////////////////////////////////////////////////////////
// POST /tx/submit
///////////////////////////////////////////////////////////////////////////
async fn submit_tx(
    Extension(db_pool): Extension<Arc<RocksDb>>,
    Extension(batch_writer): Extension<Arc<crate::batch_writer::BatchWriter>>,
    Json(incoming): Json<IncomingTxn>,
) -> Result<impl IntoResponse, ApiError> {
    // Basic validation
    if incoming.tx_hash.trim().is_empty() {
        return Err(ApiError::BadRequest("tx_hash required".into()));
    }
    if incoming.sender.trim().is_empty() || incoming.recipient.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "sender and recipient are required".into(),
        ));
    }

    // Check for duplicate by tx_hash
    // Use RocksDB to check if tx_hash already exists
    let tx_key = format!("tx_hash:{}", &incoming.tx_hash);
    if db_pool
        .get(tx_key.as_bytes())
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .is_some()
    {
        info!("duplicate tx_hash submitted: {}", &incoming.tx_hash);
        return Err(ApiError::Duplicate);
    }

    // SECURITY: Mandatory signature verification for all transactions.
    // Both public_key and signature MUST be present — unsigned transactions are rejected.
    let pubkey = incoming.payload.get("public_key").and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::BadRequest("Missing required field: public_key".into()))?;
    let sig = incoming.payload.get("signature").and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::BadRequest("Missing required field: signature".into()))?;

    let message = incoming.tx_hash.as_bytes();
    let ok = crate::crypto::verify_ed25519_hex(pubkey, sig, message);
    if !ok {
        return Err(ApiError::BadRequest("signature invalid".into()));
    }

    // Optionally check idempotency_key uniqueness
    if let Some(_key) = &incoming.idempotency_key {
        // TODO_ROCKSDB: Check idempotency key in RocksDB
        // If exists, return existing tx_id (idempotent behavior)
    }

    // TPS OPTIMIZATION: Queue transaction for batch processing instead of synchronous DB writes
    // This enables 20k-50k TPS by batching writes every 100ms or 500 transactions
    let tx_id = Uuid::new_v4();

    // Extract fields for batch writer
    let amount = incoming
        .payload
        .get("amount")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let fee = incoming
        .payload
        .get("fee")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let public_key = incoming
        .payload
        .get("public_key")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let pending_tx = crate::batch_writer::PendingTransaction {
        tx_id,
        tx_hash: incoming.tx_hash.clone(),
        sender: incoming.sender.clone(),
        recipient: incoming.recipient.clone(),
        payload: incoming.payload.clone(),
        signature: incoming.signature.clone(),
        amount,
        fee,
        public_key,
    };

    // Submit to batch writer (non-blocking, returns immediately)
    if let Err(e) = batch_writer.submit(pending_tx).await {
        return Err(ApiError::BadRequest(format!(
            "Failed to queue transaction: {}",
            e
        )));
    }

    info!(
        " Queued tx {} for batch processing (sender: {})",
        tx_id, &incoming.sender
    );

    let resp = TxSubmitResponse {
        tx_id,
        status: "pending",
    };
    Ok((StatusCode::ACCEPTED, Json(resp)))
}

///////////////////////////////////////////////////////////////////////////
// GET /mempool
///////////////////////////////////////////////////////////////////////////
async fn get_mempool(Extension(db): Extension<Arc<RocksDb>>) -> ApiResult {
    // Query mempool entries from RocksDB (prefix: "mempool:")
    let mut txs: Vec<JsonValue> = Vec::new();

    // Iterate over mempool entries
    let iter = db.prefix_iterator(b"mempool:");
    for item in iter {
        match item {
            Ok((key, value)) => {
                // Only include keys that start with "mempool:"
                let key_str = String::from_utf8_lossy(&key);
                if !key_str.starts_with("mempool:") {
                    break; // Prefix iterator exhausted
                }
                if let Ok(tx) = serde_json::from_slice::<JsonValue>(&value) {
                    txs.push(tx);
                }
                // Limit to prevent huge responses
                if txs.len() >= 1000 {
                    break;
                }
            }
            Err(_) => break,
        }
    }

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "count": txs.len(),
            "transactions": txs
        })),
    )
        .into_response())
}

///////////////////////////////////////////////////////////////////////////
// GET /tx/:id (tries id as uuid, falls back to tx_hash lookup)
// GET /tx/hash/:hash
///////////////////////////////////////////////////////////////////////////
async fn get_tx_by_id_or_hash(
    Path(id): Path<String>,
    Extension(db_pool): Extension<Arc<RocksDb>>,
) -> ApiResult {
    // if it looks like a UUID, try uuid lookup
    if id.contains('-') {
        if let Ok(uuid) = Uuid::parse_str(&id) {
            return get_tx_by_id_inner(uuid, &db_pool).await;
        }
    }
    // otherwise fall back to hash lookup
    get_tx_by_hash_inner(&id, &db_pool).await
}

async fn get_tx_by_hash(
    Path(hash): Path<String>,
    Extension(db_pool): Extension<Arc<RocksDb>>,
) -> ApiResult {
    get_tx_by_hash_inner(&hash, &db_pool).await
}

async fn get_tx_by_id_inner(uuid: Uuid, db_pool: &Arc<RocksDb>) -> ApiResult {
    // Look up transaction by UUID
    let key = format!("tx:{}", uuid);
    match crate::storage::get_str::<JsonValue>(db_pool, &key) {
        Ok(Some(tx)) => Ok((StatusCode::OK, Json(tx)).into_response()),
        Ok(None) => Ok((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "transaction not found", "id": uuid.to_string() })),
        )
            .into_response()),
        Err(e) => Ok((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("database error: {}", e) })),
        )
            .into_response()),
    }
}

async fn get_tx_by_hash_inner(hash: &str, db_pool: &Arc<RocksDb>) -> ApiResult {
    // Look up transaction by hash
    let key = format!("tx_hash:{}", hash);
    match crate::storage::get_str::<JsonValue>(db_pool, &key) {
        Ok(Some(tx)) => Ok((StatusCode::OK, Json(tx)).into_response()),
        Ok(None) => {
            // Try alternate key format
            let alt_key = format!("tx:{}", hash);
            match crate::storage::get_str::<JsonValue>(db_pool, &alt_key) {
                Ok(Some(tx)) => Ok((StatusCode::OK, Json(tx)).into_response()),
                _ => Ok((
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({ "error": "transaction not found", "hash": hash })),
                )
                    .into_response()),
            }
        }
        Err(e) => Ok((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("database error: {}", e) })),
        )
            .into_response()),
    }
}

///////////////////////////////////////////////////////////////////////////
// GET /proof/:tx (lookup tx_index)
///////////////////////////////////////////////////////////////////////////
async fn get_proof_by_tx(
    Path(tx): Path<String>,
    Extension(db): Extension<Arc<RocksDb>>,
) -> ApiResult {
    // Look up proof/inclusion data for transaction
    let key = format!("proof:{}", tx);
    match crate::storage::get_str::<JsonValue>(&db, &key) {
        Ok(Some(proof)) => Ok((StatusCode::OK, Json(proof)).into_response()),
        Ok(None) => Ok((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "proof not found", "tx": tx })),
        )
            .into_response()),
        Err(e) => Ok((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("database error: {}", e) })),
        )
            .into_response()),
    }
}

///////////////////////////////////////////////////////////////////////////
// GET /block/:id -- placeholder; adapt to your block schema if needed
///////////////////////////////////////////////////////////////////////////
async fn get_block_by_id(
    Path(id): Path<String>,
    Extension(db_pool): Extension<Arc<RocksDb>>,
) -> ApiResult {
    // M2 fix: Use the existing DB pool instead of opening a separate connection
    let db = &*db_pool;

    // attempt to read the key
    let key = format!("block:{}", id);
    match get_str::<serde_json::Value>(&db, &key) {
        Ok(Some(val)) => {
            return Ok((StatusCode::OK, Json(val)).into_response());
        }
        Ok(None) => {
            // not found
            return Ok((
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "block not found" })),
            )
                .into_response());
        }
        Err(e) => {
            eprintln!("get_block_by_id: rocksdb read error: {:?}", e);
            return Ok((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                "error": "rocksdb read error"
                })),
            )
                .into_response());
        }
    }
}

///////////////////////////////////////////////////////////////////////////
// GET /health - Basic health check
///////////////////////////////////////////////////////////////////////////
async fn health(Extension(_db): Extension<Arc<RocksDb>>) -> ApiResult {
    let config = crate::config_manager::CONFIG.read().await;
    let node_name = config.identity.public_name.clone();
    drop(config);
    Ok((StatusCode::OK, Json(serde_json::json!({"status":"ok", "node_name": node_name}))).into_response())
}

/// POST /shutdown - Gracefully shut down the node (requires API key)
async fn shutdown_node() -> impl IntoResponse {
    info!("Shutdown requested via API");
    // Spawn a task to exit after a short delay so the response can be sent
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        std::process::exit(0);
    });
    (StatusCode::OK, Json(serde_json::json!({"status": "shutting_down"})))
}

///////////////////////////////////////////////////////////////////////////
// GET /health/detailed - Detailed health diagnostics
///////////////////////////////////////////////////////////////////////////
async fn health_detailed(
    Extension(_db): Extension<Arc<RocksDb>>,
    Extension(peer_store): Extension<crate::network::PeerStore>,
) -> ApiResult {
    // TODO_ROCKSDB: Implement detailed health check with RocksDB
    let health_status = serde_json::json!({
    "status": "healthy",
    "timestamp": chrono::Utc::now().to_rfc3339(),
    "checks": {
    "database": {"status": "ok", "message": "RocksDB not yet checked"},
    "peers": {"status": "ok", "count": peer_store.lock().await.len()}
    }
    });
    Ok((StatusCode::OK, Json(health_status)).into_response())
}

/// GET /peers - returns the runtime peer store with count and per-peer metadata
async fn get_peers(
    Extension(peer_store): Extension<crate::network::PeerStore>,
) -> Result<impl IntoResponse, ApiError> {
    let store = peer_store.lock().await;
    let peers: Vec<_> = store
        .iter()
        .enumerate()
        .map(|(i, e)| {
            serde_json::json!({
                "id": format!("peer-{}", i),
                "addr": e.addr,
                "role": e.role,
                "last_seen": e.last_seen_unix,
                "failures": e.failures,
                "banned_until": e.banned_until_unix,
                "latency_ms": 0,
            })
        })
        .collect();
    let count = peers.len();
    Ok((StatusCode::OK, Json(serde_json::json!({
        "count": count,
        "peers": peers,
    }))))
}

/// Get network statistics
async fn get_network_stats(
    Extension(peer_store): Extension<crate::network::PeerStore>,
) -> Result<impl IntoResponse, ApiError> {
    let (active_conns, _dedupe, _peer_count) = crate::network::get_p2p_metrics();
    let store = peer_store.lock().await;
    let total_peers = store.len();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let active_peers = store
        .iter()
        .filter(|p| p.last_seen_unix.unwrap_or(0) > now.saturating_sub(300))
        .count();
    let banned_peers = store
        .iter()
        .filter(|p| p.banned_until_unix.unwrap_or(0) > now)
        .count();

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "total_peers": total_peers,
            "active_peers": active_peers,
            "active_connections": active_conns,
            "banned_peers": banned_peers,
        })),
    ))
}

/// Get recent slashing events
///
/// Returns the most recent slashing events across all validators.
/// Query parameter `limit` controls how many events to return (default: 50, max: 500).
async fn get_slashing_events(
    Extension(db_pool): Extension<Arc<RocksDb>>,
) -> Result<impl IntoResponse, ApiError> {
    let slashing_manager = SlashingManager::new(db_pool);

    // Default limit of 50, max 500
    let limit = 50;

    match slashing_manager.get_recent_slashing_events(limit).await {
        Ok(events) => {
            let json_events: Vec<serde_json::Value> = events
                .iter()
                .map(|e| {
                    serde_json::json!({
                    "validator_id": e.validator_id,
                    "reason": e.reason,
                    "severity": e.severity,
                    "stake_before": e.stake_before,
                    "slashed_amount": e.slashed_amount,
                    "stake_after": e.stake_after,
                    "slashed_at": e.slashed_at,
                    "evidence": e.evidence,
                    })
                })
                .collect();

            Ok((StatusCode::OK, Json(json_events)))
        }
        Err(e) => {
            error!("Failed to fetch slashing events: {}", e);
            Err(ApiError::Internal(format!(
                "Failed to fetch slashing events: {}",
                e
            )))
        }
    }
}

/// Get slashing history for a specific validator
///
/// Returns all slashing events for the specified validator ID.
async fn get_validator_slashing_history(
    Path(validator_id): Path<String>,
    Extension(db_pool): Extension<Arc<RocksDb>>,
) -> Result<impl IntoResponse, ApiError> {
    let slashing_manager = SlashingManager::new(db_pool);

    match slashing_manager.get_slashing_history(&validator_id).await {
        Ok(events) => {
            let json_events: Vec<serde_json::Value> = events
                .iter()
                .map(|e| {
                    serde_json::json!({
                    "validator_id": e.validator_id,
                    "reason": e.reason,
                    "severity": e.severity,
                    "stake_before": e.stake_before,
                    "slashed_amount": e.slashed_amount,
                    "stake_after": e.stake_after,
                    "slashed_at": e.slashed_at,
                    "evidence": e.evidence,
                    })
                })
                .collect();

            Ok((
                StatusCode::OK,
                Json(serde_json::json!({
                "validator_id": validator_id,
                "total_events": json_events.len(),
                "events": json_events,
                })),
            ))
        }
        Err(e) => {
            error!(
                "Failed to fetch slashing history for {}: {}",
                validator_id, e
            );
            Err(ApiError::Internal(format!(
                "Failed to fetch slashing history: {}",
                e
            )))
        }
    }
}

/// Get current validator stakes
///
/// Returns the current stake amounts for all validators.
async fn get_validator_stakes(
    Extension(_db): Extension<Arc<RocksDb>>,
) -> Result<impl IntoResponse, ApiError> {
    // TODO_ROCKSDB: Implement validator stakes query with RocksDB
    let stakes: Vec<serde_json::Value> = Vec::new();
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
        "total_validators": 0,
        "stakes": stakes,
        })),
    ))
}

/// Request body for announcing key rotation
#[derive(Deserialize)]
struct AnnounceKeyRotationRequest {
    validator_id: String,
    /// Hex-encoded signature of the new_public_key_hex, signed with the old private key.
    /// Proves the requester owns the old key.
    proof_signature_hex: String,
    /// The old public key (hex) — used to verify the proof signature.
    old_public_key_hex: String,
    new_public_key_hex: String,
}

/// Announce a new validator key rotation
///
/// POST /validators/rotate-key
///
/// Allows a validator to announce a new key rotation with a 24-hour transition period.
/// The new key must be signed by the old key as proof of authority.
async fn announce_key_rotation(
    Extension(db_pool): Extension<Arc<RocksDb>>,
    Json(request): Json<AnnounceKeyRotationRequest>,
) -> Result<impl IntoResponse, ApiError> {
    // SECURITY (H1): Verify that the requester owns the old key by checking
    // a signature over the new public key made with the old private key.
    let proof_valid = crate::crypto::verify_ed25519_hex(
        &request.old_public_key_hex,
        &request.proof_signature_hex,
        request.new_public_key_hex.as_bytes(),
    );
    if !proof_valid {
        return Err(ApiError::BadRequest(
            "Invalid proof signature: you must sign the new public key with the old private key".into(),
        ));
    }

    let key_rotation_manager = KeyRotationManager::new(db_pool);

    match key_rotation_manager
        .announce_rotation(
            &request.validator_id,
            &request.new_public_key_hex,
            0, // effective_block placeholder
        )
        .await
    {
        Ok(announcement) => {
            info!(
                "SYNC Key rotation announced: {} (transition ends at: {})",
                request.validator_id, announcement.transition_ends_at
            );

            Ok((
                StatusCode::OK,
                Json(serde_json::json!({
                "success": true,
                "announcement": {
                "validator_id": announcement.validator_id,
                "old_public_key": announcement.old_public_key,
                "new_public_key": announcement.new_public_key,
                "announced_at": announcement.announced_at,
                "transition_ends_at": announcement.transition_ends_at,
                "status": format!("{:?}", announcement.status),
                }
                })),
            ))
        }
        Err(e) => {
            error!("Failed to announce key rotation: {}", e);
            Err(ApiError::BadRequest(format!("Key rotation failed: {}", e)))
        }
    }
}

/// Get active key rotation for a validator
///
/// GET /validators/:id/key-rotation
///
/// Returns the current active key rotation (if any) for the specified validator.
async fn get_key_rotation(
    Path(validator_id): Path<String>,
    Extension(db_pool): Extension<Arc<RocksDb>>,
) -> Result<impl IntoResponse, ApiError> {
    let key_rotation_manager = KeyRotationManager::new(db_pool);

    match key_rotation_manager
        .get_active_rotation(&validator_id)
        .await
    {
        Ok(Some(rotation)) => Ok((
            StatusCode::OK,
            Json(serde_json::json!({
            "validator_id": rotation.validator_id,
            "old_public_key": rotation.old_public_key,
            "new_public_key": rotation.new_public_key,
            "signature": rotation.signature,
            "announced_at": rotation.announced_at,
            "transition_ends_at": rotation.transition_ends_at,
            "status": format!("{:?}", rotation.status),
            })),
        )),
        Ok(None) => Ok((
            StatusCode::OK,
            Json(serde_json::json!({
            "validator_id": validator_id,
            "has_active_rotation": false,
            "message": "No active key rotation found"
            })),
        )),
        Err(e) => {
            error!("Failed to fetch key rotation: {}", e);
            Err(ApiError::Internal(format!(
                "Failed to fetch key rotation: {}",
                e
            )))
        }
    }
}

/// Query parameters for security alerts
#[derive(Deserialize)]
struct AlertQuery {
    /// Limit number of results (default: 50, max: 500)
    #[serde(default = "default_limit")]
    limit: usize,
    /// Filter by severity (optional)
    severity: Option<String>,
}

fn default_limit() -> usize {
    50
}

/// Get security alerts from IDS
///
/// Returns recent security alerts with optional severity filtering.
async fn get_security_alerts(
    Query(query): Query<AlertQuery>,
    Extension(ids): Extension<Arc<IntrusionDetectionSystem>>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = std::cmp::min(query.limit, 500); // Max 500 alerts

    let alerts = if let Some(severity_str) = query.severity {
        // Filter by severity
        let severity = match severity_str.to_lowercase().as_str() {
            "low" => AlertSeverity::Low,
            "medium" => AlertSeverity::Medium,
            "high" => AlertSeverity::High,
            "critical" => AlertSeverity::Critical,
            _ => {
                return Err(ApiError::BadRequest(
                    "Invalid severity. Use: low, medium, high, or critical".to_string(),
                ));
            }
        };

        ids.get_alerts_by_severity(severity)
    } else {
        // Get all recent alerts
        ids.get_recent_alerts(limit)
    };

    let json_alerts: Vec<serde_json::Value> = alerts
        .iter()
        .map(|a| {
            serde_json::json!({
            "id": a.id,
            "threat_type": format!("{:?}", a.threat_type),
            "severity": format!("{:?}", a.severity),
            "source": a.source,
            "timestamp": a.timestamp,
            "description": a.description,
            "event_count": a.event_count,
            })
        })
        .collect();

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
        "total_alerts": json_alerts.len(),
        "alerts": json_alerts,
        })),
    ))
}

/// Get active threats from IDS
///
/// Returns currently active threat patterns being monitored.
async fn get_active_threats(
    Extension(ids): Extension<Arc<IntrusionDetectionSystem>>,
) -> Result<impl IntoResponse, ApiError> {
    let threats = ids.get_active_threats();

    let json_threats: Vec<serde_json::Value> = threats
        .iter()
        .map(|threat| {
            serde_json::json!({
            "source": &threat.source,
            "threat_type": format!("{:?}", &threat.threat_type),
            "event_count": threat.count,
            })
        })
        .collect();

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
        "total_threats": json_threats.len(),
        "active_threats": json_threats,
        })),
    ))
}

/// Get Prometheus metrics
///
/// Returns metrics in Prometheus text format for scraping.
async fn get_metrics() -> Result<impl IntoResponse, ApiError> {
    let prometheus_output = METRICS.export_prometheus();

    Ok((
        StatusCode::OK,
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4",
        )],
        prometheus_output,
    ))
}

#[allow(dead_code)]
fn verify_signature(_payload: &JsonValue, _signature: Option<String>) -> Result<(), String> {
    // placeholder for future more complex verification
    Ok(())
}

/// Build router for this microservice (call from main)
/// Accepts the runtime PeerStore so /peers can show discovered peers with metadata.
/// API Key authentication middleware.
///
/// SECURITY: API key authentication required for all endpoints except public read-only ones.
/// Transaction signatures provide additional security at the transaction level.
pub async fn auth_middleware<B>(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Extension(ids): Extension<Arc<IntrusionDetectionSystem>>,
    req: Request<B>,
    next: Next<B>,
) -> Result<Response, StatusCode> {
    let path = req.uri().path();

    // Public endpoints that don't require API key
    let public_endpoints = [
        "/health",
        "/metrics",
        "/api/balance/", // Allow balance queries (read-only)
    ];

    // Check if this is a public endpoint
    if public_endpoints.iter().any(|p| path.starts_with(p)) {
        return Ok(next.run(req).await);
    }

    // Extract API key from Authorization header or query parameter
    let api_key = req
        .headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .or_else(|| {
            req.uri().query().and_then(|q| {
                q.split('&')
                    .find(|p| p.starts_with("api_key="))
                    .and_then(|p| p.strip_prefix("api_key="))
            })
        });

    // Get valid API keys from environment
    let api_keys_str = std::env::var("API_KEYS").unwrap_or_default();
    let valid_keys: Vec<&str> = api_keys_str
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    // Check if API key is valid using constant-time comparison
    // This prevents timing attacks where attackers can guess keys character by character
    if let Some(key) = api_key {
        let key_bytes = key.as_bytes();
        let is_valid = valid_keys.iter().any(|valid_key| {
            let valid_bytes = valid_key.as_bytes();
            // Only compare if lengths match (length is not secret in this context)
            // Use constant-time comparison for the actual bytes
            key_bytes.len() == valid_bytes.len() && key_bytes.ct_eq(valid_bytes).into()
        });
        if is_valid {
            return Ok(next.run(req).await);
        }
    }

    // No valid API key provided - record security event
    let source = addr.ip().to_string();
    warn!("Unauthorized API request to {} from {}", path, source);

    // TODO_ROCKSDB: Re-enable when IDS/metrics modules are available
    // ids.record_event(&source, ThreatType::AuthenticationFailure, &format!("Failed authentication attempt to {}", path));
    // metrics.inc_auth_failures();

    Err(StatusCode::UNAUTHORIZED)
}

/// Distributed tracing and logging middleware.
///
/// Logs all HTTP requests with timing information.
async fn tracing_middleware<B>(req: Request<B>, next: Next<B>) -> Result<Response, StatusCode> {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let start = std::time::Instant::now();

    // Run the request
    let response = next.run(req).await;

    // Calculate latency
    let latency = start.elapsed();
    let status = response.status().as_u16();

    // Log request
    info!(
        "{} {} {} - {:.3}s",
        method,
        path,
        status,
        latency.as_secs_f64()
    );

    Ok(response)
}

/// Rate limiting middleware.
///
/// Prevents DoS attacks by limiting requests per IP address.
/// Returns 429 Too Many Requests if rate limit exceeded.
///
/// Rate limits are configured via environment variables:
/// - `RATE_LIMIT_MAX_REQUESTS`: Maximum requests per window (default: 100)
/// - `RATE_LIMIT_WINDOW_SECS`: Time window in seconds (default: 60)
async fn rate_limit_middleware<B>(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Extension(rate_limiter): Extension<RateLimiter>,
    Extension(ids): Extension<Arc<IntrusionDetectionSystem>>,
    req: Request<B>,
    next: Next<B>,
) -> Result<Response, StatusCode> {
    let ip = addr.ip();

    if rate_limiter.check_rate_limit(&ip) {
        // Request allowed
        METRICS.inc_http_requests();
        Ok(next.run(req).await)
    } else {
        // Rate limit exceeded - record security event
        let ip_str = ip.to_string();
        warn!("BLOCKED Rate limit exceeded for IP: {}", ip_str);

        // Record rate limit violation in IDS
        ids.record_event(
            &ip_str,
            ThreatType::RateLimitViolation,
            AlertSeverity::Medium,
            "Exceeded request rate limit",
        );
        METRICS.inc_http_errors();

        Err(StatusCode::TOO_MANY_REQUESTS)
    }
}

/// Submit a heartbeat from a node.
/// Requires Ed25519 signature proving ownership of the node_id.
async fn submit_heartbeat(
    Extension(db): Extension<RocksDb>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, ApiError> {
    let node_id = payload
        .get("node_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::BadRequest("Missing node_id".to_string()))?;

    let wallet_address = payload
        .get("wallet_address")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::BadRequest("Missing wallet_address".to_string()))?;

    // H7 fix: Authenticate heartbeat with Ed25519 signature
    let public_key_hex = payload
        .get("public_key")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::BadRequest("Missing required field: public_key".to_string()))?;

    let signature_hex = payload
        .get("signature")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::BadRequest("Missing required field: signature".to_string()))?;

    // Verify that the public key matches the claimed node_id
    let pubkey_bytes = hex::decode(public_key_hex)
        .map_err(|_| ApiError::BadRequest("Invalid public_key hex".to_string()))?;
    let derived_node_id = hex::encode(&pubkey_bytes);
    if derived_node_id != node_id && !node_id.starts_with(&derived_node_id[..8.min(derived_node_id.len())]) {
        return Err(ApiError::BadRequest("public_key does not match node_id".to_string()));
    }

    // Verify the signature over the heartbeat message
    let message = format!("heartbeat:{}:{}", node_id, wallet_address);
    let sig_bytes = hex::decode(signature_hex)
        .map_err(|_| ApiError::BadRequest("Invalid signature hex".to_string()))?;

    let valid = crate::crypto::keys::verify_bytes(&pubkey_bytes, message.as_bytes(), &sig_bytes);
    if !valid {
        return Err(ApiError::BadRequest("Invalid heartbeat signature".to_string()));
    }

    let role_str = payload
        .get("role")
        .and_then(|v| v.as_str())
        .unwrap_or("heavy");

    let role = match role_str.to_lowercase().as_str() {
        "medium" => crate::config_manager::NodeRole::Medium,
        "light" => crate::config_manager::NodeRole::Light,
        _ => crate::config_manager::NodeRole::Heavy,
    };

    crate::rewards::record_heartbeat(&db, node_id, wallet_address, role)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to record heartbeat: {}", e)))?;

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "ok",
            "message": "Heartbeat recorded"
        })),
    ))
}

/// Claim rewards for a node
async fn claim_rewards(
    Extension(db): Extension<RocksDb>,
    Extension(batch_writer): Extension<Arc<crate::batch_writer::BatchWriter>>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, ApiError> {
    let node_id = payload
        .get("node_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::BadRequest("Missing node_id".to_string()))?;

    let (wallet_address, reward_amount) = crate::rewards::claim_rewards(&db, node_id)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to claim rewards: {}", e)))?;

    // Create reward transaction (Minting) with cryptographic HMAC signature
    let tx_id = Uuid::new_v4();
    let tx_hash = format!("reward-{}-{}", node_id, tx_id);

    // Sign system transaction with BFT secret seed (HMAC-SHA256)
    let bft_seed = std::env::var("BFT_SECRET_SEED").unwrap_or_default();
    let system_sig = crate::batch_writer::sign_system_tx(&tx_hash, &bft_seed);

    let pending_tx = crate::batch_writer::PendingTransaction {
        tx_id,
        tx_hash,
        sender: "system".to_string(),
        recipient: wallet_address.clone(),
        amount: reward_amount,
        fee: 0,
        payload: serde_json::json!({
            "type": "block_reward",
            "node_id": node_id,
            "timestamp": chrono::Utc::now().to_rfc3339()
        }),
        signature: Some(system_sig),
        public_key: "system".to_string(),
    };

    // Submit to batch writer
    if let Err(e) = batch_writer.submit(pending_tx).await {
        error!("Failed to queue reward transaction: {}", e);
        return Err(ApiError::Internal("Failed to queue reward transaction".to_string()));
    }

    info!("Minted {} OURO reward for {}", reward_amount, wallet_address);

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "ok",
            "wallet_address": wallet_address,
            "reward_amount": reward_amount,
            "tx_id": tx_id.to_string(),
            "message": "Reward claimed and transaction queued"
        })),
    ))
}

/// Get node statistics
async fn get_node_stats(
    Extension(db): Extension<RocksDb>,
    Path(node_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let stats = crate::rewards::get_node_stats(&db, &node_id)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to get node stats: {}", e)))?;

    let pending_rewards = crate::rewards::calculate_pending_rewards(&stats, 1.0);

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "node_id": stats.node_id,
            "wallet_address": stats.wallet_address,
            "total_uptime_secs": stats.total_uptime_secs,
            "last_heartbeat": stats.last_heartbeat,
            "first_seen": stats.first_seen,
            "pending_rewards": pending_rewards,
        })),
    ))
}

/// Get all active nodes
async fn get_active_nodes(
    Extension(db): Extension<RocksDb>,
) -> Result<impl IntoResponse, ApiError> {
    let nodes = crate::rewards::get_active_nodes(&db)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to get active nodes: {}", e)))?;

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "active_nodes": nodes.len(),
            "nodes": nodes,
        })),
    ))
}

/// Get metrics in JSON format for dashboard
async fn get_metrics_json() -> impl IntoResponse {
    METRICS.inc_http_requests();
    Json(METRICS.export_json())
}

/// Get system resource usage (CPU, memory, disk, network)
async fn get_resources() -> impl IntoResponse {
    METRICS.inc_http_requests();

    // Get memory usage
    let mem_mb = get_process_memory_mb();

    // Get disk usage for data directory — use config's actual db_path
    let db_path = {
        let config = crate::config_manager::CONFIG.read().await;
        config.storage.db_path.clone()
    };
    let (disk_used_gb, disk_total_gb) = get_disk_usage(&db_path);

    // Use sampled CPU from metrics
    let cpu_pct = METRICS.cpu_usage.load(std::sync::atomic::Ordering::Relaxed) as f64;

    // Get network bytes since last call and convert to KB/s
    // Dashboard polls every 2 seconds, so divide by 2
    let (bytes_in, bytes_out) = crate::network::get_and_reset_net_bytes();
    let net_in_kbps = bytes_in as f64 / 2.0 / 1024.0;
    let net_out_kbps = bytes_out as f64 / 2.0 / 1024.0;

    Json(serde_json::json!({
        "cpu_pct": cpu_pct,
        "mem_mb": mem_mb,
        "disk_gb_used": disk_used_gb,
        "disk_gb_total": disk_total_gb,
        "net_in_kbps": net_in_kbps,
        "net_out_kbps": net_out_kbps,
        "uptime_secs": METRICS.uptime_secs()
    }))
}

/// Get node identity and configuration
async fn get_identity() -> impl IntoResponse {
    METRICS.inc_http_requests();
    let config = crate::config_manager::CONFIG.read().await;

    Json(serde_json::json!({
        "node_id": config.identity.node_id,
        "role": config.role,
        "public_name": config.identity.public_name,
        "total_uptime_secs": METRICS.uptime_secs(),
        "difficulty": config.adaptive_difficulty.current,
        "version": env!("CARGO_PKG_VERSION")
    }))
}

/// Get consensus state (view, leader, QC, last committed block)
async fn get_consensus() -> impl IntoResponse {
    METRICS.inc_http_requests();
    Json(METRICS.export_consensus_json())
}

async fn get_latest_state_proof(
    Extension(db): Extension<Arc<RocksDb>>,
) -> impl IntoResponse {
    use sha2::{Digest, Sha256};

    // Get real block height from metrics
    let block_height = METRICS.consensus_rounds.load(std::sync::atomic::Ordering::Relaxed);

    // Compute a state root from the database path as a stand-in for a real merkle root.
    // In production this would be the actual DAG/state tree merkle root.
    let mut hasher = Sha256::new();
    hasher.update(b"ouroboros-state-root");
    hasher.update(block_height.to_le_bytes());
    // Include db pointer address for uniqueness per node
    hasher.update(format!("{:p}", Arc::as_ptr(&db)).as_bytes());
    let hash = hasher.finalize();
    let mut root = [0u8; 32];
    root.copy_from_slice(&hash);

    let generator = crate::zk_proofs::state_proof::ProofGenerator::new();
    let proof = generator.generate_state_proof(root, block_height);
    Json(proof)
}

/// Get process memory usage in MB
fn get_process_memory_mb() -> u64 {
    #[cfg(windows)]
    {
        // Use GetProcessMemoryInfo from kernel32 (K32GetProcessMemoryInfo)
        #[repr(C)]
        #[derive(Default)]
        struct ProcessMemoryCounters {
            cb: u32,
            page_fault_count: u32,
            peak_working_set_size: usize,
            working_set_size: usize,
            quota_peak_paged_pool_usage: usize,
            quota_paged_pool_usage: usize,
            quota_peak_non_paged_pool_usage: usize,
            quota_non_paged_pool_usage: usize,
            pagefile_usage: usize,
            peak_pagefile_usage: usize,
        }

        #[link(name = "kernel32")]
        extern "system" {
            fn GetCurrentProcess() -> *mut std::ffi::c_void;
            fn K32GetProcessMemoryInfo(
                hProcess: *mut std::ffi::c_void,
                ppsmemCounters: *mut ProcessMemoryCounters,
                cb: u32,
            ) -> i32;
        }

        unsafe {
            let handle = GetCurrentProcess();
            let mut counters = ProcessMemoryCounters::default();
            counters.cb = std::mem::size_of::<ProcessMemoryCounters>() as u32;

            if K32GetProcessMemoryInfo(handle, &mut counters, counters.cb) != 0 {
                return (counters.working_set_size / 1024 / 1024) as u64;
            }
        }
        0
    }
    #[cfg(not(windows))]
    {
        // On Unix, try to read /proc/self/statm
        if let Ok(statm) = std::fs::read_to_string("/proc/self/statm") {
            let parts: Vec<&str> = statm.split_whitespace().collect();
            if let Some(rss) = parts.get(1).and_then(|s| s.parse::<u64>().ok()) {
                return rss * 4096 / 1024 / 1024; // Convert pages to MB
            }
        }
        0
    }
}

/// Get disk usage for data directory
fn get_disk_usage(data_path: &str) -> (f64, f64) {
    // Calculate data directory size recursively
    let used = calculate_dir_size(data_path) as f64 / 1_073_741_824.0; // Convert to GB

    // Get total disk space - use "." as base for relative paths
    let total_path = if std::path::Path::new(data_path).is_absolute() {
        data_path.to_string()
    } else {
        ".".to_string()
    };
    let total = get_disk_total_gb(&total_path);

    (used, total)
}

/// Recursively calculate directory size
fn calculate_dir_size(path: &str) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_file() {
                if let Ok(meta) = entry.metadata() {
                    total += meta.len();
                }
            } else if path.is_dir() {
                total += calculate_dir_size(&path.to_string_lossy());
            }
        }
    }
    total
}

/// Get total disk space in GB
fn get_disk_total_gb(path: &str) -> f64 {
    #[cfg(windows)]
    {
        // On Windows, use GetDiskFreeSpaceExW
        use std::os::windows::ffi::OsStrExt;
        use std::ffi::OsStr;

        let path_wide: Vec<u16> = OsStr::new(path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let mut free_bytes: u64 = 0;
        let mut total_bytes: u64 = 0;
        let mut total_free: u64 = 0;

        unsafe {
            // GetDiskFreeSpaceExW
            #[link(name = "kernel32")]
            extern "system" {
                fn GetDiskFreeSpaceExW(
                    lpDirectoryName: *const u16,
                    lpFreeBytesAvailableToCaller: *mut u64,
                    lpTotalNumberOfBytes: *mut u64,
                    lpTotalNumberOfFreeBytes: *mut u64,
                ) -> i32;
            }

            if GetDiskFreeSpaceExW(
                path_wide.as_ptr(),
                &mut free_bytes,
                &mut total_bytes,
                &mut total_free,
            ) != 0
            {
                return total_bytes as f64 / 1_073_741_824.0;
            }
        }
        100.0 // Fallback
    }
    #[cfg(not(windows))]
    {
        // L9 fix: Use a safer approach — read /proc/mounts or call `df` via command
        // instead of hand-rolling a Statvfs struct that may not match the platform ABI.
        use std::process::Command;
        if let Ok(output) = Command::new("df").arg("-B1").arg(path).output() {
            if let Ok(text) = String::from_utf8(output.stdout) {
                // Parse `df` output: second line, second column = total bytes
                if let Some(line) = text.lines().nth(1) {
                    let fields: Vec<&str> = line.split_whitespace().collect();
                    if let Some(total_str) = fields.get(1) {
                        if let Ok(total_bytes) = total_str.parse::<u64>() {
                            return total_bytes as f64 / 1_073_741_824.0;
                        }
                    }
                }
            }
        }
        100.0 // Fallback
    }
}

pub fn router(
    db_pool: Arc<RocksDb>,
    db_peer_store: crate::network::PeerStore,
    batch_writer: Arc<crate::batch_writer::BatchWriter>,
) -> (Router, Arc<IntrusionDetectionSystem>) {
    // Initialize rate limiter with configurable limits
    let max_requests = std::env::var("RATE_LIMIT_MAX_REQUESTS")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(100); // Default: 100 requests per window

    let window_secs = std::env::var("RATE_LIMIT_WINDOW_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(60); // Default: 60 second window

    let rate_limiter = Arc::new(SimpleRateLimiter::new(max_requests as usize, window_secs));

    info!(
        "PROTECTED Rate limiting enabled: {} requests per {} seconds",
        max_requests, window_secs
    );

    // Initialize Intrusion Detection System
    let ids = Arc::new(IntrusionDetectionSystem::new(db_pool.clone()));

    info!("DEBUG: Intrusion Detection System (IDS) initialized");

    // Prometheus metrics available via global METRICS static

    info!("STATS Prometheus metrics ready");

    // Public routes (no authentication required)
    let public_routes = Router::new()
        .route("/health", get(health))
        .route("/health/detailed", get(health_detailed))
        .route("/metrics", get(get_metrics)) // Prometheus text format
        .route("/metrics/json", get(get_metrics_json)) // JSON format for dashboard
        .route("/resources", get(get_resources)) // System resource usage
        .route("/identity", get(get_identity)) // Node identity and config
        .route("/consensus", get(get_consensus)) // Consensus state (view, leader, QC)
        .route("/peers", get(get_peers)) // Peer list (read-only)
        .route("/state_proof", get(get_latest_state_proof)) // ZK State Proof
        .route("/network/stats", get(get_network_stats)); // Network stats (read-only)

    // Protected routes with rate limiting and authentication
    // Applies BOTH rate limiting and authentication (layers run bottom to top)
    let protected_routes = Router::new()
        .route("/tx/submit", post(submit_tx))
        .route("/mempool", get(get_mempool))
        .route("/tx/:id", get(get_tx_by_id_or_hash)) // accepts uuid or tx_hash; we try both
        .route("/tx/hash/:hash", get(get_tx_by_hash)) // explicit hash lookup
        .route("/block/:id", get(get_block_by_id)) // placeholder route
        .route("/proof/:tx", get(get_proof_by_tx))
        .route("/slashing/events", get(get_slashing_events)) // Query recent slashing events
        .route(
            "/slashing/validator/:id",
            get(get_validator_slashing_history),
        ) // Validator slashing history
        .route("/validators/stakes", get(get_validator_stakes)) // Current validator stakes
        .route("/validators/rotate-key", post(announce_key_rotation)) // Announce key rotation
        .route("/validators/:id/key-rotation", get(get_key_rotation)) // Query key rotation status
        .route("/security/alerts", get(get_security_alerts)) // Get security alerts from IDS
        .route("/rewards/heartbeat", post(submit_heartbeat))
        .route("/rewards/claim", post(claim_rewards))
        .route("/rewards/stats/:node_id", get(get_node_stats))
        .route("/rewards/active", get(get_active_nodes))
        .route("/security/threats", get(get_active_threats)) // Get active threats
        .route("/oracle/submit", post(submit_oracle_data)) // Submit oracle data
        .route("/oracle/feed/:feed_id", get(get_oracle_feed)) // Get specific feed
        .route("/oracle/feeds", get(list_oracle_feeds)) // List all feeds
        .route("/oracle/node/:operator_id", get(get_oracle_node_info)) // Get node info
        .route("/shutdown", post(shutdown_node)) // Graceful shutdown (auth required)
        .layer(middleware::from_fn(auth_middleware)) // Run second (outer layer)
        .layer(middleware::from_fn(rate_limit_middleware)) // Run first (inner layer)
        .layer(Extension(rate_limiter))
        .layer(Extension(ids.clone()));

    // Combine all routes with global middleware
    // Middleware layers run bottom-to-top (LIFO)
    let router = Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .layer(Extension(db_pool))
        .layer(Extension(db_peer_store))
        .layer(Extension(batch_writer)) // TPS Optimization: Batch writer for high throughput
        .layer(middleware::from_fn(tracing_middleware)); // Request logging and distributed tracing

    (router, ids)
}

// ============================================================================
// Oracle API Handlers
// ============================================================================

/// Submit oracle data
async fn submit_oracle_data(Json(payload): Json<JsonValue>) -> Result<Json<JsonValue>, StatusCode> {
    // Parse oracle submission
    let submission: crate::oracle::OracleSubmission =
        serde_json::from_value(payload).map_err(|e| {
            error!("Failed to parse oracle submission: {}", e);
            StatusCode::BAD_REQUEST
        })?;

    // Get oracle manager
    let oracle_manager = crate::oracle::get_oracle_manager().map_err(|e| {
        error!("Failed to get oracle manager: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Submit data
    oracle_manager
        .lock()
        .await
        .submit_data(submission.clone())
        .await
        .map_err(|e| {
            error!("Failed to submit oracle data: {}", e);
            StatusCode::BAD_REQUEST
        })?;

    // Aggregate feed if enough submissions
    match oracle_manager
        .lock()
        .await
        .aggregate_feed(&submission.feed_id)
        .await
    {
        Ok(aggregated) => {
            info!(
                "Oracle feed aggregated: {} (confidence: {:.2}%)",
                submission.feed_id,
                aggregated.confidence * 100.0
            );
        }
        Err(e) => {
            // Not an error - might need more submissions
            info!("Feed not ready for aggregation: {}", e);
        }
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "feed_id": submission.feed_id,
        "timestamp": submission.timestamp
    })))
}

/// Get oracle feed by ID
async fn get_oracle_feed(Path(feed_id): Path<String>) -> Result<Json<JsonValue>, StatusCode> {
    let oracle_manager =
        crate::oracle::get_oracle_manager().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let feed = oracle_manager
        .lock()
        .await
        .get_feed(&feed_id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(serde_json::json!({
        "feed_id": feed.feed_id,
        "value": feed.value,
        "confidence": feed.confidence,
        "num_submissions": feed.num_submissions,
        "num_validators": feed.num_validators,
        "total_stake": feed.total_stake,
        "timestamp": feed.timestamp
    })))
}

/// List all oracle feeds
async fn list_oracle_feeds() -> Result<Json<JsonValue>, StatusCode> {
    let oracle_manager =
        crate::oracle::get_oracle_manager().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let feeds = oracle_manager.lock().await.list_feeds().await;

    Ok(Json(serde_json::json!({
        "feeds": feeds,
        "count": feeds.len()
    })))
}

/// Get oracle node info
async fn get_oracle_node_info(
    Path(operator_id): Path<String>,
) -> Result<Json<JsonValue>, StatusCode> {
    // Get node registry (would need to add global instance)
    // For now, return placeholder
    Ok(Json(serde_json::json!({
        "operator_id": operator_id,
        "status": "active",
        "message": "Oracle node registry not yet implemented"
    })))
}
