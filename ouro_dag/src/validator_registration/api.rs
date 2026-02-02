// src/validator_registration/api.rs
//! API endpoints for validator registration and management

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use super::{ValidatorRegistry, MIN_VALIDATOR_STAKE};

/// API request to register a new validator
#[derive(Debug, Deserialize)]
pub struct RegisterValidatorRequest {
    /// Ouro address (must hold staked OURO)
    pub address: String,
    /// Ed25519 public key for BFT consensus (hex encoded)
    pub bft_pubkey: String,
    /// Network endpoint for P2P (IP:port or .onion:port)
    pub network_endpoint: String,
    /// BFT consensus port
    pub bft_port: i32,
    /// Amount to stake in OURO microunits (minimum 200 OURO = 200_000_000)
    pub stake_amount: i64,
}

/// API response for validator registration
#[derive(Debug, Serialize)]
pub struct RegisterValidatorResponse {
    pub validator_id: Uuid,
    pub status: String,
    pub message: String,
}

/// API response for validator info
#[derive(Debug, Serialize)]
pub struct ValidatorInfo {
    pub id: Uuid,
    pub address: String,
    pub bft_pubkey: String,
    pub network_endpoint: String,
    pub bft_port: i32,
    pub stake_amount: i64,
    pub status: String,
    pub reputation: i32,
    pub blocks_proposed: i64,
    pub blocks_signed: i64,
    pub missed_proposals: i64,
    pub slashed_amount: i64,
    pub registered_at: String,
}

/// List all active validators
async fn list_validators(
    State(registry): State<Arc<ValidatorRegistry>>,
) -> Result<Json<Vec<ValidatorInfo>>, AppError> {
    let validators = registry.get_active_validators().await?;

    let info: Vec<ValidatorInfo> = validators
        .into_iter()
        .map(|v| ValidatorInfo {
            id: v.id,
            address: v.address,
            bft_pubkey: v.bft_pubkey,
            network_endpoint: v.network_endpoint,
            bft_port: v.bft_port as i32,
            stake_amount: v.stake_amount as i64,
            status: format!("{:?}", v.status),
            reputation: v.reputation as i32,
            blocks_proposed: v.blocks_proposed as i64,
            blocks_signed: v.blocks_signed as i64,
            missed_proposals: v.missed_proposals as i64,
            slashed_amount: v.slashed_amount as i64,
            registered_at: v.registered_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(info))
}

/// Register a new validator
async fn register_validator(
    State(registry): State<Arc<ValidatorRegistry>>,
    Json(req): Json<RegisterValidatorRequest>,
) -> Result<Json<RegisterValidatorResponse>, AppError> {
    // Validate minimum stake
    if req.stake_amount < MIN_VALIDATOR_STAKE as i64 {
        return Err(AppError::BadRequest(format!(
            "Insufficient stake: {} < {} OURO minimum",
            req.stake_amount / 1_000_000,
            MIN_VALIDATOR_STAKE / 1_000_000
        )));
    }

    let validator_id = registry
        .register_validator(
            req.address,
            req.bft_pubkey,
            req.network_endpoint,
            req.bft_port as u16,
            req.stake_amount as u64,
        )
        .await?;

    Ok(Json(RegisterValidatorResponse {
        validator_id,
        status: "pending".to_string(),
        message: format!(
            "Validator registered successfully. Stake: {} OURO. Awaiting activation.",
            req.stake_amount / 1_000_000
        ),
    }))
}

/// Get validator by ID
async fn get_validator(
    State(registry): State<Arc<ValidatorRegistry>>,
    Path(validator_id): Path<Uuid>,
) -> Result<Json<ValidatorInfo>, AppError> {
    let validator = registry
        .get_validator(validator_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Validator not found".to_string()))?;

    Ok(Json(ValidatorInfo {
        id: validator.id,
        address: validator.address,
        bft_pubkey: validator.bft_pubkey,
        network_endpoint: validator.network_endpoint,
        bft_port: validator.bft_port as i32,
        stake_amount: validator.stake_amount as i64,
        status: format!("{:?}", validator.status),
        reputation: validator.reputation as i32,
        blocks_proposed: validator.blocks_proposed as i64,
        blocks_signed: validator.blocks_signed as i64,
        missed_proposals: validator.missed_proposals as i64,
        slashed_amount: validator.slashed_amount as i64,
        registered_at: validator.registered_at.to_rfc3339(),
    }))
}

/// Activate a validator (admin/governance action)
async fn activate_validator(
    State(registry): State<Arc<ValidatorRegistry>>,
    Path(validator_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    registry.activate_validator(validator_id).await?;
    Ok(StatusCode::OK)
}

/// Request validator exit (starts unbonding period)
async fn request_exit(
    State(registry): State<Arc<ValidatorRegistry>>,
    Path(validator_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let unbonding_complete = registry.request_exit(validator_id).await?;

    Ok(Json(serde_json::json!({
    "message": "Exit requested. Unbonding period started.",
    "unbonding_complete_at": unbonding_complete.to_rfc3339(),
    "unbonding_days": 14
    })))
}

/// Error handling
#[derive(Debug)]
enum AppError {
    Internal(anyhow::Error),
    BadRequest(String),
    NotFound(String),
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        AppError::Internal(err)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::Internal(err) => {
                eprintln!("Internal error: {:?}", err);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
        };

        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

/// Create validator registration router
pub fn router(registry: Arc<ValidatorRegistry>) -> Router {
    Router::new()
        .route("/", get(list_validators))
        .route("/register", post(register_validator))
        .route("/:id", get(get_validator))
        .route("/:id/activate", post(activate_validator))
        .route("/:id/exit", post(request_exit))
        .with_state(registry)
}
