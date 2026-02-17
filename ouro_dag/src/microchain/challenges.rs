//! Challenge Mechanism for Microchains
//!
//! Implements fraud detection and challenge system for microchain
//! state anchors. Allows users to challenge invalid state transitions
//! and force exit from compromised microchains.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Challenge period for microchain anchors (7 days)
pub const MICROCHAIN_CHALLENGE_PERIOD_SECS: u64 = 7 * 24 * 60 * 60;

/// Bond required to submit challenge: 50 OURO
/// Low enough for regular users, high enough to prevent spam
pub const CHALLENGE_BOND_AMOUNT: u64 = 5_000_000_000; // 50 OURO

/// Microchain state anchor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateAnchor {
    pub microchain_id: String,
    pub state_root: [u8; 32],
    pub block_height: u64,
    pub timestamp: u64,
    pub operator_signature: Vec<u8>,
    pub status: AnchorStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AnchorStatus {
    Pending,
    Confirmed,
    Challenged,
    Finalized,
    Slashed,
}

/// Challenge submitted against state anchor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Challenge {
    pub challenge_id: String,
    pub anchor_hash: [u8; 32],
    pub challenger: String,
    pub challenge_type: ChallengeType,
    pub evidence: ChallengeEvidence,
    pub bond: u64,
    pub timestamp: u64,
    pub status: ChallengeStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ChallengeStatus {
    Pending,
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChallengeType {
    /// Invalid state transition
    InvalidStateTransition,
    /// Unauthorized transaction included
    UnauthorizedTransaction,
    /// Double spend detected
    DoubleSpend,
    /// Invalid operator signature
    InvalidSignature,
    /// State root mismatch
    StateRootMismatch,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeEvidence {
    /// Previous state root
    pub previous_state_root: [u8; 32],
    /// Claimed new state root
    pub claimed_state_root: [u8; 32],
    /// Transactions in block
    pub transactions: Vec<MicrochainTransaction>,
    /// Merkle proofs
    pub merkle_proofs: Vec<Vec<[u8; 32]>>,
    /// Additional evidence data
    pub additional_data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MicrochainTransaction {
    pub from: String,
    pub to: String,
    pub amount: u64,
    pub nonce: u64,
    pub signature: Vec<u8>,
}

/// Force exit request from microchain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForceExitRequest {
    pub exit_id: String,
    pub microchain_id: String,
    pub user: String,
    pub amount: u64,
    pub nonce: u64,
    pub merkle_proof: Vec<[u8; 32]>,
    pub state_root: [u8; 32],
    pub timestamp: u64,
    pub status: ExitStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ExitStatus {
    Pending,
    Completed,
    Rejected,
}

/// Challenge manager for microchains
pub struct ChallengeManager {
    /// Pending state anchors
    pending_anchors: Arc<RwLock<HashMap<[u8; 32], StateAnchor>>>,
    /// Active challenges
    challenges: Arc<RwLock<HashMap<String, Challenge>>>,
    /// Force exit requests
    exit_requests: Arc<RwLock<HashMap<String, ForceExitRequest>>>,
    /// Challenge bonds
    challenge_bonds: Arc<RwLock<HashMap<String, u64>>>,
    /// Operator stakes
    operator_stakes: Arc<RwLock<HashMap<String, u64>>>,
}

impl ChallengeManager {
    /// Create new challenge manager
    pub fn new() -> Self {
        Self {
            pending_anchors: Arc::new(RwLock::new(HashMap::new())),
            challenges: Arc::new(RwLock::new(HashMap::new())),
            exit_requests: Arc::new(RwLock::new(HashMap::new())),
            challenge_bonds: Arc::new(RwLock::new(HashMap::new())),
            operator_stakes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Submit state anchor (by microchain operator)
    pub fn submit_anchor(
        &self,
        microchain_id: String,
        state_root: [u8; 32],
        block_height: u64,
        operator_signature: Vec<u8>,
        current_time: u64,
    ) -> Result<[u8; 32], String> {
        let anchor = StateAnchor {
            microchain_id: microchain_id.clone(),
            state_root,
            block_height,
            timestamp: current_time,
            operator_signature,
            status: AnchorStatus::Pending,
        };

        let anchor_hash = self.compute_anchor_hash(&anchor);

        let mut anchors = self
            .pending_anchors
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        anchors.insert(anchor_hash, anchor);

        println!(" State anchor submitted for microchain: {}", microchain_id);
        println!(" Block height: {}", block_height);
        println!(" State root: {:?}", hex::encode(state_root));
        println!(
            " Challenge period: {} days",
            MICROCHAIN_CHALLENGE_PERIOD_SECS / 86400
        );

        Ok(anchor_hash)
    }

    /// Submit challenge against state anchor
    pub fn submit_challenge(
        &self,
        anchor_hash: [u8; 32],
        challenger: String,
        challenge_type: ChallengeType,
        evidence: ChallengeEvidence,
        current_time: u64,
    ) -> Result<String, String> {
        // Verify anchor exists
        let anchors = self
            .pending_anchors
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let anchor = anchors.get(&anchor_hash).ok_or("Anchor not found")?;

        // Check if still in challenge period
        if current_time > anchor.timestamp + MICROCHAIN_CHALLENGE_PERIOD_SECS {
            return Err("Challenge period expired".to_string());
        }

        // Check if already finalized
        if anchor.status == AnchorStatus::Finalized {
            return Err("Anchor already finalized".to_string());
        }
        drop(anchors);

        // Verify challenger has sufficient bond
        let bonds = self
            .challenge_bonds
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let challenger_bond = *bonds.get(&challenger).unwrap_or(&0);
        if challenger_bond < CHALLENGE_BOND_AMOUNT {
            return Err(format!(
                "Insufficient challenge bond: {} < {}",
                challenger_bond, CHALLENGE_BOND_AMOUNT
            ));
        }
        drop(bonds);

        let challenge_id = format!("challenge_{}_{}", hex::encode(anchor_hash), current_time);

        let challenge = Challenge {
            challenge_id: challenge_id.clone(),
            anchor_hash,
            challenger: challenger.clone(),
            challenge_type,
            evidence,
            bond: CHALLENGE_BOND_AMOUNT,
            timestamp: current_time,
            status: ChallengeStatus::Pending,
        };

        let mut challenges = self
            .challenges
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        challenges.insert(challenge_id.clone(), challenge);

        // Update anchor status
        let mut anchors = self
            .pending_anchors
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(anchor) = anchors.get_mut(&anchor_hash) {
            anchor.status = AnchorStatus::Challenged;
        }

        println!(
            "WARNING Challenge submitted against anchor: {:?}",
            hex::encode(anchor_hash)
        );
        println!(" Challenger: {}", challenger);
        println!(" Challenge ID: {}", challenge_id);

        Ok(challenge_id)
    }

    /// Verify challenge and determine outcome
    pub fn verify_challenge(
        &self,
        challenge_id: &str,
        microchain_state: &HashMap<String, u64>,
    ) -> Result<bool, String> {
        let mut challenges = self
            .challenges
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let challenge = challenges
            .get_mut(challenge_id)
            .ok_or("Challenge not found")?
            .clone();

        let is_valid = match challenge.challenge_type {
            ChallengeType::InvalidStateTransition => {
                self.verify_state_transition(&challenge.evidence)
            }
            ChallengeType::DoubleSpend => self.verify_double_spend(&challenge.evidence),
            ChallengeType::StateRootMismatch => {
                self.verify_state_root(&challenge.evidence, microchain_state)
            }
            ChallengeType::InvalidSignature => self.verify_signatures(&challenge.evidence),
            ChallengeType::UnauthorizedTransaction => {
                self.verify_authorization(&challenge.evidence)
            }
        };

        let mut anchors = self
            .pending_anchors
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let anchor = anchors
            .get_mut(&challenge.anchor_hash)
            .ok_or("Anchor not found")?;

        if is_valid {
            // Challenge accepted - slash operator
            anchor.status = AnchorStatus::Slashed;

            if let Some(challenge) = challenges.get_mut(challenge_id) {
                challenge.status = ChallengeStatus::Accepted;
            }

            // Slash operator stake and reward challenger
            self.slash_operator(&anchor.microchain_id, &challenge.challenger)?;

            println!("FAST Challenge ACCEPTED! Operator slashed");
            println!(" Challenge ID: {}", challenge_id);
        } else {
            // Challenge rejected - slash challenger bond
            anchor.status = AnchorStatus::Pending;

            if let Some(challenge) = challenges.get_mut(challenge_id) {
                challenge.status = ChallengeStatus::Rejected;
            }

            self.slash_challenger(&challenge.challenger)?;

            println!("ERROR Challenge REJECTED! Challenger bond slashed");
            println!(" Challenge ID: {}", challenge_id);
        }

        Ok(is_valid)
    }

    /// Finalize anchor after challenge period
    pub fn finalize_anchor(&self, anchor_hash: [u8; 32], current_time: u64) -> Result<(), String> {
        let mut anchors = self
            .pending_anchors
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let anchor = anchors.get_mut(&anchor_hash).ok_or("Anchor not found")?;

        // Check if challenge period expired
        if current_time < anchor.timestamp + MICROCHAIN_CHALLENGE_PERIOD_SECS {
            return Err("Challenge period not expired".to_string());
        }

        // Check if not challenged or slashed
        if anchor.status == AnchorStatus::Challenged {
            return Err("Anchor is being challenged".to_string());
        }

        if anchor.status == AnchorStatus::Slashed {
            return Err("Anchor was slashed".to_string());
        }

        anchor.status = AnchorStatus::Finalized;

        println!(" Anchor finalized: {:?}", hex::encode(anchor_hash));
        println!(" Microchain: {}", anchor.microchain_id);

        Ok(())
    }

    /// Request force exit from microchain
    pub fn request_force_exit(
        &self,
        microchain_id: String,
        user: String,
        amount: u64,
        nonce: u64,
        merkle_proof: Vec<[u8; 32]>,
        state_root: [u8; 32],
        current_time: u64,
    ) -> Result<String, String> {
        let exit_id = format!("exit_{}_{}", microchain_id, current_time);

        let exit_request = ForceExitRequest {
            exit_id: exit_id.clone(),
            microchain_id: microchain_id.clone(),
            user: user.clone(),
            amount,
            nonce,
            merkle_proof,
            state_root,
            timestamp: current_time,
            status: ExitStatus::Pending,
        };

        let mut exits = self
            .exit_requests
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        exits.insert(exit_id.clone(), exit_request);

        println!(" Force exit requested by {}", user);
        println!(" Microchain: {}", microchain_id);
        println!(" Amount: {} OURO", amount / 100_000_000);

        Ok(exit_id)
    }

    /// Process force exit (verify and execute)
    pub fn process_force_exit(&self, exit_id: &str, current_time: u64) -> Result<u64, String> {
        let mut exits = self
            .exit_requests
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let exit = exits.get_mut(exit_id).ok_or("Exit request not found")?;

        // Verify merkle proof
        let is_valid = self.verify_exit_merkle_proof(
            &exit.user,
            exit.amount,
            exit.nonce,
            &exit.merkle_proof,
            exit.state_root,
        );

        if !is_valid {
            exit.status = ExitStatus::Rejected;
            return Err("Invalid merkle proof".to_string());
        }

        exit.status = ExitStatus::Completed;

        println!(" Force exit completed for {}", exit.user);
        println!(" Amount withdrawn: {} OURO", exit.amount / 100_000_000);

        Ok(exit.amount)
    }

    /// Deposit challenge bond
    pub fn deposit_challenge_bond(&self, user: String, amount: u64) {
        let mut bonds = self
            .challenge_bonds
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let current = bonds.get(&user).copied().unwrap_or(0);
        bonds.insert(user.clone(), current + amount);

        println!(
            "REWARD Challenge bond deposited by {}: {} OURO",
            user,
            amount / 100_000_000
        );
    }

    /// Deposit operator stake
    pub fn deposit_operator_stake(&self, operator: String, amount: u64) {
        let mut stakes = self
            .operator_stakes
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let current = stakes.get(&operator).copied().unwrap_or(0);
        stakes.insert(operator.clone(), current + amount);

        println!(
            "REWARD Operator stake deposited by {}: {} OURO",
            operator,
            amount / 100_000_000
        );
    }

    // Private helper methods

    fn compute_anchor_hash(&self, anchor: &StateAnchor) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(anchor.microchain_id.as_bytes());
        hasher.update(&anchor.state_root);
        hasher.update(&anchor.block_height.to_le_bytes());
        hasher.update(&anchor.timestamp.to_le_bytes());
        hasher.finalize().into()
    }

    fn verify_state_transition(&self, _evidence: &ChallengeEvidence) -> bool {
        // Simplified: In production, recompute state root from transactions
        // and verify it matches the claimed state root
        false // Assume invalid for demo
    }

    fn verify_double_spend(&self, evidence: &ChallengeEvidence) -> bool {
        // Check for duplicate nonces
        let mut nonces = HashMap::new();
        for tx in &evidence.transactions {
            if nonces.contains_key(&(tx.from.clone(), tx.nonce)) {
                return true; // Double spend detected
            }
            nonces.insert((tx.from.clone(), tx.nonce), true);
        }
        false
    }

    fn verify_state_root(
        &self,
        evidence: &ChallengeEvidence,
        _state: &HashMap<String, u64>,
    ) -> bool {
        // Simplified: Verify claimed state root matches computed root
        evidence.claimed_state_root != evidence.previous_state_root
    }

    fn verify_signatures(&self, _evidence: &ChallengeEvidence) -> bool {
        // Simplified: In production, verify Ed25519 signatures
        false
    }

    fn verify_authorization(&self, _evidence: &ChallengeEvidence) -> bool {
        // Simplified: Check if all transactions are authorized
        false
    }

    fn verify_exit_merkle_proof(
        &self,
        _user: &str,
        _amount: u64,
        _nonce: u64,
        proof: &[[u8; 32]],
        state_root: [u8; 32],
    ) -> bool {
        // Simplified merkle proof verification
        !proof.is_empty() && state_root != [0; 32]
    }

    fn slash_operator(&self, microchain_id: &str, challenger: &str) -> Result<(), String> {
        let mut stakes = self
            .operator_stakes
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let operator_stake = stakes.get(microchain_id).copied().unwrap_or(0);

        if operator_stake == 0 {
            return Err("No operator stake to slash".to_string());
        }

        let slash_amount = operator_stake / 2; // Slash 50%

        // Slash operator
        stakes.insert(microchain_id.to_string(), operator_stake - slash_amount);

        // Reward challenger
        let mut bonds = self
            .challenge_bonds
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let challenger_balance = bonds.get(challenger).copied().unwrap_or(0);
        bonds.insert(challenger.to_string(), challenger_balance + slash_amount);

        println!("FAST Operator slashed: {} OURO", slash_amount / 100_000_000);
        println!(" Challenger rewarded: {} OURO", slash_amount / 100_000_000);

        Ok(())
    }

    fn slash_challenger(&self, challenger: &str) -> Result<(), String> {
        let mut bonds = self
            .challenge_bonds
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let challenger_bond = bonds.get(challenger).copied().unwrap_or(0);

        if challenger_bond < CHALLENGE_BOND_AMOUNT {
            return Err("Insufficient bond to slash".to_string());
        }

        bonds.insert(
            challenger.to_string(),
            challenger_bond - CHALLENGE_BOND_AMOUNT,
        );

        println!(
            "FAST Challenger bond slashed: {} OURO",
            CHALLENGE_BOND_AMOUNT / 100_000_000
        );

        Ok(())
    }

    /// Get anchor status
    pub fn get_anchor_status(&self, anchor_hash: [u8; 32]) -> Option<AnchorStatus> {
        let anchors = self
            .pending_anchors
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        anchors.get(&anchor_hash).map(|a| a.status.clone())
    }

    /// Get challenge status
    pub fn get_challenge_status(&self, challenge_id: &str) -> Option<ChallengeStatus> {
        let challenges = self
            .challenges
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        challenges.get(challenge_id).map(|c| c.status.clone())
    }
}

impl Default for ChallengeManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_submit_anchor() {
        let manager = ChallengeManager::new();

        let result = manager.submit_anchor(
            "microchain_1".to_string(),
            [1u8; 32],
            100,
            vec![0u8; 64],
            1000,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_challenge_submission() {
        let manager = ChallengeManager::new();

        // Submit anchor
        let anchor_hash = manager
            .submit_anchor(
                "microchain_1".to_string(),
                [1u8; 32],
                100,
                vec![0u8; 64],
                1000,
            )
            .unwrap();

        // Deposit challenge bond (50 OURO = 5,000,000,000 units)
        manager.deposit_challenge_bond("challenger1".to_string(), CHALLENGE_BOND_AMOUNT);

        // Submit challenge
        let evidence = ChallengeEvidence {
            previous_state_root: [0u8; 32],
            claimed_state_root: [1u8; 32],
            transactions: vec![],
            merkle_proofs: vec![],
            additional_data: vec![],
        };

        let result = manager.submit_challenge(
            anchor_hash,
            "challenger1".to_string(),
            ChallengeType::StateRootMismatch,
            evidence,
            1500,
        );

        assert!(result.is_ok());
        assert_eq!(
            manager.get_anchor_status(anchor_hash),
            Some(AnchorStatus::Challenged)
        );
    }

    #[test]
    fn test_force_exit() {
        let manager = ChallengeManager::new();

        let result = manager.request_force_exit(
            "microchain_1".to_string(),
            "user1".to_string(),
            5_000_000_000, // 50 OURO
            1,
            vec![[1u8; 32], [2u8; 32]],
            [3u8; 32],
            2000,
        );

        assert!(result.is_ok());

        let exit_id = result.unwrap();
        let process_result = manager.process_force_exit(&exit_id, 2100);

        assert!(process_result.is_ok());
    }
}
