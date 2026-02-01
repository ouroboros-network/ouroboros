//! # Timelock Controller
//!
//! Multisig timelock system for critical operations.
//! Requires a delay period before execution to allow for review and potential veto.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use chrono::{DateTime, Utc, Duration};

/// Timelock configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelockConfig {
 /// Delay in seconds before operation can be executed
 pub delay_secs: u64,

 /// Admin addresses that can schedule operations
 pub admin_addresses: Vec<String>,
}

impl Default for TimelockConfig {
 fn default() -> Self {
 Self {
 delay_secs: 7 * 24 * 60 * 60, // 7 days
 admin_addresses: vec![],
 }
 }
}

/// Type of timelocked operation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OperationType {
 /// Update protocol parameter
 UpdateParameter { key: String, value: String },

 /// Transfer treasury funds
 TransferTreasury { to: String, amount: u64 },

 /// Update validator set
 UpdateValidators { add: Vec<String>, remove: Vec<String> },

 /// Upgrade contract
 UpgradeContract { contract_address: String, new_code_hash: String },

 /// Change governance parameters
 ChangeGovernance { param: String, value: String },
}

/// Status of a timelocked operation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OperationStatus {
 /// Scheduled but not yet executable
 Pending,

 /// Ready to be executed
 Ready,

 /// Successfully executed
 Executed,

 /// Cancelled before execution
 Cancelled,

 /// Execution failed
 Failed { reason: String },
}

/// A timelocked operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelockOperation {
 /// Unique operation ID
 pub id: String,

 /// Type of operation
 pub operation: OperationType,

 /// Address that scheduled this operation
 pub proposer: String,

 /// When operation was scheduled
 pub scheduled_at: DateTime<Utc>,

 /// When operation can be executed (scheduled_at + delay)
 pub executable_at: DateTime<Utc>,

 /// Current status
 pub status: OperationStatus,

 /// Optional description
 pub description: String,

 /// Approvals from multisig (address -> signature)
 pub approvals: HashMap<String, String>,
}

impl TimelockOperation {
 /// Create new timelocked operation
 pub fn new(
 id: String,
 operation: OperationType,
 proposer: String,
 delay_secs: u64,
 description: String,
 ) -> Self {
 let scheduled_at = Utc::now();
 let executable_at = scheduled_at + Duration::seconds(delay_secs as i64);

 Self {
 id,
 operation,
 proposer,
 scheduled_at,
 executable_at,
 status: OperationStatus::Pending,
 description,
 approvals: HashMap::new(),
 }
 }

 /// Check if operation is ready to execute
 pub fn is_ready(&self) -> bool {
 if self.status != OperationStatus::Pending && self.status != OperationStatus::Ready {
 return false;
 }

 Utc::now() >= self.executable_at
 }

 /// Add approval
 pub fn add_approval(&mut self, address: String, signature: String) {
 self.approvals.insert(address, signature);
 }

 /// Check if operation has required approvals
 pub fn has_required_approvals(&self, required: usize) -> bool {
 self.approvals.len() >= required
 }
}

/// Timelock controller
pub struct TimelockController {
 config: TimelockConfig,
 operations: HashMap<String, TimelockOperation>,

 /// Minimum approvals required (default: 3 of 5)
 min_approvals: usize,
}

impl TimelockController {
 /// Create new timelock controller
 pub fn new(config: TimelockConfig) -> Self {
 Self {
 config,
 operations: HashMap::new(),
 min_approvals: 3, // 3 of 5 multisig
 }
 }

 /// Schedule a new operation
 pub fn schedule(
 &mut self,
 operation: OperationType,
 proposer: &str,
 description: String,
 ) -> Result<String, String> {
 // Check if proposer is admin
 if !self.config.admin_addresses.is_empty()
 && !self.config.admin_addresses.contains(&proposer.to_string()) {
 return Err("Proposer is not an admin".to_string());
 }

 // Generate operation ID
 let op_id = format!(
 "timelock_{}_{}",
 Utc::now().timestamp_millis(),
 hex::encode(&proposer.as_bytes()[..8])
 );

 let operation = TimelockOperation::new(
 op_id.clone(),
 operation,
 proposer.to_string(),
 self.config.delay_secs,
 description,
 );

 self.operations.insert(op_id.clone(), operation);

 println!(
 "TIMEOUT Timelock operation scheduled: {} (executable in {} days)",
 op_id,
 self.config.delay_secs / (24 * 60 * 60)
 );

 Ok(op_id)
 }

