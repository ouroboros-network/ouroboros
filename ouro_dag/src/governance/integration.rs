//! # Governance Integration
//!
//! Helper functions to integrate governance into Ouroboros.

use super::*;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Integration helper for governance system
pub struct GovernanceIntegration {
 controller: Arc<RwLock<GovernanceController>>,
}

impl GovernanceIntegration {
 /// Create new governance integration
 pub fn new(controller: Arc<RwLock<GovernanceController>>) -> Self {
 Self { controller }
 }

 /// Check if operations are paused (call before critical operations)
 pub async fn check_not_paused(&self) -> Result<(), String> {
 let controller = self.controller.read().await;
 if controller.is_paused().await {
 let pause_state = controller.pause().read().await;
 let state = pause_state.get_state();

 let reason = state.reason.as_ref()
 .map(|r| r.description())
 .unwrap_or_else(|| "Unknown reason".to_string());

 return Err(format!(
 "System is paused due to emergency: {}",
 reason
 ));
 }
 Ok(())
 }

 /// Execute timelocked operation after approval
 pub async fn execute_timelock_operation(
 &self,
 operation_id: &str,
 ) -> Result<OperationType, String> {
 let controller = self.controller.read().await;
 let mut timelock = controller.timelock().write().await;

 timelock.execute(operation_id)
 }

 /// Create and schedule a governance proposal
 pub async fn create_proposal(
 &self,
 proposal_type: ProposalType,
 proposer: String,
 description: String,
 current_block: u64,
 total_voting_power: u64,
 ) -> Result<String, String> {
 let controller = self.controller.read().await;
 let mut proposals = controller.proposals().write().await;

 proposals.create_proposal(
 proposal_type,
 proposer,
 description,
 current_block,
 None, // Use default voting period
 total_voting_power,
 )
 }

 /// Cast vote on proposal
 pub async fn vote_on_proposal(
 &self,
 proposal_id: String,
 voter: String,
 choice: VoteChoice,
 current_block: u64,
 signature: String,
 ) -> Result<u64, String> {
 let controller = self.controller.read().await;
 let mut voting = controller.voting().write().await;

 voting.cast_vote(proposal_id, voter, choice, current_block, signature)
 }

 /// Finalize proposal and update vote counts
 pub async fn finalize_proposal(
 &self,
 proposal_id: &str,
 current_block: u64,
 ) -> Result<ProposalStatus, String> {
 let controller = self.controller.read().await;
 let quorum = controller.config().quorum_percentage;

 // Get vote tallies
 let (yes_votes, no_votes, abstain_votes) = {
 let voting = controller.voting().read().await;
 voting.tally_votes(proposal_id)?
 };

 // Update proposal with vote counts
 let mut proposals = controller.proposals().write().await;
 let proposal = proposals.get_proposal_mut(proposal_id)
 .ok_or_else(|| "Proposal not found".to_string())?;

 proposal.yes_votes = yes_votes;
 proposal.no_votes = no_votes;
 proposal.abstain_votes = abstain_votes;

 // Finalize
 proposal.finalize(current_block, quorum)?;

 Ok(proposal.status.clone())
 }

 /// Execute passed proposal
 pub async fn execute_proposal(
 &self,
 proposal_id: &str,
 ) -> Result<ProposalType, String> {
 let controller = self.controller.read().await;
 let mut proposals = controller.proposals().write().await;

 let proposal = proposals.get_proposal(proposal_id)
 .ok_or_else(|| "Proposal not found".to_string())?;

 if proposal.status != ProposalStatus::Passed {
 return Err(format!("Proposal is {:?}, cannot execute", proposal.status));
 }

 let proposal_type = proposal.proposal_type.clone();

 // Mark as executed (caller should provide tx hash later)
 let proposal = proposals.get_proposal_mut(proposal_id).unwrap();
 proposal.status = ProposalStatus::Executed;
 proposal.execution_tx = Some("pending".to_string());

 println!(" Executing proposal {}", proposal_id);

 Ok(proposal_type)
 }

 /// Guardian votes to pause system
 pub async fn guardian_pause_vote(
 &self,
 guardian: &str,
 signature: &str,
 reason: PauseReason,
 ) -> Result<bool, String> {
 let controller = self.controller.read().await;
 let mut pause = controller.pause().write().await;

 pause.vote_pause(guardian, signature, reason)
 }

 /// Guardian votes to unpause system
 pub async fn guardian_unpause_vote(
 &self,
 guardian: &str,
 signature: &str,
 resolution: String,
 ) -> Result<bool, String> {
 let controller = self.controller.read().await;
 let mut pause = controller.pause().write().await;

 pause.vote_unpause(guardian, signature, resolution)
 }

