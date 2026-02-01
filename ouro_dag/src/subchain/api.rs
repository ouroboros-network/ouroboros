use crate::PgPool;
// src/subchain/api.rs
use axum::{Router, routing::{post}, Json, extract::State, response::IntoResponse, http::StatusCode, middleware};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;
use crate::subchain::manager::SubchainManager;

#[derive(Clone)]
pub struct ApiState {
 pub manager: Arc<SubchainManager>,
 pub pg: Arc<PgPool>,
}

#[derive(Deserialize)]
pub struct PostBatchAnchor {
 pub batch_root: Vec<u8>,
 pub aggregator: String,
 pub leaf_count: usize,
 pub serialized_leaves_ref: Option<String>,
}

#[derive(Serialize)]
pub struct ApiOk { pub status: &'static str }

pub async fn post_batch_anchor(State(_state): State<ApiState>, Json(_req): Json<PostBatchAnchor>) -> impl IntoResponse {
 (StatusCode::OK, Json(ApiOk { status: "ok" })).into_response()
}

#[derive(Deserialize)]
pub struct AppendHeaderPayload {
 pub height: u64,
 pub batch_roots: Vec<String>, // hex strings
}

pub async fn append_header(State(state): State<ApiState>, Json(payload): Json<AppendHeaderPayload>) -> impl IntoResponse {
 // Validate and decode batch roots - don't silently swallow errors
 let batch_anchors: Result<Vec<Vec<u8>>, String> = payload.batch_roots
 .into_iter()
 .enumerate()
 .map(|(i, h)| {
 hex::decode(&h).map_err(|e| format!("invalid hex in batch_root[{}]: {}", i, e))
 })
 .collect();

 let batch_anchors = match batch_anchors {
 Ok(anchors) => anchors,
 Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
 };

 let hdr = crate::subchain::store::SubBlockHeader {
 id: Uuid::new_v4(),
 height: payload.height,
 timestamp: chrono::Utc::now(),
 batch_anchors,
 };

 if let Err(e) = state.manager.append_header(hdr) {
 return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
 }
 (StatusCode::OK, Json(ApiOk { status: "ok" })).into_response()
}

#[derive(Deserialize)]
pub struct RegisterMicrochain {
 pub microchain_id: Uuid,
 pub pubkey_hex: String,
 pub owner: Option<String>,
}

pub async fn register_microchain(State(state): State<ApiState>, Json(payload): Json<RegisterMicrochain>) -> impl IntoResponse {
 // validate pubkey - must be exactly 32 bytes for ed25519
 let pubkey = match hex::decode(&payload.pubkey_hex) {
 Ok(pk) => pk,
 Err(e) => return (StatusCode::BAD_REQUEST, format!("invalid pubkey hex: {}", e)).into_response(),
 };

 if pubkey.len() != 32 {
 return (StatusCode::BAD_REQUEST, format!("pubkey must be exactly 32 bytes, got {}", pubkey.len())).into_response();
 }


 // create controller and assign
 let controller = crate::controller::Controller::new((*state.pg).clone(), 1000i64);
 match controller.assign_microchain(payload.microchain_id).await {
 Ok(sub_id) => (StatusCode::OK, Json(serde_json::json!({"status":"ok","assigned_subchain": sub_id}))).into_response(),
 Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("assignment failed: {}", e)).into_response(),
 }
}

pub fn router(pg: Arc<PgPool>) -> Router {
 // build manager and state for router
 // Note: SubchainManager already opens the sled database, no need to open SubStore separately
 let manager = SubchainManager::open("subchain-api", Some(Arc::new((*pg).clone()))).expect("open manager");

 let state = ApiState { manager: Arc::new(manager), pg };
 Router::new()
 .route("/batch_anchor", post(post_batch_anchor))
 .route("/header", post(append_header))
 .route("/register_microchain", post(register_microchain))
 .layer(middleware::from_fn(crate::api::auth_middleware)) // LOCKED AUTH REQUIRED
 .with_state(state)
}
