//! # Proposal System
//!
//! On-chain governance proposals with voting mechanisms.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use chrono::{DateTime, Utc};

/// Type of proposal
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProposalType {
 /// Update protocol parameter
 ParameterChange {
 parameter: String,
 current_value: String,
 new_value: String,
 },

 /// Transfer funds from treasury
 TreasurySpend {
 recipient: String,
 amount: u64,
 purpose: String,
 },

 /// Update validator set
 ValidatorUpdate {
 add: Vec<String>,
 remove: Vec<String>,
 },

 /// Upgrade smart contract
 ContractUpgrade {
 contract_address: String,
 new_code_hash: String,
 },

 /// Change governance rules
 GovernanceChange {
 change_type: String,
 details: String,
 },

 /// General proposal (text only)
 General {
 title: String,
 description: String,
 },
}

/// Proposal status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProposalStatus {
 /// Proposal created, voting period active
 Active,

 /// Voting period ended, passed quorum
 Passed,

 /// Voting period ended, failed quorum or rejected
 Rejected,

 /// Passed proposal executed successfully
 Executed,

 /// Proposal cancelled before completion
 Cancelled,

 /// Execution failed
 ExecutionFailed { reason: String },
}

/// On-chain proposal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
 /// Unique proposal ID
 pub id: String,

 /// Proposal type
 pub proposal_type: ProposalType,

 /// Proposer address
 pub proposer: String,

 /// Proposal description
 pub description: String,

 /// Block height when created
 pub created_at_block: u64,

 /// Block height when voting ends
 pub voting_ends_at_block: u64,

 /// Current status
 pub status: ProposalStatus,

 /// Vote counts
 pub yes_votes: u64,
 pub no_votes: u64,
 pub abstain_votes: u64,

 /// Total voting power at snapshot
 pub total_voting_power: u64,

 /// Execution transaction hash (if executed)
 pub execution_tx: Option<String>,
}

impl Proposal {
 /// Create new proposal
 pub fn new(
 id: String,
 proposal_type: ProposalType,
 proposer: String,
 description: String,
 current_block: u64,
 voting_period_blocks: u64,
 total_voting_power: u64,
 ) -> Self {
 Self {
 id,
 proposal_type,
 proposer,
 description,
 created_at_block: current_block,
 voting_ends_at_block: current_block + voting_period_blocks,
 status: ProposalStatus::Active,
 yes_votes: 0,
 no_votes: 0,
 abstain_votes: 0,
 total_voting_power,
 execution_tx: None,
 }
 }

 /// Check if voting period is active
 pub fn is_active(&self, current_block: u64) -> bool {
 self.status == ProposalStatus::Active && current_block < self.voting_ends_at_block
 }

 /// Check if proposal has ended
 pub fn has_ended(&self, current_block: u64) -> bool {
 current_block >= self.voting_ends_at_block
 }

 /// Calculate total votes cast
 pub fn total_votes(&self) -> u64 {
 self.yes_votes + self.no_votes + self.abstain_votes
 }

 /// Calculate participation rate
 pub fn participation_rate(&self) -> f64 {
 if self.total_voting_power == 0 {
 return 0.0;
 }
 (self.total_votes() as f64 / self.total_voting_power as f64) * 100.0
 }

 /// Check if quorum reached
 pub fn has_quorum(&self, quorum_percentage: u8) -> bool {
 let participation = self.participation_rate();
 participation >= quorum_percentage as f64
 }

 /// Check if proposal passed
 pub fn has_passed(&self, quorum_percentage: u8) -> bool {
 self.has_quorum(quorum_percentage) && self.yes_votes > self.no_votes
 }

 /// Finalize proposal after voting period
 pub fn finalize(&mut self, current_block: u64, quorum_percentage: u8) -> Result<(), String> {
 if !self.has_ended(current_block) {
 return Err("Voting period has not ended".to_string());
 }

 if self.status != ProposalStatus::Active {
 return Err("Proposal is not active".to_string());
 }

 if self.has_passed(quorum_percentage) {
 self.status = ProposalStatus::Passed;
 println!(" Proposal {} passed", self.id);
 } else {
 self.status = ProposalStatus::Rejected;
 println!("ERROR Proposal {} rejected", self.id);
 }

 Ok(())
 }

 /// Mark as executed
 pub fn mark_executed(&mut self, tx_hash: String) {
 self.status = ProposalStatus::Executed;
 self.execution_tx = Some(tx_hash);
 }

 /// Mark execution as failed
 pub fn mark_execution_failed(&mut self, reason: String) {
 self.status = ProposalStatus::ExecutionFailed { reason };
 }
}

/// Proposal registry
pub struct ProposalRegistry {
 proposals: HashMap<String, Proposal>,

 /// Proposal ID counter
 next_id: u64,

 /// Minimum voting period in blocks
 min_voting_period: u64,
}

impl ProposalRegistry {
 /// Create new proposal registry
 pub fn new() -> Self {
 Self {
 proposals: HashMap::new(),
 next_id: 1,
 min_voting_period: 100_800, // ~7 days at 6s/block
 }
 }

