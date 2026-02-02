// src/vm/types.rs
//! Type definitions for OVM contracts

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

/// Contract address (32 bytes derived from code hash)
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContractAddress(pub [u8; 32]);

impl ContractAddress {
    /// Create from code hash
    pub fn from_code(code: &[u8]) -> Self {
        let hash = Sha256::digest(code);
        Self(hash.into())
    }

    /// Create from hex string
    pub fn from_hex(hex: &str) -> Result<Self, hex::FromHexError> {
        let bytes = hex::decode(hex)?;
        if bytes.len() != 32 {
            return Err(hex::FromHexError::InvalidStringLength);
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(Self(arr))
    }

    /// Convert to hex string
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Get as bytes
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for ContractAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}", self.to_hex())
    }
}

impl fmt::Debug for ContractAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ContractAddress({}...{})",
            &self.to_hex()[..8],
            &self.to_hex()[56..]
        )
    }
}

/// Contract metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractMetadata {
    /// Contract address
    pub address: ContractAddress,

    /// Owner address (who deployed)
    pub owner: String,

    /// WASM code size in bytes
    pub code_size: usize,

    /// Code hash (same as address)
    pub code_hash: String,

    /// When deployed
    pub deployed_at: chrono::DateTime<chrono::Utc>,

    /// Total gas used by this contract
    pub total_gas_used: u64,

    /// Number of calls
    pub call_count: u64,

    /// Contract balance (OURO in smallest units)
    pub balance: u64,

    /// Optional contract name/description
    pub name: Option<String>,

    /// Contract version
    pub version: Option<String>,
}

/// Contract execution context
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    /// Contract being executed
    pub contract_address: ContractAddress,

    /// Caller address
    pub caller: String,

    /// Value sent with call (in OURO smallest units)
    pub value: u64,

    /// Current block number
    pub block_number: u64,

    /// Current block timestamp
    pub block_timestamp: u64,

    /// Transaction hash
    pub tx_hash: Option<String>,

    /// Gas limit for this execution
    pub gas_limit: u64,

    /// Chain ID
    pub chain_id: u32,
}

/// Contract call result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractResult {
    /// Success or failure
    pub success: bool,

    /// Return data
    pub return_data: Vec<u8>,

    /// Gas used
    pub gas_used: u64,

    /// Error message (if failed)
    pub error: Option<String>,

    /// Logs emitted
    pub logs: Vec<ContractLog>,
}

/// Contract log event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractLog {
    /// Contract that emitted the log
    pub contract_address: ContractAddress,

    /// Topics (indexed parameters)
    pub topics: Vec<Vec<u8>>,

    /// Data (non-indexed parameters)
    pub data: Vec<u8>,

    /// Block number
    pub block_number: u64,

    /// Transaction index
    pub tx_index: u64,
}

/// Storage key for contract state
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StorageKey {
    /// Contract address
    pub contract: ContractAddress,

    /// Storage slot (32 bytes)
    pub key: [u8; 32],
}

impl StorageKey {
    /// Create from contract and key
    pub fn new(contract: ContractAddress, key: [u8; 32]) -> Self {
        Self { contract, key }
    }

    /// Create from hex
    pub fn from_hex(contract: ContractAddress, key_hex: &str) -> Result<Self, hex::FromHexError> {
        let bytes = hex::decode(key_hex)?;
        if bytes.len() != 32 {
            return Err(hex::FromHexError::InvalidStringLength);
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes);
        Ok(Self::new(contract, key))
    }

    /// Serialize to bytes for storage
    pub fn to_bytes(&self) -> [u8; 64] {
        let mut result = [0u8; 64];
        result[..32].copy_from_slice(&self.contract.0);
        result[32..].copy_from_slice(&self.key);
        result
    }
}

impl fmt::Debug for StorageKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "StorageKey({:?}, {})",
            self.contract,
            hex::encode(&self.key[..4])
        )
    }
}

/// ABI parameter types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AbiType {
    U8,
    U16,
    U32,
    U64,
    U128,
    I8,
    I16,
    I32,
    I64,
    I128,
    Bool,
    String,
    Bytes,
    Address,
    Array {
        inner: Box<AbiType>,
        size: Option<usize>,
    },
    Tuple {
        types: Vec<AbiType>,
    },
}

/// Contract ABI (Application Binary Interface)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractAbi {
    /// Contract functions
    pub functions: Vec<AbiFunction>,

    /// Events
    pub events: Vec<AbiEvent>,

    /// Constructor
    pub constructor: Option<AbiFunction>,
}

/// ABI function definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbiFunction {
    /// Function name
    pub name: String,

    /// Input parameters
    pub inputs: Vec<AbiParam>,

    /// Output parameters
    pub outputs: Vec<AbiParam>,

    /// Is function mutable (can modify state)
    pub mutability: FunctionMutability,
}

/// Function mutability
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum FunctionMutability {
    /// Can read and write state
    Mutable,
    /// Can only read state
    View,
    /// Cannot access state
    Pure,
}

/// ABI parameter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbiParam {
    /// Parameter name
    pub name: String,

    /// Parameter type
    #[serde(rename = "type")]
    pub param_type: AbiType,
}

/// ABI event definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbiEvent {
    /// Event name
    pub name: String,

    /// Event parameters
    pub inputs: Vec<AbiEventParam>,
}

/// ABI event parameter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbiEventParam {
    /// Parameter name
    pub name: String,

    /// Parameter type
    #[serde(rename = "type")]
    pub param_type: AbiType,

    /// Is indexed (searchable)
    pub indexed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contract_address_from_code() {
        let code = b"test contract code";
        let addr = ContractAddress::from_code(code);

        assert_eq!(addr.0.len(), 32);
        assert!(addr.to_hex().len() == 64);
    }

    #[test]
    fn test_contract_address_hex() {
        let hex = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let addr = ContractAddress::from_hex(hex).unwrap();

        assert_eq!(addr.to_hex(), hex);
    }

    #[test]
    fn test_storage_key() {
        let contract = ContractAddress([1u8; 32]);
        let key = [2u8; 32];
        let storage_key = StorageKey::new(contract, key);

        let bytes = storage_key.to_bytes();
        assert_eq!(bytes.len(), 64);
        assert_eq!(&bytes[..32], &contract.0);
        assert_eq!(&bytes[32..], &key);
    }
}
