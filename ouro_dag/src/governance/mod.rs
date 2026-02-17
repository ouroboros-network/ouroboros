//! On-chain governance module
//!
//! Implements proposal submission, voting, and execution for protocol
//! parameter changes. Treasury spending proposals are governed here.
//!
//! Governance parameters:
//! - Proposal threshold: 1000 OURO staked to create proposal
//! - Voting period: 7 days (60480 blocks at 10s/block)
//! - Quorum: 33% of circulating supply must vote
//! - Approval: >50% of votes must be in favor

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Voting period in blocks (7 days at 10s/block)
pub const VOTING_PERIOD_BLOCKS: u64 = 60_480;

/// Minimum stake to create a proposal (1000 OURO in smallest units)
pub const PROPOSAL_THRESHOLD: u64 = 1_000 * 100_000_000;

/// Quorum percentage (33% of circulating supply)
pub const QUORUM_PERCENT: f64 = 0.33;

/// Approval percentage (>50%)
pub const APPROVAL_PERCENT: f64 = 0.50;

/// Types of governance proposals
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProposalType {
    /// Change a protocol parameter (e.g., block size, fee structure)
    ParameterChange {
        parameter: String,
        old_value: String,
        new_value: String,
    },
    /// Spend from the treasury reserve
    TreasurySpend {
        recipient: String,
        amount: u64,
        reason: String,
    },
    /// Upgrade the protocol (soft fork)
    ProtocolUpgrade {
        version: String,
        description: String,
    },
    /// Free-form text proposal (non-binding)
    TextProposal {
        text: String,
    },
}

/// Status of a governance proposal
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProposalStatus {
    /// Voting is active
    Active,
    /// Voting ended, proposal passed
    Passed,
    /// Voting ended, proposal rejected (did not meet quorum or approval)
    Rejected,
    /// Passed proposal has been executed
    Executed,
    /// Proposal was cancelled by the proposer
    Cancelled,
}

/// A governance proposal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    /// Unique proposal ID
    pub id: u64,
    /// Proposer address
    pub proposer: String,
    /// Title of the proposal
    pub title: String,
    /// Detailed description
    pub description: String,
    /// Type-specific proposal data
    pub proposal_type: ProposalType,
    /// Block height when proposal was created
    pub created_at_block: u64,
    /// Block height when voting ends
    pub voting_ends_at_block: u64,
    /// Current status
    pub status: ProposalStatus,
    /// Votes for
    pub votes_for: u64,
    /// Votes against
    pub votes_against: u64,
    /// Votes to abstain
    pub votes_abstain: u64,
    /// Individual vote records (voter -> vote)
    pub voters: HashMap<String, Vote>,
}

/// A single vote on a proposal
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Vote {
    For,
    Against,
    Abstain,
}

/// Governance manager
pub struct GovernanceManager {
    /// All proposals by ID
    proposals: HashMap<u64, Proposal>,
    /// Next proposal ID
    next_id: u64,
}

impl GovernanceManager {
    pub fn new() -> Self {
        Self {
            proposals: HashMap::new(),
            next_id: 1,
        }
    }

    /// Submit a new proposal
    pub fn submit_proposal(
        &mut self,
        proposer: String,
        title: String,
        description: String,
        proposal_type: ProposalType,
        current_block: u64,
    ) -> Result<u64, String> {
        let id = self.next_id;
        self.next_id += 1;

        let proposal = Proposal {
            id,
            proposer,
            title,
            description,
            proposal_type,
            created_at_block: current_block,
            voting_ends_at_block: current_block + VOTING_PERIOD_BLOCKS,
            status: ProposalStatus::Active,
            votes_for: 0,
            votes_against: 0,
            votes_abstain: 0,
            voters: HashMap::new(),
        };

        self.proposals.insert(id, proposal);
        Ok(id)
    }

