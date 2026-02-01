//! # Voting System
//!
//! Token-weighted voting for governance proposals.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Vote choice
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum VoteChoice {
 /// Vote yes
 Yes,

 /// Vote no
 No,

 /// Abstain from voting
 Abstain,
}

/// Individual vote record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vote {
 /// Voter address
 pub voter: String,

 /// Proposal ID
 pub proposal_id: String,

 /// Vote choice
 pub choice: VoteChoice,

 /// Voting power (OURO balance at snapshot)
 pub voting_power: u64,

 /// Block height when vote was cast
 pub voted_at_block: u64,

 /// Digital signature
 pub signature: String,
}

impl Vote {
 /// Create new vote
 pub fn new(
 voter: String,
 proposal_id: String,
 choice: VoteChoice,
 voting_power: u64,
 voted_at_block: u64,
 signature: String,
 ) -> Self {
 Self {
 voter,
 proposal_id,
 choice,
 voting_power,
 voted_at_block,
 signature,
 }
 }
}

/// Voting power snapshot for a proposal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VotingPowerSnapshot {
 /// Proposal ID
 pub proposal_id: String,

 /// Block height at snapshot
 pub snapshot_block: u64,

 /// Address -> voting power (OURO balance)
 pub balances: HashMap<String, u64>,

 /// Total voting power
 pub total_power: u64,
}

impl VotingPowerSnapshot {
 /// Create new snapshot
 pub fn new(
 proposal_id: String,
 snapshot_block: u64,
 balances: HashMap<String, u64>,
 ) -> Self {
 let total_power = balances.values().sum();

 Self {
 proposal_id,
 snapshot_block,
 balances,
 total_power,
 }
 }

 /// Get voting power for address
 pub fn get_voting_power(&self, address: &str) -> u64 {
 self.balances.get(address).copied().unwrap_or(0)
 }
}

/// Voting power calculator
pub trait VotingPower {
 /// Calculate voting power for address at block height
 fn get_voting_power(&self, address: &str, block_height: u64) -> u64;

 /// Create snapshot of all voting power at block height
 fn create_snapshot(&self, block_height: u64) -> HashMap<String, u64>;
}

/// Voting registry
pub struct VotingRegistry {
 /// Votes by proposal ID -> voter address -> vote
 votes: HashMap<String, HashMap<String, Vote>>,

 /// Voting power snapshots by proposal ID
 snapshots: HashMap<String, VotingPowerSnapshot>,

 /// Minimum voting period in blocks
 min_voting_period: u64,

 /// Quorum percentage (0-100)
 quorum_percentage: u8,
}

impl VotingRegistry {
 /// Create new voting registry
 pub fn new(min_voting_period: u64, quorum_percentage: u8) -> Self {
 assert!(quorum_percentage <= 100, "Quorum must be 0-100");

 Self {
 votes: HashMap::new(),
 snapshots: HashMap::new(),
 min_voting_period,
 quorum_percentage,
 }
 }

 /// Create voting snapshot for proposal
 pub fn create_snapshot(
 &mut self,
 proposal_id: String,
 snapshot_block: u64,
 balances: HashMap<String, u64>,
 ) -> u64 {
 let snapshot = VotingPowerSnapshot::new(
 proposal_id.clone(),
 snapshot_block,
 balances,
 );

 let total_power = snapshot.total_power;
 self.snapshots.insert(proposal_id, snapshot);

 total_power
 }

 /// Cast vote
 pub fn cast_vote(
 &mut self,
 proposal_id: String,
 voter: String,
 choice: VoteChoice,
 current_block: u64,
 signature: String,
 ) -> Result<u64, String> {
 // Get snapshot
 let snapshot = self.snapshots.get(&proposal_id)
 .ok_or_else(|| "Proposal snapshot not found".to_string())?;

 // Get voting power
 let voting_power = snapshot.get_voting_power(&voter);
 if voting_power == 0 {
 return Err("No voting power (zero balance at snapshot)".to_string());
 }

 // Check if already voted
 if let Some(proposal_votes) = self.votes.get(&proposal_id) {
 if proposal_votes.contains_key(&voter) {
 return Err("Already voted on this proposal".to_string());
 }
 }

 // Create vote
 let vote = Vote::new(
 voter.clone(),
 proposal_id.clone(),
 choice,
 voting_power,
 current_block,
 signature,
 );

 // Store vote
 self.votes
 .entry(proposal_id.clone())
 .or_insert_with(HashMap::new)
 .insert(voter.clone(), vote);

 println!(
 " Vote cast on {}: {} ({} voting power)",
 proposal_id,
 match choice {
 VoteChoice::Yes => "YES",
 VoteChoice::No => "NO",
 VoteChoice::Abstain => "ABSTAIN",
 },
 voting_power
 );

 Ok(voting_power)
 }

