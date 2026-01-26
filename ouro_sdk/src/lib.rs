pub mod microchain;
pub mod subchain;
pub mod transaction;
pub mod client;
pub mod types;
pub mod error;

pub use microchain::{Microchain, MicrochainBuilder};
pub use subchain::{Subchain, SubchainBuilder, SubchainConfig, SubchainStatus, ValidatorConfig};
pub use transaction::{Transaction, TransactionBuilder};
pub use client::OuroClient;
pub use types::{MicrochainConfig, ConsensusType, AnchorFrequency};
pub use error::{SdkError, Result};

/// SDK version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Prelude module for convenient imports
pub mod prelude {
    pub use crate::microchain::Microchain;
    pub use crate::subchain::{Subchain, SubchainBuilder, SubchainConfig};
    pub use crate::transaction::Transaction;
    pub use crate::client::OuroClient;
    pub use crate::types::*;
    pub use crate::error::{SdkError, Result};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
    }
}
