//! Native (built-in) smart contracts
//!
//! These contracts are compiled into the node binary and execute at native speed,
//! unlike user-deployed WASM contracts in the VM module. They handle core protocol
//! operations that need to be fast and trustworthy.
//!
//! Native contracts:
//! - Token transfer (OURO coin operations)
//! - Staking (validator stake management)
//! - Bridge (cross-chain lock/mint)
//! - Governance (proposal/voting execution)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Unique identifier for a native contract
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum NativeContractId {
    /// Core OURO token transfer contract
    TokenTransfer,
    /// Validator staking contract
    Staking,
    /// Cross-chain bridge contract
    Bridge,
    /// Governance execution contract
    Governance,
    /// Custom named contract
    Custom(String),
}

impl std::fmt::Display for NativeContractId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NativeContractId::TokenTransfer => write!(f, "native:token_transfer"),
            NativeContractId::Staking => write!(f, "native:staking"),
            NativeContractId::Bridge => write!(f, "native:bridge"),
            NativeContractId::Governance => write!(f, "native:governance"),
            NativeContractId::Custom(name) => write!(f, "native:{}", name),
        }
    }
}

/// Input to a native contract call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NativeContractCall {
    /// Which contract to invoke
    pub contract: NativeContractId,
    /// Function/method to call
    pub method: String,
    /// Caller address
    pub caller: String,
    /// Call arguments (JSON-encoded)
    pub args: serde_json::Value,
    /// Gas limit for this call
    pub gas_limit: u64,
}

/// Result of a native contract execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NativeContractResult {
    /// Whether execution succeeded
    pub success: bool,
    /// Return data (JSON-encoded)
    pub return_data: serde_json::Value,
    /// Gas used
    pub gas_used: u64,
    /// Error message if failed
    pub error: Option<String>,
    /// State changes to apply
    pub state_changes: Vec<StateChange>,
}

/// A state change produced by contract execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateChange {
    /// Storage key
    pub key: String,
    /// New value (None = delete)
    pub value: Option<Vec<u8>>,
}

/// Registry of native contracts
pub struct NativeContractRegistry {
    /// Registered contracts and their gas costs per method
    gas_table: HashMap<(NativeContractId, String), u64>,
}

impl NativeContractRegistry {
    /// Create registry with default gas costs
    pub fn new() -> Self {
        let mut gas_table = HashMap::new();

        // Token transfer costs
        gas_table.insert(
            (NativeContractId::TokenTransfer, "transfer".to_string()),
            21_000,
        );
        gas_table.insert(
            (NativeContractId::TokenTransfer, "balance_of".to_string()),
            5_000,
        );

        // Staking costs
        gas_table.insert(
            (NativeContractId::Staking, "stake".to_string()),
            50_000,
        );
        gas_table.insert(
            (NativeContractId::Staking, "unstake".to_string()),
            50_000,
        );
        gas_table.insert(
            (NativeContractId::Staking, "claim_rewards".to_string()),
            30_000,
        );

        // Bridge costs
        gas_table.insert(
            (NativeContractId::Bridge, "lock".to_string()),
            100_000,
        );
        gas_table.insert(
            (NativeContractId::Bridge, "mint".to_string()),
            100_000,
        );

        // Governance costs
        gas_table.insert(
            (NativeContractId::Governance, "vote".to_string()),
            25_000,
        );
        gas_table.insert(
            (NativeContractId::Governance, "propose".to_string()),
            75_000,
        );

        Self { gas_table }
    }

    /// Get the gas cost for a contract method call
    pub fn gas_cost(&self, contract: &NativeContractId, method: &str) -> u64 {
        self.gas_table
            .get(&(contract.clone(), method.to_string()))
            .copied()
            .unwrap_or(21_000) // Default gas cost
    }

