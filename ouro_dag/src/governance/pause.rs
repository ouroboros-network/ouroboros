//! # Emergency Pause System
//!
//! Guardian-based emergency pause mechanism for critical situations.
//! Requires 3 of 5 guardians to activate pause.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use chrono::{DateTime, Utc, Duration};

/// Reason for emergency pause
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PauseReason {
 /// Critical security vulnerability detected
 SecurityVulnerability { description: String },

 /// Potential exploit in progress
 ActiveExploit { description: String },

 /// Oracle manipulation detected
 OracleManipulation { oracle_id: String },

 /// Consensus failure
 ConsensusFailure { description: String },

 /// Network partition detected
 NetworkPartition { description: String },

 /// Custom reason
 Other { reason: String },
}

impl PauseReason {
 pub fn description(&self) -> String {
 match self {
 PauseReason::SecurityVulnerability { description } => {
 format!("Security vulnerability: {}", description)
 }
 PauseReason::ActiveExploit { description } => {
 format!("Active exploit: {}", description)
 }
 PauseReason::OracleManipulation { oracle_id } => {
 format!("Oracle manipulation: {}", oracle_id)
 }
 PauseReason::ConsensusFailure { description } => {
 format!("Consensus failure: {}", description)
 }
 PauseReason::NetworkPartition { description } => {
 format!("Network partition: {}", description)
 }
 PauseReason::Other { reason } => reason.clone(),
 }
 }
}

/// Guardian set configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardianSet {
 /// List of guardian addresses
 pub guardians: Vec<String>,

 /// Minimum guardians required to pause (default: 3)
 pub min_required: usize,
}

impl GuardianSet {
 /// Create new guardian set
 pub fn new(guardians: Vec<String>, min_required: usize) -> Self {
 assert!(
 min_required <= guardians.len(),
 "min_required cannot exceed total guardians"
 );

 Self {
 guardians,
 min_required,
 }
 }

 /// Check if address is a guardian
 pub fn is_guardian(&self, address: &str) -> bool {
 self.guardians.contains(&address.to_string())
 }

 /// Add guardian
 pub fn add_guardian(&mut self, address: String) {
 if !self.guardians.contains(&address) {
 self.guardians.push(address);
 }
 }

 /// Remove guardian
 pub fn remove_guardian(&mut self, address: &str) -> Result<(), String> {
 if self.guardians.len() <= self.min_required {
 return Err("Cannot remove guardian: would fall below minimum".to_string());
 }

 self.guardians.retain(|g| g != address);
 Ok(())
 }
}

/// Pause vote from a guardian
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PauseVote {
 /// Guardian address
 pub guardian: String,

 /// Vote timestamp
 pub timestamp: DateTime<Utc>,

 /// Digital signature
 pub signature: String,

 /// Reason for pause
 pub reason: PauseReason,
}

/// Emergency pause state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PauseState {
 /// Is system currently paused
 pub paused: bool,

 /// When pause was activated
 pub paused_at: Option<DateTime<Utc>>,

 /// When pause will automatically expire (24 hours default)
 pub expires_at: Option<DateTime<Utc>>,

 /// Reason for pause
 pub reason: Option<PauseReason>,

 /// Votes from guardians
 pub votes: HashMap<String, PauseVote>,

 /// Resolution actions taken
 pub resolution: Option<String>,
}

impl Default for PauseState {
 fn default() -> Self {
 Self {
 paused: false,
 paused_at: None,
 expires_at: None,
 reason: None,
 votes: HashMap::new(),
 resolution: None,
 }
 }
}

/// Emergency pause controller
pub struct EmergencyPause {
 guardian_set: GuardianSet,
 state: PauseState,

 /// Auto-expire duration in seconds (default: 24 hours)
 auto_expire_secs: u64,
}

impl EmergencyPause {
 /// Create new emergency pause controller
 pub fn new(guardian_set: GuardianSet) -> Self {
 Self {
 guardian_set,
 state: PauseState::default(),
 auto_expire_secs: 24 * 60 * 60, // 24 hours
 }
 }

 /// Check if system is paused
 pub fn is_paused(&self) -> bool {
 if !self.state.paused {
 return false;
 }

 // Check auto-expiry
 if let Some(expires_at) = self.state.expires_at {
 if Utc::now() >= expires_at {
 return false; // Pause has expired
 }
 }

 true
 }

 /// Vote to pause system
 pub fn vote_pause(
 &mut self,
 guardian: &str,
 signature: &str,
 reason: PauseReason,
 ) -> Result<bool, String> {
 // Check if guardian is valid
 if !self.guardian_set.is_guardian(guardian) {
 return Err("Address is not a guardian".to_string());
 }

 // Check if already paused
 if self.state.paused {
 return Err("System is already paused".to_string());
 }

 // Record vote
 let vote = PauseVote {
 guardian: guardian.to_string(),
 timestamp: Utc::now(),
 signature: signature.to_string(),
 reason: reason.clone(),
 };

 self.state.votes.insert(guardian.to_string(), vote);

 println!(
 "PROTECTED Guardian {} voted to pause ({}/{})",
 guardian,
 self.state.votes.len(),
 self.guardian_set.min_required
 );

 // Check if threshold reached
 if self.state.votes.len() >= self.guardian_set.min_required {
 self.activate_pause(reason)?;
 return Ok(true); // Pause activated
 }

 Ok(false) // Vote recorded, but not enough votes yet
 }