    /// Cast a vote on a proposal
    pub fn vote(
        &mut self,
        proposal_id: u64,
        voter: String,
        vote: Vote,
        voting_power: u64,
        current_block: u64,
    ) -> Result<(), String> {
        let proposal = self
            .proposals
            .get_mut(&proposal_id)
            .ok_or("Proposal not found")?;

        if proposal.status != ProposalStatus::Active {
            return Err("Proposal is not active".to_string());
        }

        if current_block > proposal.voting_ends_at_block {
            return Err("Voting period has ended".to_string());
        }

        if proposal.voters.contains_key(&voter) {
            return Err("Already voted on this proposal".to_string());
        }

        match &vote {
            Vote::For => proposal.votes_for += voting_power,
            Vote::Against => proposal.votes_against += voting_power,
            Vote::Abstain => proposal.votes_abstain += voting_power,
        }

        proposal.voters.insert(voter, vote);
        Ok(())
    }

    /// Finalize a proposal after voting period ends
    pub fn finalize(
        &mut self,
        proposal_id: u64,
        current_block: u64,
        circulating_supply: u64,
    ) -> Result<ProposalStatus, String> {
        let proposal = self
            .proposals
            .get_mut(&proposal_id)
            .ok_or("Proposal not found")?;

        if proposal.status != ProposalStatus::Active {
            return Err("Proposal is not active".to_string());
        }

        if current_block < proposal.voting_ends_at_block {
            return Err("Voting period has not ended yet".to_string());
        }

        let total_votes = proposal.votes_for + proposal.votes_against + proposal.votes_abstain;
        let quorum_threshold = (circulating_supply as f64 * QUORUM_PERCENT) as u64;

        if total_votes < quorum_threshold {
            proposal.status = ProposalStatus::Rejected;
            return Ok(ProposalStatus::Rejected);
        }

        let approval_votes = proposal.votes_for;
        let total_non_abstain = proposal.votes_for + proposal.votes_against;

        if total_non_abstain > 0 && (approval_votes as f64 / total_non_abstain as f64) > APPROVAL_PERCENT {
            proposal.status = ProposalStatus::Passed;
            Ok(ProposalStatus::Passed)
        } else {
            proposal.status = ProposalStatus::Rejected;
            Ok(ProposalStatus::Rejected)
        }
    }

    /// Get a proposal by ID
    pub fn get_proposal(&self, id: u64) -> Option<&Proposal> {
        self.proposals.get(&id)
    }

    /// List all active proposals
    pub fn active_proposals(&self) -> Vec<&Proposal> {
        self.proposals
            .values()
            .filter(|p| p.status == ProposalStatus::Active)
            .collect()
    }
}

impl Default for GovernanceManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proposal_lifecycle() {
        let mut gov = GovernanceManager::new();

        // Submit proposal
        let id = gov
            .submit_proposal(
                "proposer1".into(),
                "Increase block size".into(),
                "Double the max block size".into(),
                ProposalType::ParameterChange {
                    parameter: "max_block_size".into(),
                    old_value: "1MB".into(),
                    new_value: "2MB".into(),
                },
                100,
            )
            .unwrap();

        assert_eq!(id, 1);
        assert_eq!(gov.active_proposals().len(), 1);

        // Vote
        gov.vote(id, "voter1".into(), Vote::For, 1000, 200).unwrap();
        gov.vote(id, "voter2".into(), Vote::For, 2000, 200).unwrap();
        gov.vote(id, "voter3".into(), Vote::Against, 500, 200).unwrap();

        // Finalize (small circulating supply so quorum is met)
        let status = gov.finalize(id, 100 + VOTING_PERIOD_BLOCKS + 1, 10_000).unwrap();
        assert_eq!(status, ProposalStatus::Passed);
    }

    #[test]
    fn test_double_vote_rejected() {
        let mut gov = GovernanceManager::new();
        let id = gov
            .submit_proposal(
                "proposer1".into(),
                "Test".into(),
                "Test".into(),
                ProposalType::TextProposal { text: "test".into() },
                100,
            )
            .unwrap();

        gov.vote(id, "voter1".into(), Vote::For, 1000, 200).unwrap();
        assert!(gov.vote(id, "voter1".into(), Vote::Against, 1000, 200).is_err());
    }
}
