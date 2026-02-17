// src/vm/mod.rs
//! Ouroboros Virtual Machine (OVM)
//!
//! Hybrid WASM + Native execution environment for smart contracts.
//!
//! # Architecture
//!
//! - **WASM Contracts**: Sandboxed user contracts with gas metering
//! - **Native Precompiles**: High-performance crypto and math at native speed
//! - **Host Functions**: Bridge between WASM and native Rust
//! - **Storage Layer**: RocksDB-backed persistent contract state
//!
//! # Example
//!
//! ```rust,ignore
//! use ouro_dag::vm::{OuroborosVM, ExecutionContext};
//!
//! // Create VM
//! let vm = OuroborosVM::new(storage, 10_000_000);
//!
//! // Deploy contract
//! let address = vm.deploy_contract(wasm_code, deployer, None, None)?;
//!
//! // Call contract
//! let context = ExecutionContext { /* ... */ };
//! let result = vm.call_contract(context, "transfer", &args)?;
//! ```

pub mod api;
pub mod contract_macros;
pub mod gas;
pub mod host_functions;
pub mod ovm;
pub mod precompiles;
pub mod storage;
pub mod types;

// Re-export main types
pub use gas::{GasCosts, GasState};
pub use ovm::OuroborosVM;
pub use precompiles::Precompiles;
pub use storage::ContractStorage;
pub use types::{
    AbiFunction, AbiType, ContractAbi, ContractAddress, ContractLog, ContractMetadata,
    ContractResult, ExecutionContext, StorageKey,
};

use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Contract call payload parsed from transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ContractCallPayload {
    /// Contract address (hex)
    contract: String,
    /// Method to call
    method: String,
    /// Arguments (JSON encoded)
    #[serde(default)]
    args: Vec<serde_json::Value>,
    /// Gas limit override
    #[serde(default)]
    gas_limit: Option<u64>,
}

/// Execute smart contracts for a batch of transactions
///
/// This function processes transactions that have contract call payloads
/// and executes them in the OuroborosVM.
pub fn execute_contracts(
    db: &crate::storage::RocksDb,
    transactions: &[crate::dag::transaction::Transaction],
) -> Result<Vec<ContractResult>, String> {
    let mut results = Vec::new();

    // Create contract storage from RocksDB (db is already Arc<DB>)
    let storage = Arc::new(ContractStorage::new(db.clone()));

    // Create VM instance with 10M gas limit default
    let vm = OuroborosVM::new(storage, 10_000_000);

    // Get current block info (best effort)
    let current_block = crate::storage::get_str::<u64>(db, "current_block_height")
        .unwrap_or(Some(0))
        .unwrap_or(0);

    let current_timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    for tx in transactions {
        // Check if transaction has a contract call payload
        let payload = match &tx.payload {
            Some(p) => p,
            None => {
                // No payload - not a contract call, skip
                continue;
            }
        };

        // Try to parse as contract call
        let call: ContractCallPayload = match serde_json::from_str(payload) {
            Ok(c) => c,
            Err(_) => {
                // Not a contract call payload format, skip
                continue;
            }
        };

        // Parse contract address
        let contract_address = match ContractAddress::from_hex(&call.contract) {
            Ok(addr) => addr,
            Err(e) => {
                results.push(ContractResult {
                    success: false,
                    return_data: vec![],
                    gas_used: 0,
                    error: Some(format!("Invalid contract address: {}", e)),
                    logs: vec![],
                });
                continue;
            }
        };

        // Build execution context
        let context = ExecutionContext {
            contract_address,
            caller: tx.sender.clone(),
            value: tx.amount,
            block_number: current_block,
            block_timestamp: current_timestamp,
            tx_hash: Some(tx.id.to_string()),
            gas_limit: call.gas_limit.unwrap_or(10_000_000),
            chain_id: 1, // Mainnet
        };

        // Encode args as bytes
        let args_bytes = serde_json::to_vec(&call.args).unwrap_or_default();

        // Execute contract call
        match vm.call_contract(context, &call.method, &args_bytes) {
            Ok(result) => {
                log::info!(
                    "Contract call {} on {} succeeded (gas: {})",
                    call.method,
                    call.contract,
                    result.gas_used
                );
                results.push(result);
            }
            Err(e) => {
                log::warn!(
                    "Contract call {} on {} failed: {}",
                    call.method,
                    call.contract,
                    e
                );
                results.push(ContractResult {
                    success: false,
                    return_data: vec![],
                    gas_used: 0,
                    error: Some(e.to_string()),
                    logs: vec![],
                });
            }
        }
    }

    Ok(results)
}
