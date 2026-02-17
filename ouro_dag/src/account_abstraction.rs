// Account Abstraction (ERC-4337 style)
// Smart contract wallets with gas sponsorship and social recovery

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// User operation (replaces traditional transaction)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserOperation {
    /// Sender (smart contract wallet address)
    pub sender: String,
    /// Nonce
    pub nonce: u64,
    /// Contract initialization code (if deploying)
    pub init_code: Vec<u8>,
    /// Call data
    pub call_data: Vec<u8>,
    /// Gas limits
    pub call_gas_limit: u64,
    pub verification_gas_limit: u64,
    pub pre_verification_gas: u64,
    /// Gas prices
    pub max_fee_per_gas: u64,
    pub max_priority_fee_per_gas: u64,
    /// Paymaster (gas sponsor)
    pub paymaster: Option<String>,
    pub paymaster_data: Vec<u8>,
    /// Signature
    pub signature: Vec<u8>,
}

/// Smart wallet account
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartWallet {
    pub address: String,
    pub owners: Vec<String>,
    pub threshold: usize, // Multi-sig threshold
    pub nonce: u64,
    pub guardians: Vec<String>, // For social recovery
}

impl SmartWallet {
    /// Create new smart wallet
    pub fn new(owners: Vec<String>, threshold: usize) -> Self {
        let address = Self::compute_address(&owners, threshold);

        Self {
            address,
            owners,
            threshold,
            nonce: 0,
            guardians: Vec::new(),
        }
    }

    /// Add guardian for social recovery
    pub fn add_guardian(&mut self, guardian: String) {
        if !self.guardians.contains(&guardian) {
            self.guardians.push(guardian);
        }
    }

    /// Verify user operation signature
    pub fn verify_signature(&self, op: &UserOperation) -> bool {
        // TODO: Implement multi-sig verification
        // For now, simple check
        op.nonce == self.nonce && !op.signature.is_empty()
    }

    /// Compute wallet address deterministically
    fn compute_address(owners: &[String], threshold: usize) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"smart_wallet");
        for owner in owners {
            hasher.update(owner.as_bytes());
        }
        hasher.update(&threshold.to_le_bytes());
        let hash = hasher.finalize();

        hex::encode(&hash[0..20])
    }
}

/// Paymaster (gas sponsor)
#[derive(Debug, Clone)]
pub struct Paymaster {
    pub address: String,
    pub balance: u64,
    pub supported_tokens: Vec<String>,
}

impl Paymaster {
    pub fn new(address: String, balance: u64) -> Self {
        Self {
            address,
            balance,
            supported_tokens: vec!["OURO".to_string()],
        }
    }

    /// Check if can sponsor transaction
    pub fn can_sponsor(&self, gas_cost: u64) -> bool {
        self.balance >= gas_cost
    }

    /// Sponsor transaction (pay gas)
    pub fn sponsor(&mut self, gas_cost: u64) -> Result<(), String> {
        if !self.can_sponsor(gas_cost) {
            return Err("Insufficient paymaster balance".to_string());
        }

        self.balance -= gas_cost;
        Ok(())
    }
}

/// Entry point contract (validates and executes user operations)
pub struct EntryPoint {
    wallets: HashMap<String, SmartWallet>,
    paymasters: HashMap<String, Paymaster>,
}

impl EntryPoint {
    pub fn new() -> Self {
        Self {
            wallets: HashMap::new(),
            paymasters: HashMap::new(),
        }
    }

    /// Register smart wallet
    pub fn register_wallet(&mut self, wallet: SmartWallet) {
        self.wallets.insert(wallet.address.clone(), wallet);
    }

    /// Register paymaster
    pub fn register_paymaster(&mut self, paymaster: Paymaster) {
        self.paymasters.insert(paymaster.address.clone(), paymaster);
    }

    /// Validate user operation
    pub fn validate_user_op(&self, op: &UserOperation) -> Result<(), String> {
        // Get wallet
        let wallet = self.wallets.get(&op.sender).ok_or("Wallet not found")?;

        // Verify nonce
        if op.nonce != wallet.nonce {
            return Err(format!(
                "Invalid nonce: expected {}, got {}",
                wallet.nonce, op.nonce
            ));
        }

        // Verify signature
        if !wallet.verify_signature(op) {
            return Err("Invalid signature".to_string());
        }

        // Verify paymaster if present
        if let Some(ref paymaster_addr) = op.paymaster {
            let paymaster = self
                .paymasters
                .get(paymaster_addr)
                .ok_or("Paymaster not found")?;

            let gas_cost = op.call_gas_limit + op.verification_gas_limit + op.pre_verification_gas;

            if !paymaster.can_sponsor(gas_cost) {
                return Err("Paymaster cannot sponsor".to_string());
            }
        }

        Ok(())
    }