 /// Approve an operation
 pub fn approve(
 &mut self,
 operation_id: &str,
 approver: &str,
 signature: &str,
 ) -> Result<(), String> {
 let op = self.operations.get_mut(operation_id)
 .ok_or_else(|| "Operation not found".to_string())?;

 if op.status != OperationStatus::Pending && op.status != OperationStatus::Ready {
 return Err(format!("Operation is {:?}, cannot approve", op.status));
 }

 op.add_approval(approver.to_string(), signature.to_string());

 println!(
 " Approval added to operation {} ({}/{})",
 operation_id,
 op.approvals.len(),
 self.min_approvals
 );

 // Update status if ready
 if op.is_ready() && op.has_required_approvals(self.min_approvals) {
 op.status = OperationStatus::Ready;
 println!("🟢 Operation {} is now ready for execution", operation_id);
 }

 Ok(())
 }

 /// Execute an operation
 pub fn execute(&mut self, operation_id: &str) -> Result<OperationType, String> {
 let op = self.operations.get_mut(operation_id)
 .ok_or_else(|| "Operation not found".to_string())?;

 // Check if ready
 if !op.is_ready() {
 return Err(format!(
 "Operation not ready (executable at {})",
 op.executable_at
 ));
 }

 // Check approvals
 if !op.has_required_approvals(self.min_approvals) {
 return Err(format!(
 "Insufficient approvals ({}/{})",
 op.approvals.len(),
 self.min_approvals
 ));
 }

 let operation = op.operation.clone();
 op.status = OperationStatus::Executed;

 println!(" Timelock operation executed: {}", operation_id);

 Ok(operation)
 }

 /// Cancel an operation
 pub fn cancel(
 &mut self,
 operation_id: &str,
 canceller: &str,
 ) -> Result<(), String> {
 // Check if canceller is admin
 if !self.config.admin_addresses.is_empty()
 && !self.config.admin_addresses.contains(&canceller.to_string()) {
 return Err("Only admins can cancel operations".to_string());
 }

 let op = self.operations.get_mut(operation_id)
 .ok_or_else(|| "Operation not found".to_string())?;

 if op.status == OperationStatus::Executed {
 return Err("Cannot cancel executed operation".to_string());
 }

 op.status = OperationStatus::Cancelled;

 println!("ERROR Timelock operation cancelled: {}", operation_id);

 Ok(())
 }

 /// Get operation by ID
 pub fn get_operation(&self, operation_id: &str) -> Option<&TimelockOperation> {
 self.operations.get(operation_id)
 }

 /// Get all pending operations
 pub fn get_pending_operations(&self) -> Vec<&TimelockOperation> {
 self.operations
 .values()
 .filter(|op| op.status == OperationStatus::Pending || op.status == OperationStatus::Ready)
 .collect()
 }

 /// Set minimum approvals required
 pub fn set_min_approvals(&mut self, min: usize) {
 self.min_approvals = min;
 }
}

#[cfg(test)]
mod tests {
 use super::*;

 #[test]
 fn test_schedule_operation() {
 let config = TimelockConfig {
 delay_secs: 60, // 1 minute for testing
 admin_addresses: vec!["admin1".to_string()],
 };

 let mut controller = TimelockController::new(config);

 let op_id = controller.schedule(
 OperationType::UpdateParameter {
 key: "max_block_size".to_string(),
 value: "2000000".to_string(),
 },
 "admin1",
 "Increase max block size to 2MB".to_string(),
 ).unwrap();

 let op = controller.get_operation(&op_id).unwrap();
 assert_eq!(op.status, OperationStatus::Pending);
 assert!(!op.is_ready()); // Not ready yet (need to wait delay)
 }

 #[test]
 fn test_multisig_approval() {
 let config = TimelockConfig::default();
 let mut controller = TimelockController::new(config);

 let op_id = controller.schedule(
 OperationType::TransferTreasury {
 to: "recipient".to_string(),
 amount: 1000000,
 },
 "admin1",
 "Transfer treasury funds".to_string(),
 ).unwrap();

 // Add approvals
 controller.approve(&op_id, "approver1", "sig1").unwrap();
 controller.approve(&op_id, "approver2", "sig2").unwrap();
 controller.approve(&op_id, "approver3", "sig3").unwrap();

 let op = controller.get_operation(&op_id).unwrap();
 assert!(op.has_required_approvals(3));
 }

 #[test]
 fn test_cancel_operation() {
 let config = TimelockConfig {
 delay_secs: 60,
 admin_addresses: vec!["admin1".to_string()],
 };

 let mut controller = TimelockController::new(config);

 let op_id = controller.schedule(
 OperationType::UpdateParameter {
 key: "test".to_string(),
 value: "123".to_string(),
 },
 "admin1",
 "Test operation".to_string(),
 ).unwrap();

 controller.cancel(&op_id, "admin1").unwrap();

 let op = controller.get_operation(&op_id).unwrap();
 assert_eq!(op.status, OperationStatus::Cancelled);
 }
}
