use crate::PgPool;
// src/subchain/api.rs
use crate::subchain::manager::SubchainManager;
use axum::{
    extract::State, http::StatusCode, middleware, response::IntoResponse, routing::post, Json,
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct ApiState {
    pub manager: Arc<SubchainManager>,
    pub pg: Arc<PgPool>,
    pub registry: Arc<crate::subchain::registry::SubchainRegistry>,
}

#[derive(Deserialize)]
pub struct PostBatchAnchor {
    pub batch_root: Vec<u8>,
    pub aggregator: String,
    pub leaf_count: usize,
    pub serialized_leaves_ref: Option<String>,
}

#[derive(Serialize)]
pub struct ApiOk {
    pub status: &'static str,
}

pub async fn post_batch_anchor(
    State(_state): State<ApiState>,
    Json(_req): Json<PostBatchAnchor>,
) -> impl IntoResponse {
    (StatusCode::OK, Json(ApiOk { status: "ok" })).into_response()
}

#[derive(Deserialize)]
pub struct AppendHeaderPayload {
    pub height: u64,
    pub batch_roots: Vec<String>, // hex strings
}

pub async fn append_header(
    State(state): State<ApiState>,
    Json(payload): Json<AppendHeaderPayload>,
) -> impl IntoResponse {
    // Validate and decode batch roots - don't silently swallow errors
    let batch_anchors: Result<Vec<Vec<u8>>, String> = payload
        .batch_roots
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

pub async fn register_microchain(
    State(state): State<ApiState>,
    Json(payload): Json<RegisterMicrochain>,
) -> impl IntoResponse {
    // validate pubkey - must be exactly 32 bytes for ed25519
    let pubkey = match hex::decode(&payload.pubkey_hex) {
        Ok(pk) => pk,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                format!("invalid pubkey hex: {}", e),
            )
                .into_response()
        }
    };

    if pubkey.len() != 32 {
        return (
            StatusCode::BAD_REQUEST,
            format!("pubkey must be exactly 32 bytes, got {}", pubkey.len()),
        )
            .into_response();
    }

    // create controller and assign
    let controller = crate::controller::Controller::new((*state.pg).clone(), 1000i64);
    match controller.assign_microchain(payload.microchain_id).await {
        Ok(sub_id) => (
            StatusCode::OK,
            Json(serde_json::json!({"status":"ok","assigned_subchain": sub_id})),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("assignment failed: {}", e),
        )
            .into_response(),
    }
}

pub async fn advertise_subchain(
    State(state): State<ApiState>,
    Json(ad): Json<crate::subchain::registry::SubchainAdvertisement>,
) -> impl IntoResponse {
    state.registry.advertise(ad);
    (StatusCode::OK, Json(serde_json::json!({"status": "ok"})))
}

pub async fn discover_subchains(
    State(state): State<ApiState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let app_type = params.get("type");
    let ads = if let Some(t) = app_type {
        state.registry.discover(t)
    } else {
        state.registry.get_all()
    };
    (StatusCode::OK, Json(ads))
}

pub fn router(pg: Arc<PgPool>, registry: Arc<crate::subchain::registry::SubchainRegistry>) -> Router {
    // build manager and state for router
    // Note: SubchainManager already opens the sled database, no need to open SubStore separately
    let manager =
        SubchainManager::open("subchain-api", Some(Arc::new((*pg).clone()))).expect("open manager");

    let state = ApiState {
        manager: Arc::new(manager),
        pg,
        registry,
    };
    Router::new()
        .route("/batch_anchor", post(post_batch_anchor))
        .route("/header", post(append_header))
        .route("/register_microchain", post(register_microchain))
        .route("/advertise", post(advertise_subchain))
        .route("/discover", axum::routing::get(discover_subchains))
        .layer(middleware::from_fn(crate::api::auth_middleware)) // LOCKED AUTH REQUIRED
        .with_state(state)
}
