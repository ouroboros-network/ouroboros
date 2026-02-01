pub mod block;
pub mod consensus;
pub mod crypto_bridge;
pub mod leader_rotation;
pub mod messages;
pub mod qc;
pub mod state;
pub mod validator_registry;
pub mod slashing; // Validator slashing mechanism

// Re-export commonly used items
pub use validator_registry::ValidatorRegistry;
pub use slashing::{SlashingManager, SlashingReason, SlashingSeverity};
