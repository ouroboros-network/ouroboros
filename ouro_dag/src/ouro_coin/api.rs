use crate::PgPool;
// src/ouro_coin/api.rs
// REST API endpoints for OURO Coin

use crate::ouro_coin::{OuroCoinManager, DECIMALS, TOTAL_SUPPLY};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Serialize)]
struct OuroInfo {
    name: String,
    symbol: String,
    total_supply: u64,
    decimals: u8,
    circulating_supply: u64,
}

#[derive(Debug, Deserialize)]
struct TransferRequest {
    from_address: String,
    to_address: String,
    amount: u64,
    fee: u64,
    nonce: u64,
    signature: String,
    public_key: String,
}

#[derive(Debug, Serialize)]
struct TransferResponse {
    tx_id: String,
    status: String,
}

#[derive(Debug, Serialize)]
struct BalanceResponse {
    address: String,
    balance: u64,
    locked: u64,
    balance_readable: String, // Balance in OURO (with decimals)
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

/// Build Ouro Coin API router
pub fn router(db_pool: Arc<PgPool>) -> Router {
    let manager = Arc::new(OuroCoinManager::new(db_pool));

    Router::new()
        .route("/info", get(get_ouro_info))
        .route("/balance/:address", get(get_balance))
        .route("/nonce/:address", get(get_nonce))
        .route("/transfer", post(transfer))
        .route("/verify_supply", get(verify_supply))
        .route("/init_genesis", post(init_genesis))
        .with_state(manager)
}

/// GET /ouro/info - Get OURO coin information
async fn get_ouro_info(
    State(manager): State<Arc<OuroCoinManager>>,
) -> Result<Json<OuroInfo>, (StatusCode, Json<ErrorResponse>)> {
    let circulating = manager.get_circulating_supply().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
    })?;

    Ok(Json(OuroInfo {
        name: "Ouroboros".to_string(),
        symbol: "OURO".to_string(),
        total_supply: TOTAL_SUPPLY,
        decimals: DECIMALS,
        circulating_supply: circulating,
    }))
}

/// GET /ouro/balance/:address - Get balance for address
async fn get_balance(
    State(manager): State<Arc<OuroCoinManager>>,
    Path(address): Path<String>,
) -> Result<Json<BalanceResponse>, (StatusCode, Json<ErrorResponse>)> {
    let balance = manager.get_balance(&address).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
    })?;

    match balance {
        Some(b) => {
            let balance_ouro = b.balance as f64 / 10_f64.powi(DECIMALS as i32);
            Ok(Json(BalanceResponse {
                address: b.address,
                balance: b.balance,
                locked: b.locked,
                balance_readable: format!("{:.4} OURO", balance_ouro),
            }))
        }
        None => Ok(Json(BalanceResponse {
            address,
            balance: 0,
            locked: 0,
            balance_readable: "0.0000 OURO".to_string(),
        })),
    }
}

/// GET /ouro/nonce/:address - Get next nonce for address
async fn get_nonce(
    State(manager): State<Arc<OuroCoinManager>>,
    Path(address): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let nonce = manager.get_nonce(&address).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
    })?;

    Ok(Json(serde_json::json!({
    "address": address,
    "next_nonce": nonce
    })))
}

/// POST /ouro/transfer - Transfer OURO between addresses (with signature verification)
async fn transfer(
    State(manager): State<Arc<OuroCoinManager>>,
    Json(req): Json<TransferRequest>,
) -> Result<Json<TransferResponse>, (StatusCode, Json<ErrorResponse>)> {
    // SECURITY: Signature verification happens inside transfer() function

    let tx_id = manager
        .transfer(
            &req.from_address,
            &req.to_address,
            req.amount,
            req.fee,
            req.nonce,
            &req.signature,
            &req.public_key,
        )
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            )
        })?;

    Ok(Json(TransferResponse {
        tx_id: tx_id.to_string(),
        status: "confirmed".to_string(),
    }))
}

/// GET /ouro/verify_supply - Verify supply integrity
async fn verify_supply(
    State(manager): State<Arc<OuroCoinManager>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let circulating = manager.get_circulating_supply().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
    })?;

    let is_valid = manager.verify_supply_integrity().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
    })?;

    Ok(Json(serde_json::json!({
    "total_supply": TOTAL_SUPPLY,
    "circulating_supply": circulating,
    "is_valid": is_valid,
    "message": if is_valid {
    "Supply integrity verified"
    } else {
    "WARNING: Supply mismatch detected!"
    }
    })))
}

/// POST /ouro/init_genesis - Initialize genesis allocation (73M OURO)
async fn init_genesis(
    State(manager): State<Arc<OuroCoinManager>>,
    Json(req): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let genesis_address = req
        .get("genesis_address")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "genesis_address required".to_string(),
                }),
            )
        })?;

    manager
        .initialize_genesis(genesis_address)
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            )
        })?;

    Ok(Json(serde_json::json!({
    "status": "success",
    "message": format!("{} OURO allocated to {}", TOTAL_SUPPLY, genesis_address),
    "total_supply": TOTAL_SUPPLY
    })))
}