 /// Get vote for voter on proposal
 pub fn get_vote(&self, proposal_id: &str, voter: &str) -> Option<&Vote> {
 self.votes
 .get(proposal_id)?
 .get(voter)
 }

 /// Get all votes for proposal
 pub fn get_proposal_votes(&self, proposal_id: &str) -> Option<&HashMap<String, Vote>> {
 self.votes.get(proposal_id)
 }

 /// Calculate vote tallies for proposal
 pub fn tally_votes(&self, proposal_id: &str) -> Result<(u64, u64, u64), String> {
 let votes = self.votes.get(proposal_id)
 .ok_or_else(|| "No votes found for proposal".to_string())?;

 let mut yes_votes = 0u64;
 let mut no_votes = 0u64;
 let mut abstain_votes = 0u64;

 for vote in votes.values() {
 match vote.choice {
 VoteChoice::Yes => yes_votes += vote.voting_power,
 VoteChoice::No => no_votes += vote.voting_power,
 VoteChoice::Abstain => abstain_votes += vote.voting_power,
 }
 }

 Ok((yes_votes, no_votes, abstain_votes))
 }

 /// Check if proposal has reached quorum
 pub fn has_quorum(&self, proposal_id: &str) -> Result<bool, String> {
 let snapshot = self.snapshots.get(proposal_id)
 .ok_or_else(|| "Proposal snapshot not found".to_string())?;

 let (yes, no, abstain) = self.tally_votes(proposal_id).unwrap_or((0, 0, 0));
 let total_voted = yes + no + abstain;

 if snapshot.total_power == 0 {
 return Ok(false);
 }

 let participation = (total_voted as f64 / snapshot.total_power as f64) * 100.0;
 Ok(participation >= self.quorum_percentage as f64)
 }

 /// Get voting snapshot for proposal
 pub fn get_snapshot(&self, proposal_id: &str) -> Option<&VotingPowerSnapshot> {
 self.snapshots.get(proposal_id)
 }

 /// Get voter participation rate
 pub fn get_participation_rate(&self, proposal_id: &str) -> Result<f64, String> {
 let snapshot = self.snapshots.get(proposal_id)
 .ok_or_else(|| "Proposal snapshot not found".to_string())?;

 let votes = self.votes.get(proposal_id);
 let num_voters = votes.map(|v| v.len()).unwrap_or(0);
 let total_addresses = snapshot.balances.len();

 if total_addresses == 0 {
 return Ok(0.0);
 }

 Ok((num_voters as f64 / total_addresses as f64) * 100.0)
 }

 /// Set quorum percentage
 pub fn set_quorum(&mut self, percentage: u8) {
 assert!(percentage <= 100, "Quorum must be 0-100");
 self.quorum_percentage = percentage;
 }

 /// Get configuration
 pub fn get_config(&self) -> (u64, u8) {
 (self.min_voting_period, self.quorum_percentage)
 }
}

#[cfg(test)]
mod tests {
 use super::*;

 fn create_test_snapshot() -> HashMap<String, u64> {
 let mut balances = HashMap::new();
 balances.insert("voter1".to_string(), 100_000_000); // 1 OURO
 balances.insert("voter2".to_string(), 500_000_000); // 5 OURO
 balances.insert("voter3".to_string(), 200_000_000); // 2 OURO
 balances.insert("voter4".to_string(), 1_000_000_000); // 10 OURO
 balances
 }

 #[test]
 fn test_create_snapshot() {
 let mut registry = VotingRegistry::new(100_800, 40);

 let balances = create_test_snapshot();
 let total = registry.create_snapshot(
 "PROP-1".to_string(),
 1000,
 balances,
 );

 assert_eq!(total, 1_800_000_000); // 18 OURO total

 let snapshot = registry.get_snapshot("PROP-1").unwrap();
 assert_eq!(snapshot.get_voting_power("voter1"), 100_000_000);
 assert_eq!(snapshot.get_voting_power("voter4"), 1_000_000_000);
 }