 /// Activate emergency pause
 fn activate_pause(&mut self, reason: PauseReason) -> Result<(), String> {
 let now = Utc::now();
 let expires_at = now + Duration::seconds(self.auto_expire_secs as i64);

 self.state.paused = true;
 self.state.paused_at = Some(now);
 self.state.expires_at = Some(expires_at);
 self.state.reason = Some(reason.clone());

 println!("CRITICAL: EMERGENCY PAUSE ACTIVATED: {}", reason.description());
 println!(" Pause will auto-expire at: {}", expires_at);

 Ok(())
 }

 /// Unpause system (requires threshold votes)
 pub fn vote_unpause(
 &mut self,
 guardian: &str,
 signature: &str,
 resolution: String,
 ) -> Result<bool, String> {
 // Check if guardian is valid
 if !self.guardian_set.is_guardian(guardian) {
 return Err("Address is not a guardian".to_string());
 }

 // Check if paused
 if !self.state.paused {
 return Err("System is not paused".to_string());
 }

 // Remove vote (reuse votes HashMap for unpause votes)
 self.state.votes.remove(guardian);

 println!(
 " Guardian {} voted to unpause (remaining: {})",
 guardian,
 self.state.votes.len()
 );

 // Check if all votes removed (consensus to unpause)
 if self.state.votes.is_empty() {
 self.deactivate_pause(resolution)?;
 return Ok(true); // Unpaused
 }

 Ok(false) // Vote recorded, still paused
 }

 /// Deactivate pause
 fn deactivate_pause(&mut self, resolution: String) -> Result<(), String> {
 self.state.paused = false;
 self.state.resolution = Some(resolution.clone());

 println!(" Emergency pause deactivated");
 println!(" Resolution: {}", resolution);

 Ok(())
 }

 /// Force unpause (for expired pauses)
 pub fn force_unpause(&mut self) -> Result<(), String> {
 if !self.state.paused {
 return Err("System is not paused".to_string());
 }

 // Check if expired
 if let Some(expires_at) = self.state.expires_at {
 if Utc::now() < expires_at {
 return Err("Pause has not expired yet".to_string());
 }
 }

 self.state.paused = false;
 self.state.resolution = Some("Auto-expired after timeout".to_string());

 println!("TIMEOUT Emergency pause auto-expired");

 Ok(())
 }

 /// Get pause state
 pub fn get_state(&self) -> &PauseState {
 &self.state
 }

 /// Get guardian set
 pub fn get_guardians(&self) -> &GuardianSet {
 &self.guardian_set
 }

 /// Update guardian set (governance-controlled)
 pub fn update_guardians(&mut self, new_set: GuardianSet) {
 println!("SYNC Guardian set updated: {} guardians", new_set.guardians.len());
 self.guardian_set = new_set;
 }

 /// Set auto-expire duration
 pub fn set_auto_expire(&mut self, secs: u64) {
 self.auto_expire_secs = secs;
 }
}

#[cfg(test)]
mod tests {
 use super::*;

 fn create_test_guardians() -> GuardianSet {
 GuardianSet::new(
 vec![
 "guardian1".to_string(),
 "guardian2".to_string(),
 "guardian3".to_string(),
 "guardian4".to_string(),
 "guardian5".to_string(),
 ],
 3, // 3 of 5
 )
 }

 #[test]
 fn test_pause_activation() {
 let guardians = create_test_guardians();
 let mut pause = EmergencyPause::new(guardians);

 assert!(!pause.is_paused());

 let reason = PauseReason::SecurityVulnerability {
 description: "Critical bug found".to_string(),
 };

 // First vote
 let result = pause.vote_pause("guardian1", "sig1", reason.clone()).unwrap();
 assert!(!result); // Not enough votes
 assert!(!pause.is_paused());

 // Second vote
 let result = pause.vote_pause("guardian2", "sig2", reason.clone()).unwrap();
 assert!(!result); // Still not enough
 assert!(!pause.is_paused());

 // Third vote - should activate pause
 let result = pause.vote_pause("guardian3", "sig3", reason.clone()).unwrap();
 assert!(result); // Pause activated
 assert!(pause.is_paused());
 }

 #[test]
 fn test_unpause() {
 let guardians = create_test_guardians();
 let mut pause = EmergencyPause::new(guardians);

 // Activate pause
 let reason = PauseReason::ActiveExploit {
 description: "Exploit detected".to_string(),
 };

 pause.vote_pause("guardian1", "sig1", reason.clone()).unwrap();
 pause.vote_pause("guardian2", "sig2", reason.clone()).unwrap();
 pause.vote_pause("guardian3", "sig3", reason).unwrap();

 assert!(pause.is_paused());

 // Vote to unpause
 pause.vote_unpause("guardian1", "sig1", "Issue resolved".to_string()).unwrap();
 assert!(pause.is_paused()); // Still paused

 pause.vote_unpause("guardian2", "sig2", "Issue resolved".to_string()).unwrap();
 assert!(pause.is_paused()); // Still paused

 pause.vote_unpause("guardian3", "sig3", "Issue resolved".to_string()).unwrap();
 assert!(!pause.is_paused()); // Now unpaused
 }

 #[test]
 fn test_non_guardian_cannot_pause() {
 let guardians = create_test_guardians();
 let mut pause = EmergencyPause::new(guardians);

 let reason = PauseReason::Other {
 reason: "Test".to_string(),
 };

 let result = pause.vote_pause("random_address", "sig", reason);
 assert!(result.is_err());
 }

 #[test]
 fn test_guardian_set_management() {
 let mut guardians = create_test_guardians();

 assert!(guardians.is_guardian("guardian1"));
 assert!(!guardians.is_guardian("guardian6"));

 guardians.add_guardian("guardian6".to_string());
 assert!(guardians.is_guardian("guardian6"));

 guardians.remove_guardian("guardian6").unwrap();
 assert!(!guardians.is_guardian("guardian6"));
 }
}