    /// Execute user operation
    pub fn execute_user_op(&mut self, op: &UserOperation) -> Result<Vec<u8>, String> {
        // Validate
        self.validate_user_op(op)?;

        // Update nonce (defensive: re-check wallet exists to handle race conditions)
        let wallet = self
            .wallets
            .get_mut(&op.sender)
            .ok_or_else(|| "Wallet not found during execution".to_string())?;
        wallet.nonce += 1;

        // Handle gas payment
        let gas_cost = op.call_gas_limit + op.verification_gas_limit + op.pre_verification_gas;

        if let Some(ref paymaster_addr) = op.paymaster {
            // Paymaster pays (defensive: re-check paymaster exists)
            let paymaster = self
                .paymasters
                .get_mut(paymaster_addr)
                .ok_or_else(|| "Paymaster not found during execution".to_string())?;
            paymaster.sponsor(gas_cost)?;
        } else {
            // Wallet pays (not implemented here)
        }

        // Execute call_data (simplified)
        Ok(vec![]) // Return execution result
    }
}

/// Social recovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryRequest {
    pub wallet_address: String,
    pub new_owners: Vec<String>,
    pub guardian_approvals: Vec<String>,
}

impl RecoveryRequest {
    /// Check if recovery is approved
    pub fn is_approved(&self, wallet: &SmartWallet) -> bool {
        let required = (wallet.guardians.len() / 2) + 1;

        let approved_count = self
            .guardian_approvals
            .iter()
            .filter(|g| wallet.guardians.contains(g))
            .count();

        approved_count >= required
    }

    /// Execute recovery (change owners)
    pub fn execute(&self, wallet: &mut SmartWallet) -> Result<(), String> {
        if !self.is_approved(wallet) {
            return Err("Insufficient guardian approvals".to_string());
        }

        wallet.owners = self.new_owners.clone();
        wallet.nonce += 1; // Reset nonce

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smart_wallet() {
        let owners = vec!["alice".to_string(), "bob".to_string()];
        let mut wallet = SmartWallet::new(owners, 2);

        assert_eq!(wallet.threshold, 2);
        assert_eq!(wallet.nonce, 0);

        wallet.add_guardian("charlie".to_string());
        assert_eq!(wallet.guardians.len(), 1);
    }

    #[test]
    fn test_paymaster() {
        let mut paymaster = Paymaster::new("paymaster1".to_string(), 1000);

        assert!(paymaster.can_sponsor(500));
        assert!(paymaster.sponsor(500).is_ok());
        assert_eq!(paymaster.balance, 500);

        assert!(!paymaster.can_sponsor(600));
    }

    #[test]
    fn test_user_operation() {
        let mut entry_point = EntryPoint::new();

        // Create wallet
        let wallet = SmartWallet::new(vec!["alice".to_string()], 1);
        let wallet_addr = wallet.address.clone();
        entry_point.register_wallet(wallet);

        // Create paymaster
        let paymaster = Paymaster::new("paymaster1".to_string(), 10000);
        entry_point.register_paymaster(paymaster);

        // Create user operation
        let op = UserOperation {
            sender: wallet_addr,
            nonce: 0,
            init_code: vec![],
            call_data: vec![],
            call_gas_limit: 100,
            verification_gas_limit: 50,
            pre_verification_gas: 20,
            max_fee_per_gas: 10,
            max_priority_fee_per_gas: 1,
            paymaster: Some("paymaster1".to_string()),
            paymaster_data: vec![],
            signature: vec![1, 2, 3],
        };

        // Execute
        assert!(entry_point.execute_user_op(&op).is_ok());
    }

    #[test]
    fn test_social_recovery() {
        let mut wallet = SmartWallet::new(vec!["alice".to_string()], 1);
        wallet.add_guardian("bob".to_string());
        wallet.add_guardian("charlie".to_string());
        wallet.add_guardian("dave".to_string());

        // Create recovery request (2 of 3 guardians approve)
        let recovery = RecoveryRequest {
            wallet_address: wallet.address.clone(),
            new_owners: vec!["new_alice".to_string()],
            guardian_approvals: vec!["bob".to_string(), "charlie".to_string()],
        };

        assert!(recovery.is_approved(&wallet));
        assert!(recovery.execute(&mut wallet).is_ok());
        assert_eq!(wallet.owners[0], "new_alice");
    }
}