 /// Get all active proposals
 pub async fn get_active_proposals(&self, current_block: u64) -> Vec<Proposal> {
 let controller = self.controller.read().await;
 let proposals = controller.proposals().read().await;

 proposals.get_active_proposals(current_block)
 .into_iter()
 .cloned()
 .collect()
 }

 /// Get proposal by ID
 pub async fn get_proposal(&self, proposal_id: &str) -> Option<Proposal> {
 let controller = self.controller.read().await;
 let proposals = controller.proposals().read().await;

 proposals.get_proposal(proposal_id).cloned()
 }
}

/// Background task to finalize ended proposals
pub async fn finalize_ended_proposals_task(
 controller: Arc<RwLock<GovernanceController>>,
 mut current_block_rx: tokio::sync::watch::Receiver<u64>,
) {
 println!(" Started proposal finalization task");

 loop {
 // Wait for block update
 if current_block_rx.changed().await.is_err() {
 break;
 }

 let current_block = *current_block_rx.borrow();

 // Finalize any ended proposals
 let controller_lock = controller.read().await;
 let quorum = controller_lock.config().quorum_percentage;
 let mut proposals = controller_lock.proposals().write().await;
 let voting = controller_lock.voting().read().await;

 if let Ok(finalized) = proposals.finalize_ended_proposals(current_block, quorum) {
 for proposal_id in finalized {
 // Update vote counts
 if let Ok((yes, no, abstain)) = voting.tally_votes(&proposal_id) {
 if let Some(proposal) = proposals.get_proposal_mut(&proposal_id) {
 proposal.yes_votes = yes;
 proposal.no_votes = no;
 proposal.abstain_votes = abstain;

 println!(
 "STATS Finalized proposal {}: {:?} (Y:{} N:{} A:{})",
 proposal_id,
 proposal.status,
 yes,
 no,
 abstain
 );
 }
 }
 }
 }

 drop(proposals);
 drop(voting);
 drop(controller_lock);

 // Check every 10 blocks
 tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
 }
}

#[cfg(test)]
mod tests {
 use super::*;

 fn create_test_controller() -> Arc<RwLock<GovernanceController>> {
 let guardians = vec![
 "guardian1".to_string(),
 "guardian2".to_string(),
 "guardian3".to_string(),
 "guardian4".to_string(),
 "guardian5".to_string(),
 ];

 Arc::new(RwLock::new(GovernanceController::new(
 GovernanceConfig::default(),
 guardians,
 )))
 }

 #[tokio::test]
 async fn test_check_not_paused() {
 let controller = create_test_controller();
 let integration = GovernanceIntegration::new(controller.clone());

 // Should not be paused initially
 assert!(integration.check_not_paused().await.is_ok());

 // Activate pause
 {
 let ctrl = controller.read().await;
 let mut pause = ctrl.pause().write().await;

 let reason = PauseReason::SecurityVulnerability {
 description: "Test vulnerability".to_string(),
 };

 pause.vote_pause("guardian1", "sig1", reason.clone()).unwrap();
 pause.vote_pause("guardian2", "sig2", reason.clone()).unwrap();
 pause.vote_pause("guardian3", "sig3", reason).unwrap();
 }

 // Should be paused now
 assert!(integration.check_not_paused().await.is_err());
 }

 #[tokio::test]
 async fn test_create_and_vote_proposal() {
 let controller = create_test_controller();
 let integration = GovernanceIntegration::new(controller.clone());

 // Create proposal
 let proposal_id = integration.create_proposal(
 ProposalType::ParameterChange {
 parameter: "max_block_size".to_string(),
 current_value: "1000000".to_string(),
 new_value: "2000000".to_string(),
 },
 "proposer1".to_string(),
 "Increase max block size".to_string(),
 1000,
 1_000_000_000, // 1B total voting power
 ).await.unwrap();

 // Create voting snapshot
 {
 let ctrl = controller.read().await;
 let mut voting = ctrl.voting().write().await;

 let mut balances = std::collections::HashMap::new();
 balances.insert("voter1".to_string(), 500_000_000);
 balances.insert("voter2".to_string(), 500_000_000);

 voting.create_snapshot(proposal_id.clone(), 1000, balances);
 }

 // Vote
 let voting_power = integration.vote_on_proposal(
 proposal_id.clone(),
 "voter1".to_string(),
 VoteChoice::Yes,
 1050,
 "sig1".to_string(),
 ).await.unwrap();

 assert_eq!(voting_power, 500_000_000);

 // Check proposal exists
 let proposal = integration.get_proposal(&proposal_id).await;
 assert!(proposal.is_some());
 }
}
