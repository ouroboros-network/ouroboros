// src/vm/contract_macros.rs
//! Contract development macros and helpers
//!
//! Provides utilities to make writing OVM contracts easier.
//! Full procedural macros would go in a separate crate, but these are helper traits.

use serde::{Deserialize, Serialize};

/// Trait for OVM contract state
///
/// All contract state structs should implement this trait
pub trait ContractState: Serialize + for<'de> Deserialize<'de> + Default {
    /// Load state from storage
    fn load() -> Self {
        // In actual implementation, this would read from contract storage
        Self::default()
    }

    /// Save state to storage
    fn save(&self) {
        // In actual implementation, this would write to contract storage
    }
}

/// Contract method dispatcher
///
/// Automatically routes method calls to the correct handler
#[macro_export]
macro_rules! contract_dispatcher {
 (
 state: $state:ty,
 methods: {
 $(
 $method_name:literal => $handler:expr
 ),* $(,)?
 }
 ) => {
 pub fn dispatch(method: &str, args: &[u8]) -> Result<Vec<u8>, String> {
 match method {
 $(
 $method_name => {
 let handler: fn(&mut $state, &[u8]) -> Result<Vec<u8>, String> = $handler;
 let mut state = <$state>::load();
 let result = handler(&mut state, args)?;
 state.save();
 Ok(result)
 }
 )*
 _ => Err(format!("Unknown method: {}", method)),
 }
 }
 };
}

/// Helper to decode arguments
pub fn decode_args<T: for<'de> Deserialize<'de>>(args: &[u8]) -> Result<T, String> {
    serde_json::from_slice(args).map_err(|e| format!("Failed to decode args: {}", e))
}

/// Helper to encode return value
pub fn encode_result<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    serde_json::to_vec(value).map_err(|e| format!("Failed to encode result: {}", e))
}

/// Standard contract errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContractError {
    /// Caller is not authorized
    Unauthorized,

    /// Insufficient balance
    InsufficientBalance,

    /// Invalid arguments
    InvalidArguments(String),

    /// Transfer failed
    TransferFailed(String),

    /// Custom error
    Custom(String),
}

impl std::fmt::Display for ContractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContractError::Unauthorized => write!(f, "Unauthorized"),
            ContractError::InsufficientBalance => write!(f, "Insufficient balance"),
            ContractError::InvalidArguments(msg) => write!(f, "Invalid arguments: {}", msg),
            ContractError::TransferFailed(msg) => write!(f, "Transfer failed: {}", msg),
            ContractError::Custom(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for ContractError {}

/// Event logging for contracts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractEvent {
    pub name: String,
    pub data: serde_json::Value,
}

impl ContractEvent {
    pub fn new(name: &str, data: serde_json::Value) -> Self {
        Self {
            name: name.to_string(),
            data,
        }
    }

    /// Emit an event (in actual implementation, this would call host function)
    pub fn emit(&self) {
        // In WASM, this would call the log host function
        println!("Event: {} - {:?}", self.name, self.data);
    }
}

/// Helper macro for emitting events
#[macro_export]
macro_rules! emit_event {
 ($name:expr, $($key:expr => $value:expr),* $(,)?) => {
 {
 let mut map = serde_json::Map::new();
 $(
 map.insert($key.to_string(), serde_json::to_value($value).unwrap());
 )*
 let event = ContractEvent::new($name, serde_json::Value::Object(map));
 event.emit();
 }
 };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize, Deserialize, Default)]
    struct TestState {
        value: u64,
    }

    impl ContractState for TestState {}

    #[test]
    fn test_encode_decode() {
        let value = 42u64;
        let encoded = encode_result(&value).unwrap();
        let decoded: u64 = decode_args(&encoded).unwrap();
        assert_eq!(value, decoded);
    }

    #[test]
    fn test_contract_event() {
        let event = ContractEvent::new(
            "Transfer",
            serde_json::json!({
            "from": "alice",
            "to": "bob",
            "amount": 100
            }),
        );

        assert_eq!(event.name, "Transfer");
        event.emit();
    }

    #[test]
    fn test_emit_event_macro() {
        emit_event!("Transfer",
        "from" => "alice",
        "to" => "bob",
        "amount" => 100u64
        );
    }
}
