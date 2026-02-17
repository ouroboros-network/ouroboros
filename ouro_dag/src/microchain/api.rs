// src/microchain/api.rs
use axum::{
    extract::State,
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct MicroState {
    // TODO_ROCKSDB: Add RocksDB reference when implementing persistence
}

#[derive(Deserialize)]
pub struct AppendMicroHeader {/* extend as needed */}

#[derive(Deserialize)]
pub struct ProvisionalClaim {
    pub microchain_id: String,
    pub owner: String,
    pub amount: i64,
}

#[derive(Serialize)]
pub struct ApiOk {
    pub status: &'static str,
}

pub async fn append_header(
    State(_state): State<MicroState>,
    Json(_hdr): Json<AppendMicroHeader>,
) -> impl IntoResponse {
    (StatusCode::OK, Json(ApiOk { status: "ok" }))
}

pub async fn provisional_claim(
    State(_state): State<MicroState>,
    Json(_payload): Json<ProvisionalClaim>,
) -> impl IntoResponse {
    // TODO_ROCKSDB: Implement provisional claim storage with RocksDB
    (StatusCode::OK, Json(ApiOk { status: "ok" }))
}

pub async fn tip(State(_state): State<MicroState>) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(ApiOk {
            status: "tip-not-implemented",
        }),
    )
}

pub fn router() -> Router {
    let state = MicroState {};
    Router::new()
        .route("/append", post(append_header))
        .route("/provisional_claim", post(provisional_claim))
        .route("/tip", get(tip))
        .layer(middleware::from_fn(crate::api::auth_middleware)) // LOCKED AUTH REQUIRED
        .with_state(state)
}