 /// Create new proposal
 pub fn create_proposal(
 &mut self,
 proposal_type: ProposalType,
 proposer: String,
 description: String,
 current_block: u64,
 voting_period_blocks: Option<u64>,
 total_voting_power: u64,
 ) -> Result<String, String> {
 // Validate voting period
 let voting_period = voting_period_blocks.unwrap_or(self.min_voting_period);
 if voting_period < self.min_voting_period {
 return Err(format!(
 "Voting period must be at least {} blocks",
 self.min_voting_period
 ));
 }

 // Generate proposal ID
 let proposal_id = format!("PROP-{}", self.next_id);
 self.next_id += 1;

 let proposal = Proposal::new(
 proposal_id.clone(),
 proposal_type,
 proposer,
 description,
 current_block,
 voting_period,
 total_voting_power,
 );

 self.proposals.insert(proposal_id.clone(), proposal);

 println!(" New proposal created: {}", proposal_id);

 Ok(proposal_id)
 }

 /// Get proposal by ID
 pub fn get_proposal(&self, proposal_id: &str) -> Option<&Proposal> {
 self.proposals.get(proposal_id)
 }

 /// Get mutable proposal by ID
 pub fn get_proposal_mut(&mut self, proposal_id: &str) -> Option<&mut Proposal> {
 self.proposals.get_mut(proposal_id)
 }

 /// Get all active proposals
 pub fn get_active_proposals(&self, current_block: u64) -> Vec<&Proposal> {
 self.proposals
 .values()
 .filter(|p| p.is_active(current_block))
 .collect()
 }

 /// Get proposals by status
 pub fn get_proposals_by_status(&self, status: ProposalStatus) -> Vec<&Proposal> {
 self.proposals
 .values()
 .filter(|p| p.status == status)
 .collect()
 }

 /// Finalize ended proposals
 pub fn finalize_ended_proposals(
 &mut self,
 current_block: u64,
 quorum_percentage: u8,
 ) -> Result<Vec<String>, String> {
 let mut finalized = Vec::new();

 for proposal in self.proposals.values_mut() {
 if proposal.status == ProposalStatus::Active && proposal.has_ended(current_block) {
 if let Ok(()) = proposal.finalize(current_block, quorum_percentage) {
 finalized.push(proposal.id.clone());
 }
 }
 }

 Ok(finalized)
 }

 /// Cancel proposal
 pub fn cancel_proposal(
 &mut self,
 proposal_id: &str,
 canceller: &str,
 ) -> Result<(), String> {
 let proposal = self.proposals.get_mut(proposal_id)
 .ok_or_else(|| "Proposal not found".to_string())?;

 // Only proposer can cancel
 if proposal.proposer != canceller {
 return Err("Only proposer can cancel".to_string());
 }

 // Can only cancel active proposals
 if proposal.status != ProposalStatus::Active {
 return Err("Can only cancel active proposals".to_string());
 }

 proposal.status = ProposalStatus::Cancelled;

 println!("ERROR Proposal {} cancelled", proposal_id);

 Ok(())
 }

 /// Set minimum voting period
 pub fn set_min_voting_period(&mut self, blocks: u64) {
 self.min_voting_period = blocks;
 }
}

#[cfg(test)]
mod tests {
 use super::*;

 #[test]
 fn test_create_proposal() {
 let mut registry = ProposalRegistry::new();

 let proposal_id = registry.create_proposal(
 ProposalType::ParameterChange {
 parameter: "max_block_size".to_string(),
 current_value: "1000000".to_string(),
 new_value: "2000000".to_string(),
 },
 "proposer1".to_string(),
 "Increase max block size".to_string(),
 1000,
 None,
 100_000_000, // 100M total voting power
 ).unwrap();

 let proposal = registry.get_proposal(&proposal_id).unwrap();
 assert_eq!(proposal.status, ProposalStatus::Active);
 assert_eq!(proposal.proposer, "proposer1");
 }

 #[test]
 fn test_proposal_voting_period() {
 let proposal = Proposal::new(
 "PROP-1".to_string(),
 ProposalType::General {
 title: "Test".to_string(),
 description: "Test proposal".to_string(),
 },
 "proposer".to_string(),
 "Description".to_string(),
 1000,
 100, // 100 blocks
 1000000,
 );

 assert!(proposal.is_active(1050));
 assert!(proposal.is_active(1099));
 assert!(!proposal.is_active(1100));
 assert!(proposal.has_ended(1100));
 }

 #[test]
 fn test_proposal_quorum() {
 let mut proposal = Proposal::new(
 "PROP-1".to_string(),
 ProposalType::General {
 title: "Test".to_string(),
 description: "Test proposal".to_string(),
 },
 "proposer".to_string(),
 "Description".to_string(),
 1000,
 100,
 1_000_000, // 1M total voting power
 );

 // 400k yes, 100k no = 500k total (50% participation)
 proposal.yes_votes = 400_000;
 proposal.no_votes = 100_000;

 assert!(proposal.has_quorum(40)); // 40% quorum
 assert!(proposal.has_quorum(50)); // 50% quorum
 assert!(!proposal.has_quorum(60)); // 60% quorum fails

 assert!(proposal.has_passed(40)); // Passed with yes > no
 }

 #[test]
 fn test_proposal_finalization() {
 let mut proposal = Proposal::new(
 "PROP-1".to_string(),
 ProposalType::General {
 title: "Test".to_string(),
 description: "Test proposal".to_string(),
 },
 "proposer".to_string(),
 "Description".to_string(),
 1000,
 100,
 1_000_000,
 );

 proposal.yes_votes = 500_000;
 proposal.no_votes = 100_000;

 // Cannot finalize before voting ends
 assert!(proposal.finalize(1050, 40).is_err());

 // Can finalize after voting ends
 proposal.finalize(1100, 40).unwrap();
 assert_eq!(proposal.status, ProposalStatus::Passed);
 }
}
