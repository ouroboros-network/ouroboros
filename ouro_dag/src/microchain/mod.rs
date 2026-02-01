// src/microchain/mod.rs
pub mod store;
pub mod api;
pub mod security;
pub mod integration;
pub mod challenges;

pub use security::{SecurityMode, TransactionSignature};
pub use integration::{
 SecurityModeStore,
 register_microchain_security,
 validate_microchain_transaction,
 get_security_mode,
 is_key_authorized,
 MicrochainRegistration,
};
pub use challenges::{
 ChallengeManager,
 StateAnchor,
 Challenge,
 ChallengeType,
 ForceExitRequest,
};