    /// Execute a native contract call
    pub fn execute(&self, call: &NativeContractCall) -> NativeContractResult {
        let gas_cost = self.gas_cost(&call.contract, &call.method);

        if call.gas_limit < gas_cost {
            return NativeContractResult {
                success: false,
                return_data: serde_json::Value::Null,
                gas_used: call.gas_limit,
                error: Some("Insufficient gas".to_string()),
                state_changes: vec![],
            };
        }

        match &call.contract {
            NativeContractId::TokenTransfer => self.execute_token_transfer(call, gas_cost),
            NativeContractId::Staking => self.execute_staking(call, gas_cost),
            _ => NativeContractResult {
                success: false,
                return_data: serde_json::Value::Null,
                gas_used: gas_cost,
                error: Some(format!("Contract {} not yet implemented", call.contract)),
                state_changes: vec![],
            },
        }
    }

    fn execute_token_transfer(&self, call: &NativeContractCall, gas_cost: u64) -> NativeContractResult {
        match call.method.as_str() {
            "transfer" => {
                let to = call.args.get("to").and_then(|v| v.as_str());
                let amount = call.args.get("amount").and_then(|v| v.as_u64());

                match (to, amount) {
                    (Some(to), Some(amount)) => NativeContractResult {
                        success: true,
                        return_data: serde_json::json!({
                            "from": call.caller,
                            "to": to,
                            "amount": amount,
                        }),
                        gas_used: gas_cost,
                        error: None,
                        state_changes: vec![
                            StateChange {
                                key: format!("balance:{}", call.caller),
                                value: None, // Debit handled by caller
                            },
                            StateChange {
                                key: format!("balance:{}", to),
                                value: None, // Credit handled by caller
                            },
                        ],
                    },
                    _ => NativeContractResult {
                        success: false,
                        return_data: serde_json::Value::Null,
                        gas_used: gas_cost,
                        error: Some("Missing 'to' or 'amount' argument".to_string()),
                        state_changes: vec![],
                    },
                }
            }
            "balance_of" => {
                NativeContractResult {
                    success: true,
                    return_data: serde_json::json!({ "balance": 0 }),
                    gas_used: gas_cost,
                    error: None,
                    state_changes: vec![],
                }
            }
            _ => NativeContractResult {
                success: false,
                return_data: serde_json::Value::Null,
                gas_used: gas_cost,
                error: Some(format!("Unknown method: {}", call.method)),
                state_changes: vec![],
            },
        }
    }

    fn execute_staking(&self, call: &NativeContractCall, gas_cost: u64) -> NativeContractResult {
        match call.method.as_str() {
            "stake" | "unstake" | "claim_rewards" => {
                NativeContractResult {
                    success: true,
                    return_data: serde_json::json!({
                        "method": call.method,
                        "caller": call.caller,
                        "status": "ok",
                    }),
                    gas_used: gas_cost,
                    error: None,
                    state_changes: vec![],
                }
            }
            _ => NativeContractResult {
                success: false,
                return_data: serde_json::Value::Null,
                gas_used: gas_cost,
                error: Some(format!("Unknown method: {}", call.method)),
                state_changes: vec![],
            },
        }
    }
}

impl Default for NativeContractRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_transfer() {
        let registry = NativeContractRegistry::new();
        let call = NativeContractCall {
            contract: NativeContractId::TokenTransfer,
            method: "transfer".to_string(),
            caller: "alice".to_string(),
            args: serde_json::json!({ "to": "bob", "amount": 1000 }),
            gas_limit: 100_000,
        };

        let result = registry.execute(&call);
        assert!(result.success);
        assert_eq!(result.gas_used, 21_000);
    }

    #[test]
    fn test_insufficient_gas() {
        let registry = NativeContractRegistry::new();
        let call = NativeContractCall {
            contract: NativeContractId::TokenTransfer,
            method: "transfer".to_string(),
            caller: "alice".to_string(),
            args: serde_json::json!({ "to": "bob", "amount": 1000 }),
            gas_limit: 100, // Too low
        };

        let result = registry.execute(&call);
        assert!(!result.success);
        assert!(result.error.unwrap().contains("Insufficient gas"));
    }

    #[test]
    fn test_contract_id_display() {
        assert_eq!(NativeContractId::TokenTransfer.to_string(), "native:token_transfer");
        assert_eq!(NativeContractId::Custom("escrow".into()).to_string(), "native:escrow");
    }
}
