use thiserror::Error;

pub type Result<T> = std::result::Result<T, SdkError>;

#[derive(Error, Debug)]
pub enum SdkError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Transaction failed: {0}")]
    TransactionFailed(String),

    #[error("Microchain not found: {0}")]
    MicrochainNotFound(String),

    #[error("Insufficient balance: required {required}, available {available}")]
    InsufficientBalance { required: u64, available: u64 },

    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Anchor failed: {0}")]
    AnchorFailed(String),

    #[error("RPC error: {0}")]
    Rpc(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(String),
}

impl From<String> for SdkError {
    fn from(s: String) -> Self {
        SdkError::Other(s)
    }
}

impl From<&str> for SdkError {
    fn from(s: &str) -> Self {
        SdkError::Other(s.to_string())
    }
}
