//! Fraud Proof System for Cross-Chain Transfers
//!
//! Implements fraud detection and proof mechanisms for optimistic
//! cross-chain message relaying. Allows anyone to challenge fraudulent
//! relays within the challenge period.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use sha2::{Sha256, Digest};

/// Challenge period in seconds (10 minutes)
pub const CHALLENGE_PERIOD_SECS: u64 = 600;

/// Bond required for relayers: 2,500 OURO
/// Cross-chain security requires significant collateral
pub const RELAYER_BOND_AMOUNT: u64 = 250_000_000_000; // 2,500 OURO

/// Reward for successful fraud proof: 250 OURO (10% of relayer bond)
/// Big bounty incentivizes watchtower nodes
pub const FRAUD_PROOF_REWARD: u64 = 25_000_000_000; // 250 OURO

/// Cross-chain message being relayed
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CrossChainMessage {
 pub source_subchain: String,
 pub destination_subchain: String,
 pub sender: String,
 pub recipient: String,
 pub amount: u64,
 pub nonce: u64,
 pub timestamp: u64,
}

impl CrossChainMessage {
 /// Calculate message hash for verification
 pub fn hash(&self) -> [u8; 32] {
 let mut hasher = Sha256::new();
 hasher.update(self.source_subchain.as_bytes());
 hasher.update(self.destination_subchain.as_bytes());
 hasher.update(self.sender.as_bytes());
 hasher.update(self.recipient.as_bytes());
 hasher.update(&self.amount.to_le_bytes());
 hasher.update(&self.nonce.to_le_bytes());
 hasher.update(&self.timestamp.to_le_bytes());
 hasher.finalize().into()
 }
}

/// Relayed message with bond
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayedMessage {
 pub message: CrossChainMessage,
 pub relayer: String,
 pub bond: u64,
 pub relay_timestamp: u64,
 pub status: RelayStatus,
 pub merkle_proof: Option<Vec<[u8; 32]>>,
}

/// Status of relayed message
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RelayStatus {
 Pending,
 Confirmed,
 Challenged,
 Slashed,
}

/// Fraud proof submitted by challenger
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FraudProof {
 pub message_hash: [u8; 32],
 pub challenger: String,
 pub proof_type: FraudProofType,
 pub evidence: Vec<u8>,
 pub timestamp: u64,
}

/// Types of fraud that can be proven
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FraudProofType {
 /// Message never existed on source chain
 MessageNotFound,
 /// Invalid merkle proof
 InvalidMerkleProof,
 /// Message already spent (double relay)
 DoubleRelay,
 /// Insufficient balance on source
 InsufficientBalance,
}

/// Fraud proof system manager
pub struct FraudProofManager {
 /// Pending relayed messages
 pending_relays: Arc<RwLock<HashMap<[u8; 32], RelayedMessage>>>,
 /// Submitted fraud proofs
 fraud_proofs: Arc<RwLock<HashMap<[u8; 32], Vec<FraudProof>>>>,
 /// Relayer bonds
 relayer_bonds: Arc<RwLock<HashMap<String, u64>>>,
 /// Slashed relayers
 slashed_relayers: Arc<RwLock<HashMap<String, u64>>>,
}

impl FraudProofManager {
 /// Create new fraud proof manager
 pub fn new() -> Self {
 Self {
 pending_relays: Arc::new(RwLock::new(HashMap::new())),
 fraud_proofs: Arc::new(RwLock::new(HashMap::new())),
 relayer_bonds: Arc::new(RwLock::new(HashMap::new())),
 slashed_relayers: Arc::new(RwLock::new(HashMap::new())),
 }
 }

 /// Submit a relayed message (optimistic execution)
 pub fn submit_relay(
 &self,
 message: CrossChainMessage,
 relayer: String,
 merkle_proof: Option<Vec<[u8; 32]>>,
 current_time: u64,
 ) -> Result<[u8; 32], String> {
 // Verify relayer has sufficient bond
 let bonds = self.relayer_bonds.read().unwrap_or_else(|poisoned| poisoned.into_inner());
 let relayer_bond = *bonds.get(&relayer).unwrap_or(&0);

 if relayer_bond < RELAYER_BOND_AMOUNT {
 return Err(format!(
 "Insufficient bond: {} < {}",
 relayer_bond, RELAYER_BOND_AMOUNT
 ));
 }
 drop(bonds);

 let message_hash = message.hash();

 let relayed = RelayedMessage {
 message,
 relayer: relayer.clone(),
 bond: RELAYER_BOND_AMOUNT,
 relay_timestamp: current_time,
 status: RelayStatus::Pending,
 merkle_proof,
 };

 let mut pending = self.pending_relays.write().unwrap_or_else(|poisoned| poisoned.into_inner());
 pending.insert(message_hash, relayed);

 println!(" Relay submitted by {}: {:?}", relayer, hex::encode(message_hash));
 println!(" Challenge period: {} seconds", CHALLENGE_PERIOD_SECS);

 Ok(message_hash)
 }

