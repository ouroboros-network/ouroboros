//! # Governance System
//!
//! Decentralized governance for Ouroboros blockchain with:
//! - Multisig timelock for critical operations (7-day delay)
//! - Emergency pause system (3/5 guardians)
//! - Proposal and voting mechanisms
//! - On-chain parameter updates

pub mod timelock;
pub mod pause;
pub mod proposals;
pub mod voting;
pub mod integration;

pub use timelock::{TimelockController, TimelockOperation, TimelockConfig, OperationType, OperationStatus};
pub use pause::{EmergencyPause, GuardianSet, PauseReason, PauseState};
pub use proposals::{Proposal, ProposalStatus, ProposalType, ProposalRegistry};
pub use voting::{Vote, VoteChoice, VotingPower, VotingRegistry};
pub use integration::{GovernanceIntegration, finalize_ended_proposals_task};

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Governance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceConfig {
 /// Timelock delay in seconds (default: 7 days)
 pub timelock_delay_secs: u64,

 /// Minimum guardians for emergency pause (default: 3)
 pub min_guardians_for_pause: usize,

 /// Total number of guardians (default: 5)
 pub total_guardians: usize,

 /// Minimum voting period in blocks (default: 100,800 blocks ≈ 7 days at 6s/block)
 pub min_voting_period: u64,

 /// Quorum percentage required (default: 40%)
 pub quorum_percentage: u8,

 /// Proposal threshold (min OURO to create proposal, default: 10,000 OURO)
 pub proposal_threshold: u64,
}

impl Default for GovernanceConfig {
 fn default() -> Self {
 Self {
 timelock_delay_secs: 7 * 24 * 60 * 60, // 7 days
 min_guardians_for_pause: 3,
 total_guardians: 5,
 min_voting_period: 100_800, // ~7 days at 6s/block
 quorum_percentage: 40,
 proposal_threshold: 10_000_000_000, // 10,000 OURO (8 decimals)
 }
 }
}

/// Main governance controller
pub struct GovernanceController {
 config: GovernanceConfig,
 timelock: Arc<RwLock<TimelockController>>,
 pause: Arc<RwLock<EmergencyPause>>,
 proposals: Arc<RwLock<ProposalRegistry>>,
 voting: Arc<RwLock<VotingRegistry>>,
}

impl GovernanceController {
 /// Create new governance controller
 pub fn new(config: GovernanceConfig, guardians: Vec<String>) -> Self {
 let timelock_config = TimelockConfig {
 delay_secs: config.timelock_delay_secs,
 admin_addresses: vec![], // Set via governance
 };

 let guardian_set = GuardianSet::new(
 guardians,
 config.min_guardians_for_pause,
 );

 Self {
 config: config.clone(),
 timelock: Arc::new(RwLock::new(TimelockController::new(timelock_config))),
 pause: Arc::new(RwLock::new(EmergencyPause::new(guardian_set))),
 proposals: Arc::new(RwLock::new(ProposalRegistry::new())),
 voting: Arc::new(RwLock::new(VotingRegistry::new(
 config.min_voting_period,
 config.quorum_percentage,
 ))),
 }
 }

 /// Check if system is paused
 pub async fn is_paused(&self) -> bool {
 self.pause.read().await.is_paused()
 }

 /// Get timelock controller
 pub fn timelock(&self) -> Arc<RwLock<TimelockController>> {
 self.timelock.clone()
 }

 /// Get emergency pause controller
 pub fn pause(&self) -> Arc<RwLock<EmergencyPause>> {
 self.pause.clone()
 }

 /// Get proposal registry
 pub fn proposals(&self) -> Arc<RwLock<ProposalRegistry>> {
 self.proposals.clone()
 }

 /// Get voting registry
 pub fn voting(&self) -> Arc<RwLock<VotingRegistry>> {
 self.voting.clone()
 }

 /// Get governance configuration
 pub fn config(&self) -> &GovernanceConfig {
 &self.config
 }
}

#[cfg(test)]
mod tests {
 use super::*;

 #[tokio::test]
 async fn test_governance_creation() {
 let guardians = vec![
 "guardian1".to_string(),
 "guardian2".to_string(),
 "guardian3".to_string(),
 "guardian4".to_string(),
 "guardian5".to_string(),
 ];

 let controller = GovernanceController::new(
 GovernanceConfig::default(),
 guardians,
 );

 assert!(!controller.is_paused().await);
 assert_eq!(controller.config().min_guardians_for_pause, 3);
 assert_eq!(controller.config().timelock_delay_secs, 7 * 24 * 60 * 60);
 }
}
