// src/vm/api.rs
//! REST API endpoints for contract deployment and execution

use super::ovm::OuroborosVM;
use super::types::{ContractAddress, ContractResult, ExecutionContext};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Contract deployment request
#[derive(Debug, Serialize, Deserialize)]
pub struct DeployRequest {
    /// WASM bytecode (hex encoded)
    pub code: String,

    /// Deployer address
    pub deployer: String,

    /// Optional contract name
    pub name: Option<String>,

    /// Optional contract version
    pub version: Option<String>,

    /// Gas limit for deployment
    #[serde(default = "default_gas_limit")]
    pub gas_limit: u64,
}

fn default_gas_limit() -> u64 {
    10_000_000
}

/// Contract deployment response
#[derive(Debug, Serialize)]
pub struct DeployResponse {
    pub success: bool,
    pub contract_address: Option<String>,
    pub error: Option<String>,
}

/// Contract call request
#[derive(Debug, Deserialize)]
pub struct CallRequest {
    /// Contract address (hex)
    pub contract_address: String,

    /// Method name
    pub method: String,

    /// Arguments (hex encoded)
    pub args: String,

    /// Caller address
    pub caller: String,

    /// Value to send (in smallest units)
    #[serde(default)]
    pub value: u64,

    /// Gas limit
    #[serde(default = "default_gas_limit")]
    pub gas_limit: u64,
}

/// Contract call response
#[derive(Debug, Serialize)]
pub struct CallResponse {
    pub success: bool,
    pub return_data: String, // hex encoded
    pub gas_used: u64,
    pub error: Option<String>,
    pub logs: Vec<LogEntry>,
}

#[derive(Debug, Serialize)]
pub struct LogEntry {
    pub contract_address: String,
    pub data: String, // hex encoded
    pub block_number: u64,
}

/// Contract info response
#[derive(Debug, Serialize)]
pub struct ContractInfoResponse {
    pub address: String,
    pub owner: String,
    pub code_size: usize,
    pub deployed_at: String,
    pub total_gas_used: u64,
    pub call_count: u64,
    pub balance: u64,
    pub name: Option<String>,
    pub version: Option<String>,
}

/// Create contract API router
pub fn router(vm: Arc<OuroborosVM>) -> Router {
    Router::new()
        .route("/deploy", post(deploy_contract))
        .route("/call", post(call_contract))
        .route("/:address", get(get_contract_info))
        .route("/:address/code", get(get_contract_code))
        .with_state(vm)
}

/// Deploy a new contract
async fn deploy_contract(
    State(vm): State<Arc<OuroborosVM>>,
    Json(req): Json<DeployRequest>,
) -> Result<Json<DeployResponse>, (StatusCode, String)> {
    // Decode WASM code
    let code = hex::decode(&req.code)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid hex code: {}", e)))?;

    // Deploy contract
    match vm.deploy_contract(code.as_slice(), req.deployer, req.name, req.version) {
        Ok(address) => Ok(Json(DeployResponse {
            success: true,
            contract_address: Some(address.to_hex()),
            error: None,
        })),
        Err(e) => Ok(Json(DeployResponse {
            success: false,
            contract_address: None,
            error: Some(e.to_string()),
        })),
    }
}

/// Call a contract method
async fn call_contract(
    State(vm): State<Arc<OuroborosVM>>,
    Json(req): Json<CallRequest>,
) -> Result<Json<CallResponse>, (StatusCode, String)> {
    // Parse contract address
    let address = ContractAddress::from_hex(&req.contract_address)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid address: {}", e)))?;

    // Decode arguments
    let args = hex::decode(&req.args)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid hex args: {}", e)))?;

    // Create execution context
    let context = ExecutionContext {
        contract_address: address,
        caller: req.caller,
        value: req.value,
        block_number: 0, // TODO: Get from chain state
        block_timestamp: chrono::Utc::now().timestamp() as u64,
        tx_hash: None,
        gas_limit: req.gas_limit,
        chain_id: 1, // TODO: Get from config
    };

    // Call contract
    match vm.call_contract(context, &req.method, &args) {
        Ok(result) => {
            let logs = result
                .logs
                .iter()
                .map(|log| LogEntry {
                    contract_address: log.contract_address.to_hex(),
                    data: hex::encode(&log.data),
                    block_number: log.block_number,
                })
                .collect();

            Ok(Json(CallResponse {
                success: result.success,
                return_data: hex::encode(&result.return_data),
                gas_used: result.gas_used,
                error: result.error,
                logs,
            }))
        }
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

/// Get contract info
async fn get_contract_info(
    State(vm): State<Arc<OuroborosVM>>,
    Path(address_hex): Path<String>,
) -> Result<Json<ContractInfoResponse>, (StatusCode, String)> {
    let address = ContractAddress::from_hex(&address_hex)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid address: {}", e)))?;

    match vm.get_contract_metadata(address) {
        Ok(Some(metadata)) => Ok(Json(ContractInfoResponse {
            address: metadata.address.to_hex(),
            owner: metadata.owner,
            code_size: metadata.code_size,
            deployed_at: metadata.deployed_at.to_rfc3339(),
            total_gas_used: metadata.total_gas_used,
            call_count: metadata.call_count,
            balance: metadata.balance,
            name: metadata.name,
            version: metadata.version,
        })),
        Ok(None) => Err((StatusCode::NOT_FOUND, "Contract not found".to_string())),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

/// Get contract code
async fn get_contract_code(
    State(vm): State<Arc<OuroborosVM>>,
    Path(address_hex): Path<String>,
) -> Result<String, (StatusCode, String)> {
    let address = ContractAddress::from_hex(&address_hex)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid address: {}", e)))?;

    match vm.storage().load_contract_code(address) {
        Ok(code) => Ok(hex::encode(code)),
        Err(_) => Err((StatusCode::NOT_FOUND, "Contract not found".to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::storage::ContractStorage;
    use axum::body::Body;
    use axum::http::Request;
    use rocksdb::{Options, DB};
    use tempfile::tempdir;
    use tower::ServiceExt;

    async fn create_test_app() -> Router {
        let dir = tempdir().unwrap();
        let mut opts = Options::default();
        opts.create_if_missing(true);
        let db = Arc::new(DB::open(&opts, dir.path()).unwrap());
        let storage = Arc::new(ContractStorage::new(db));
        let vm = Arc::new(OuroborosVM::new(storage, 10_000_000));
        router(vm)
    }

    #[tokio::test]
    async fn test_deploy_contract_api() {
        let app = create_test_app().await;

        // Simple WASM module
        let wasm = wat::parse_str(
            r#"
 (module
 (func (export "test") (result i32)
 i32.const 42
 )
 )
 "#,
        )
        .unwrap();

        let request_body = DeployRequest {
            code: hex::encode(&wasm),
            deployer: "test_deployer".to_string(),
            name: Some("TestContract".to_string()),
            version: Some("1.0.0".to_string()),
            gas_limit: 10_000_000,
        };

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/deploy")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&request_body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