 /// Submit a fraud proof
 pub fn submit_fraud_proof(
 &self,
 message_hash: [u8; 32],
 challenger: String,
 proof_type: FraudProofType,
 evidence: Vec<u8>,
 current_time: u64,
 ) -> Result<(), String> {
 // Check if message exists
 let pending = self.pending_relays.read().unwrap_or_else(|poisoned| poisoned.into_inner());
 let relay = pending.get(&message_hash)
 .ok_or("Message not found")?;

 // Check if still in challenge period
 if current_time > relay.relay_timestamp + CHALLENGE_PERIOD_SECS {
 return Err("Challenge period expired".to_string());
 }

 // Check if already slashed
 if relay.status == RelayStatus::Slashed {
 return Err("Relay already slashed".to_string());
 }
 drop(pending);

 let fraud_proof = FraudProof {
 message_hash,
 challenger: challenger.clone(),
 proof_type,
 evidence,
 timestamp: current_time,
 };

 let mut proofs = self.fraud_proofs.write().unwrap_or_else(|poisoned| poisoned.into_inner());
 proofs.entry(message_hash)
 .or_insert_with(Vec::new)
 .push(fraud_proof);

 println!("WARNING Fraud proof submitted by {}", challenger);
 println!(" Message: {:?}", hex::encode(message_hash));

 Ok(())
 }

 /// Verify fraud proof and slash relayer if valid
 pub fn verify_and_slash(
 &self,
 message_hash: [u8; 32],
 source_chain_state: &HashMap<String, u64>,
 source_chain_messages: &HashMap<u64, CrossChainMessage>,
 ) -> Result<bool, String> {
 let proofs = self.fraud_proofs.read().unwrap_or_else(|poisoned| poisoned.into_inner());
 let fraud_proofs = proofs.get(&message_hash)
 .ok_or("No fraud proofs found")?;

 if fraud_proofs.is_empty() {
 return Ok(false);
 }

 let pending = self.pending_relays.read().unwrap_or_else(|poisoned| poisoned.into_inner());
 let relay = pending.get(&message_hash)
 .ok_or("Relay not found")?
 .clone();
 drop(pending);

 // Verify each fraud proof
 for proof in fraud_proofs {
 let is_valid = match &proof.proof_type {
 FraudProofType::MessageNotFound => {
 // Check if message exists in source chain
 !source_chain_messages.contains_key(&relay.message.nonce)
 }
 FraudProofType::InsufficientBalance => {
 // Check if sender had sufficient balance
 let balance = source_chain_state
 .get(&relay.message.sender)
 .unwrap_or(&0);
 *balance < relay.message.amount
 }
 FraudProofType::InvalidMerkleProof => {
 // Verify merkle proof
 if let Some(merkle_proof) = &relay.merkle_proof {
 !self.verify_merkle_proof(
 message_hash,
 merkle_proof,
 &proof.evidence
 )
 } else {
 true // No proof provided
 }
 }
 FraudProofType::DoubleRelay => {
 // Check for duplicate relay
 let evidence_hash: [u8; 32] = proof.evidence[0..32]
 .try_into()
 .unwrap_or([0; 32]);
 self.check_double_relay(message_hash, evidence_hash)
 }
 };

 if is_valid {
 // Fraud proven! Slash the relayer
 self.slash_relayer(&relay.relayer, &proof.challenger, RELAYER_BOND_AMOUNT)?;

 // Update relay status
 let mut pending = self.pending_relays.write().unwrap_or_else(|poisoned| poisoned.into_inner());
 if let Some(relay) = pending.get_mut(&message_hash) {
 relay.status = RelayStatus::Slashed;
 }

 println!("FAST FRAUD PROVEN! Relayer {} slashed", relay.relayer);
 println!(" Challenger {} receives reward: {} OURO",
 proof.challenger,
 FRAUD_PROOF_REWARD / 100_000_000
 );

 return Ok(true);
 }
 }

 Ok(false)
 }

