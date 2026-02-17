// src/mainchain/api.rs
use crate::anchor_service::AnchorService;
use axum::{
    extract::State, http::StatusCode, middleware, response::IntoResponse, routing::post, Json,
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct MainchainState {
    pub anchor_svc: Arc<AnchorService>,
}

#[derive(Deserialize)]
pub struct AnchorPost {
    pub subchain: Uuid,
    pub block_height: i64,
    pub root: Vec<u8>,
}

#[derive(Serialize)]
struct AnchorResp {
    status: &'static str,
    txid: String,
}

pub async fn post_anchor(
    State(state): State<MainchainState>,
    Json(payload): Json<AnchorPost>,
) -> impl IntoResponse {
    match state
        .anchor_svc
        .post_anchor(payload.subchain, payload.block_height, &payload.root)
        .await
    {
        Ok(txid) => (StatusCode::OK, Json(AnchorResp { status: "ok", txid })).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("post_anchor failed: {}", e),
        )
            .into_response(),
    }
}

pub fn router(anchor_svc: Arc<AnchorService>) -> Router {
    let state = MainchainState { anchor_svc };
    Router::new()
        .route("/anchors", post(post_anchor))
        .layer(middleware::from_fn(crate::api::auth_middleware)) // LOCKED AUTH REQUIRED
        .with_state(state)
}