 #[test]
 fn test_cast_vote() {
 let mut registry = VotingRegistry::new(100_800, 40);

 let balances = create_test_snapshot();
 registry.create_snapshot("PROP-1".to_string(), 1000, balances);

 // Cast vote
 let voting_power = registry.cast_vote(
 "PROP-1".to_string(),
 "voter1".to_string(),
 VoteChoice::Yes,
 1050,
 "signature1".to_string(),
 ).unwrap();

 assert_eq!(voting_power, 100_000_000);

 // Check vote was recorded
 let vote = registry.get_vote("PROP-1", "voter1").unwrap();
 assert_eq!(vote.choice, VoteChoice::Yes);
 assert_eq!(vote.voting_power, 100_000_000);
 }

 #[test]
 fn test_cannot_vote_twice() {
 let mut registry = VotingRegistry::new(100_800, 40);

 let balances = create_test_snapshot();
 registry.create_snapshot("PROP-1".to_string(), 1000, balances);

 // First vote succeeds
 registry.cast_vote(
 "PROP-1".to_string(),
 "voter1".to_string(),
 VoteChoice::Yes,
 1050,
 "sig1".to_string(),
 ).unwrap();

 // Second vote fails
 let result = registry.cast_vote(
 "PROP-1".to_string(),
 "voter1".to_string(),
 VoteChoice::No,
 1051,
 "sig2".to_string(),
 );

 assert!(result.is_err());
 }

 #[test]
 fn test_tally_votes() {
 let mut registry = VotingRegistry::new(100_800, 40);

 let balances = create_test_snapshot();
 registry.create_snapshot("PROP-1".to_string(), 1000, balances);

 // Cast multiple votes
 registry.cast_vote("PROP-1".to_string(), "voter1".to_string(), VoteChoice::Yes, 1050, "sig1".to_string()).unwrap();
 registry.cast_vote("PROP-1".to_string(), "voter2".to_string(), VoteChoice::Yes, 1051, "sig2".to_string()).unwrap();
 registry.cast_vote("PROP-1".to_string(), "voter3".to_string(), VoteChoice::No, 1052, "sig3".to_string()).unwrap();

 let (yes, no, abstain) = registry.tally_votes("PROP-1").unwrap();

 assert_eq!(yes, 600_000_000); // voter1 (1 OURO) + voter2 (5 OURO)
 assert_eq!(no, 200_000_000); // voter3 (2 OURO)
 assert_eq!(abstain, 0);
 }

 #[test]
 fn test_quorum() {
 let mut registry = VotingRegistry::new(100_800, 40); // 40% quorum

 let balances = create_test_snapshot();
 registry.create_snapshot("PROP-1".to_string(), 1000, balances);

 // Total: 18 OURO, need 40% = 7.2 OURO

 // Vote with 6 OURO (33%) - below quorum
 registry.cast_vote("PROP-1".to_string(), "voter2".to_string(), VoteChoice::Yes, 1050, "sig1".to_string()).unwrap();
 assert!(!registry.has_quorum("PROP-1").unwrap());

 // Vote with 1 more OURO (total 7 OURO, 39%) - still below
 registry.cast_vote("PROP-1".to_string(), "voter1".to_string(), VoteChoice::Yes, 1051, "sig2".to_string()).unwrap();
 assert!(!registry.has_quorum("PROP-1").unwrap());

 // Vote with 2 more OURO (total 9 OURO, 50%) - above quorum!
 registry.cast_vote("PROP-1".to_string(), "voter3".to_string(), VoteChoice::No, 1052, "sig3".to_string()).unwrap();
 assert!(registry.has_quorum("PROP-1").unwrap());
 }

 #[test]
 fn test_no_voting_power() {
 let mut registry = VotingRegistry::new(100_800, 40);

 let balances = create_test_snapshot();
 registry.create_snapshot("PROP-1".to_string(), 1000, balances);

 // Try to vote with address not in snapshot
 let result = registry.cast_vote(
 "PROP-1".to_string(),
 "unknown_voter".to_string(),
 VoteChoice::Yes,
 1050,
 "sig".to_string(),
 );

 assert!(result.is_err());
 }
}