 /// Confirm relay after challenge period
 pub fn confirm_relay(
 &self,
 message_hash: [u8; 32],
 current_time: u64,
 ) -> Result<(), String> {
 let mut pending = self.pending_relays.write().unwrap_or_else(|poisoned| poisoned.into_inner());
 let relay = pending.get_mut(&message_hash)
 .ok_or("Relay not found")?;

 // Check if challenge period passed
 if current_time < relay.relay_timestamp + CHALLENGE_PERIOD_SECS {
 return Err("Challenge period not expired".to_string());
 }

 // Check if not slashed
 if relay.status == RelayStatus::Slashed {
 return Err("Relay was slashed".to_string());
 }

 // Confirm and release bond + reward
 relay.status = RelayStatus::Confirmed;

 let mut bonds = self.relayer_bonds.write().unwrap_or_else(|poisoned| poisoned.into_inner());
 let current_bond = bonds.get(&relay.relayer).unwrap_or(&0);
 bonds.insert(relay.relayer.clone(), current_bond + RELAYER_BOND_AMOUNT + 1_000_000); // +0.01 OURO reward

 println!(" Relay confirmed: {:?}", hex::encode(message_hash));
 println!(" Relayer {} receives bond + reward", relay.relayer);

 Ok(())
 }

 /// Deposit bond for relayer
 pub fn deposit_bond(&self, relayer: String, amount: u64) {
 let mut bonds = self.relayer_bonds.write().unwrap_or_else(|poisoned| poisoned.into_inner());
 let current = bonds.get(&relayer).unwrap_or(&0);
 bonds.insert(relayer.clone(), current + amount);

 println!("REWARD Relayer {} deposited {} OURO bond", relayer, amount / 100_000_000);
 }

 /// Slash relayer and reward challenger
 fn slash_relayer(
 &self,
 relayer: &str,
 challenger: &str,
 amount: u64,
 ) -> Result<(), String> {
 let mut bonds = self.relayer_bonds.write().unwrap_or_else(|poisoned| poisoned.into_inner());
 let relayer_bond = bonds.get(relayer).unwrap_or(&0);

 if *relayer_bond < amount {
 return Err(format!("Insufficient bond to slash: {} < {}", relayer_bond, amount));
 }

 // Slash relayer
 bonds.insert(relayer.to_string(), relayer_bond - amount);

 // Reward challenger (50% of bond)
 let challenger_balance = bonds.get(challenger).unwrap_or(&0);
 bonds.insert(challenger.to_string(), challenger_balance + FRAUD_PROOF_REWARD);

 // Record slash
 let mut slashed = self.slashed_relayers.write().unwrap_or_else(|poisoned| poisoned.into_inner());
 let total_slashed = slashed.get(relayer).unwrap_or(&0);
 slashed.insert(relayer.to_string(), total_slashed + amount);

 Ok(())
 }

 /// Verify merkle proof
 fn verify_merkle_proof(
 &self,
 message_hash: [u8; 32],
 proof: &[[u8; 32]],
 root: &[u8],
 ) -> bool {
 if root.len() != 32 {
 return false;
 }

 let mut current = message_hash;

 for sibling in proof {
 let mut hasher = Sha256::new();
 if current < *sibling {
 hasher.update(&current);
 hasher.update(sibling);
 } else {
 hasher.update(sibling);
 hasher.update(&current);
 }
 current = hasher.finalize().into();
 }

 current == root[0..32]
 }

 /// Check for double relay
 fn check_double_relay(&self, message_hash: [u8; 32], other_hash: [u8; 32]) -> bool {
 if message_hash == other_hash {
 return false;
 }

 let pending = self.pending_relays.read().unwrap_or_else(|poisoned| poisoned.into_inner());

 if let (Some(relay1), Some(relay2)) = (
 pending.get(&message_hash),
 pending.get(&other_hash),
 ) {
 // Check if same nonce from same sender
 relay1.message.nonce == relay2.message.nonce
 && relay1.message.sender == relay2.message.sender
 && relay1.message.source_subchain == relay2.message.source_subchain
 } else {
 false
 }
 }

 /// Get relayer bond
 pub fn get_relayer_bond(&self, relayer: &str) -> u64 {
 let bonds = self.relayer_bonds.read().unwrap_or_else(|poisoned| poisoned.into_inner());
 *bonds.get(relayer).unwrap_or(&0)
 }

 /// Get total slashed amount for relayer
 pub fn get_slashed_amount(&self, relayer: &str) -> u64 {
 let slashed = self.slashed_relayers.read().unwrap_or_else(|poisoned| poisoned.into_inner());
 *slashed.get(relayer).unwrap_or(&0)
 }

 /// Get relay status
 pub fn get_relay_status(&self, message_hash: [u8; 32]) -> Option<RelayStatus> {
 let pending = self.pending_relays.read().unwrap_or_else(|poisoned| poisoned.into_inner());
 pending.get(&message_hash).map(|r| r.status.clone())
 }
}

impl Default for FraudProofManager {
 fn default() -> Self {
 Self::new()
 }
}

#[cfg(test)]
mod tests {
 use super::*;

 #[test]
 fn test_submit_relay() {
 let manager = FraudProofManager::new();

 // Deposit bond first
 manager.deposit_bond("relayer1".to_string(), 200_000_000);

 let message = CrossChainMessage {
 source_subchain: "us".to_string(),
 destination_subchain: "africa".to_string(),
 sender: "alice".to_string(),
 recipient: "bob".to_string(),
 amount: 1_000_000,
 nonce: 1,
 timestamp: 1000,
 };

 let result = manager.submit_relay(
 message,
 "relayer1".to_string(),
 None,
 2000,
 );

 assert!(result.is_ok());
 }

 #[test]
 fn test_insufficient_bond() {
 let manager = FraudProofManager::new();

 let message = CrossChainMessage {
 source_subchain: "us".to_string(),
 destination_subchain: "africa".to_string(),
 sender: "alice".to_string(),
 recipient: "bob".to_string(),
 amount: 1_000_000,
 nonce: 1,
 timestamp: 1000,
 };

 let result = manager.submit_relay(
 message,
 "relayer1".to_string(),
 None,
 2000,
 );

 assert!(result.is_err());
 assert!(result.unwrap_err().contains("Insufficient bond"));
 }

 #[test]
 fn test_fraud_proof_insufficient_balance() {
 let manager = FraudProofManager::new();

 // Setup
 manager.deposit_bond("relayer1".to_string(), 200_000_000);

 let message = CrossChainMessage {
 source_subchain: "us".to_string(),
 destination_subchain: "africa".to_string(),
 sender: "alice".to_string(),
 recipient: "bob".to_string(),
 amount: 1_000_000_000, // 10 OURO
 nonce: 1,
 timestamp: 1000,
 };

 let message_hash = manager.submit_relay(
 message.clone(),
 "relayer1".to_string(),
 None,
 2000,
 ).unwrap();

 // Submit fraud proof
 let result = manager.submit_fraud_proof(
 message_hash,
 "challenger1".to_string(),
 FraudProofType::InsufficientBalance,
 vec![],
 2100,
 );

 assert!(result.is_ok());

 // Verify fraud proof
 let mut source_state = HashMap::new();
 source_state.insert("alice".to_string(), 500_000_000); // Only 5 OURO

 let result = manager.verify_and_slash(
 message_hash,
 &source_state,
 &HashMap::new(),
 );

 assert!(result.is_ok());
 assert!(result.unwrap()); // Fraud proven

 // Check relayer was slashed
 assert_eq!(manager.get_relay_status(message_hash), Some(RelayStatus::Slashed));
 }

 #[test]
 fn test_confirm_relay_after_challenge_period() {
 let manager = FraudProofManager::new();

 manager.deposit_bond("relayer1".to_string(), 200_000_000);

 let message = CrossChainMessage {
 source_subchain: "us".to_string(),
 destination_subchain: "africa".to_string(),
 sender: "alice".to_string(),
 recipient: "bob".to_string(),
 amount: 1_000_000,
 nonce: 1,
 timestamp: 1000,
 };

 let message_hash = manager.submit_relay(
 message,
 "relayer1".to_string(),
 None,
 2000,
 ).unwrap();

 // Try to confirm too early
 let result = manager.confirm_relay(message_hash, 2500);
 assert!(result.is_err());

 // Confirm after challenge period
 let result = manager.confirm_relay(message_hash, 2000 + CHALLENGE_PERIOD_SECS + 1);
 assert!(result.is_ok());

 assert_eq!(manager.get_relay_status(message_hash), Some(RelayStatus::Confirmed));
 }
}
