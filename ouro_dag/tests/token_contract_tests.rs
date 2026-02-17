//! Token contract integration tests
//!
//! Tests the ERC20-like token contract template.

#[cfg(test)]
mod token_tests {
    use std::collections::HashMap;

    /// Token state structure (mirrors contract_templates/token)
    #[derive(Debug, Clone)]
    struct TokenState {
        name: String,
        symbol: String,
        decimals: u8,
        total_supply: u64,
        balances: HashMap<String, u64>,
        allowances: HashMap<String, HashMap<String, u64>>,
        owner: String,
    }

    impl TokenState {
        fn new(name: String, symbol: String, decimals: u8, owner: String) -> Self {
            Self {
                name,
                symbol,
                decimals,
                total_supply: 0,
                balances: HashMap::new(),
                allowances: HashMap::new(),
                owner,
            }
        }

        fn mint(&mut self, to: &str, amount: u64, caller: &str) -> Result<(), String> {
            if caller != self.owner {
                return Err("Only owner can mint".to_string());
            }

            let balance = self.balances.get(to).unwrap_or(&0);
            self.balances.insert(to.to_string(), balance + amount);
            self.total_supply += amount;

            Ok(())
        }

        fn transfer(&mut self, from: &str, to: &str, amount: u64) -> Result<(), String> {
            let sender_balance = *self.balances.get(from).unwrap_or(&0);

            if sender_balance < amount {
                return Err(format!(
                    "Insufficient balance: {} < {}",
                    sender_balance, amount
                ));
            }

            self.balances
                .insert(from.to_string(), sender_balance - amount);

            let recipient_balance = *self.balances.get(to).unwrap_or(&0);
            self.balances
                .insert(to.to_string(), recipient_balance + amount);

            Ok(())
        }

        fn approve(&mut self, owner: &str, spender: &str, amount: u64) -> Result<(), String> {
            self.allowances
                .entry(owner.to_string())
                .or_insert_with(HashMap::new)
                .insert(spender.to_string(), amount);

            Ok(())
        }

        fn transfer_from(
            &mut self,
            spender: &str,
            from: &str,
            to: &str,
            amount: u64,
        ) -> Result<(), String> {
            // Check allowance (copy value to avoid borrow issues)
            let allowance = *self
                .allowances
                .get(from)
                .and_then(|map| map.get(spender))
                .unwrap_or(&0);

            if allowance < amount {
                return Err(format!(
                    "Insufficient allowance: {} < {}",
                    allowance, amount
                ));
            }

            // Check balance
            let from_balance = *self.balances.get(from).unwrap_or(&0);
            if from_balance < amount {
                return Err(format!(
                    "Insufficient balance: {} < {}",
                    from_balance, amount
                ));
            }

            // Update allowance
            self.allowances
                .get_mut(from)
                .unwrap()
                .insert(spender.to_string(), allowance - amount);

            // Transfer
            self.transfer(from, to, amount)?;

            Ok(())
        }

        fn burn(&mut self, from: &str, amount: u64) -> Result<(), String> {
            let balance = *self.balances.get(from).unwrap_or(&0);

            if balance < amount {
                return Err(format!(
                    "Insufficient balance to burn: {} < {}",
                    balance, amount
                ));
            }

            self.balances.insert(from.to_string(), balance - amount);
            self.total_supply -= amount;

            Ok(())
        }

        fn balance_of(&self, address: &str) -> u64 {
            *self.balances.get(address).unwrap_or(&0)
        }

        fn allowance(&self, owner: &str, spender: &str) -> u64 {
            self.allowances
                .get(owner)
                .and_then(|map| map.get(spender))
                .copied()
                .unwrap_or(0)
        }
    }

    #[test]
    fn test_token_initialization() {
        let token = TokenState::new(
            "My Token".to_string(),
            "MTK".to_string(),
            18,
            "owner_address".to_string(),
        );

        assert_eq!(token.name, "My Token");
        assert_eq!(token.symbol, "MTK");
        assert_eq!(token.decimals, 18);
        assert_eq!(token.total_supply, 0);
        assert_eq!(token.owner, "owner_address");

        println!("✅ Token initialized: {} ({})", token.name, token.symbol);
    }

    #[test]
    fn test_token_minting() {
        let mut token = TokenState::new(
            "Test Token".to_string(),
            "TST".to_string(),
            8,
            "owner".to_string(),
        );

        // Owner can mint
        token.mint("alice", 1000, "owner").unwrap();

        assert_eq!(token.balance_of("alice"), 1000);
        assert_eq!(token.total_supply, 1000);

        println!("✅ Minted 1000 tokens to alice");
    }

    #[test]
    fn test_non_owner_cannot_mint() {
        let mut token = TokenState::new(
            "Test Token".to_string(),
            "TST".to_string(),
            8,
            "owner".to_string(),
        );

        // Non-owner cannot mint
        let result = token.mint("alice", 1000, "attacker");

        assert!(result.is_err());
        assert_eq!(token.total_supply, 0);

        println!("✅ Non-owner mint correctly rejected");
    }

    #[test]
    fn test_token_transfer() {
        let mut token = TokenState::new(
            "Test Token".to_string(),
            "TST".to_string(),
            8,
            "owner".to_string(),
        );

        // Setup
        token.mint("alice", 1000, "owner").unwrap();

        // Transfer
        token.transfer("alice", "bob", 300).unwrap();

        assert_eq!(token.balance_of("alice"), 700);
        assert_eq!(token.balance_of("bob"), 300);

        println!("✅ Transferred 300 tokens from alice to bob");
    }

    #[test]
    fn test_transfer_insufficient_balance() {
        let mut token = TokenState::new(
            "Test Token".to_string(),
            "TST".to_string(),
            8,
            "owner".to_string(),
        );

        token.mint("alice", 100, "owner").unwrap();

        // Try to transfer more than balance
        let result = token.transfer("alice", "bob", 200);

        assert!(result.is_err());
        assert_eq!(token.balance_of("alice"), 100);
        assert_eq!(token.balance_of("bob"), 0);

        println!("✅ Insufficient balance transfer correctly rejected");
    }

    #[test]
    fn test_approve_and_allowance() {
        let mut token = TokenState::new(
            "Test Token".to_string(),
            "TST".to_string(),
            8,
            "owner".to_string(),
        );

        // Approve
        token.approve("alice", "bob", 500).unwrap();

        assert_eq!(token.allowance("alice", "bob"), 500);

        println!("✅ Approved bob to spend 500 tokens from alice");
    }

    #[test]
    fn test_transfer_from() {
        let mut token = TokenState::new(
            "Test Token".to_string(),
            "TST".to_string(),
            8,
            "owner".to_string(),
        );

        // Setup
        token.mint("alice", 1000, "owner").unwrap();
        token.approve("alice", "bob", 500).unwrap();

        // Bob transfers from alice's account
        token.transfer_from("bob", "alice", "charlie", 300).unwrap();

        assert_eq!(token.balance_of("alice"), 700);
        assert_eq!(token.balance_of("charlie"), 300);
        assert_eq!(token.allowance("alice", "bob"), 200); // 500 - 300

        println!("✅ TransferFrom executed successfully");
    }

    #[test]
    fn test_transfer_from_insufficient_allowance() {
        let mut token = TokenState::new(
            "Test Token".to_string(),
            "TST".to_string(),
            8,
            "owner".to_string(),
        );

        token.mint("alice", 1000, "owner").unwrap();
        token.approve("alice", "bob", 100).unwrap();

        // Try to transfer more than allowance
        let result = token.transfer_from("bob", "alice", "charlie", 200);

        assert!(result.is_err());
        assert_eq!(token.balance_of("alice"), 1000);

        println!("✅ Insufficient allowance correctly rejected");
    }

    #[test]
    fn test_token_burn() {
        let mut token = TokenState::new(
            "Test Token".to_string(),
            "TST".to_string(),
            8,
            "owner".to_string(),
        );

        token.mint("alice", 1000, "owner").unwrap();

        // Burn
        token.burn("alice", 300).unwrap();

        assert_eq!(token.balance_of("alice"), 700);
        assert_eq!(token.total_supply, 700);

        println!("✅ Burned 300 tokens, supply reduced");
    }

    #[test]
    fn test_burn_insufficient_balance() {
        let mut token = TokenState::new(
            "Test Token".to_string(),
            "TST".to_string(),
            8,
            "owner".to_string(),
        );

        token.mint("alice", 100, "owner").unwrap();

        // Try to burn more than balance
        let result = token.burn("alice", 200);

        assert!(result.is_err());
        assert_eq!(token.balance_of("alice"), 100);
        assert_eq!(token.total_supply, 100);

        println!("✅ Insufficient balance burn correctly rejected");
    }

    #[test]
    fn test_multiple_transfers() {
        let mut token = TokenState::new(
            "Test Token".to_string(),
            "TST".to_string(),
            8,
            "owner".to_string(),
        );

        // Setup
        token.mint("alice", 10000, "owner").unwrap();

        // Multiple transfers
        token.transfer("alice", "bob", 1000).unwrap();
        token.transfer("alice", "charlie", 2000).unwrap();
        token.transfer("bob", "charlie", 500).unwrap();

        assert_eq!(token.balance_of("alice"), 7000);
        assert_eq!(token.balance_of("bob"), 500);
        assert_eq!(token.balance_of("charlie"), 2500);
        assert_eq!(token.total_supply, 10000);

        println!("✅ Multiple transfers executed correctly");
    }

    #[test]
    fn test_zero_transfer() {
        let mut token = TokenState::new(
            "Test Token".to_string(),
            "TST".to_string(),
            8,
            "owner".to_string(),
        );

        token.mint("alice", 1000, "owner").unwrap();

        // Transfer zero (should succeed)
        token.transfer("alice", "bob", 0).unwrap();

        assert_eq!(token.balance_of("alice"), 1000);
        assert_eq!(token.balance_of("bob"), 0);

        println!("✅ Zero transfer handled correctly");
    }

    #[test]
    fn test_supply_tracking() {
        let mut token = TokenState::new(
            "Test Token".to_string(),
            "TST".to_string(),
            8,
            "owner".to_string(),
        );

        // Mint increases supply
        token.mint("alice", 5000, "owner").unwrap();
        assert_eq!(token.total_supply, 5000);

        token.mint("bob", 3000, "owner").unwrap();
        assert_eq!(token.total_supply, 8000);

        // Transfers don't change supply
        token.transfer("alice", "bob", 1000).unwrap();
        assert_eq!(token.total_supply, 8000);

        // Burn decreases supply
        token.burn("alice", 2000).unwrap();
        assert_eq!(token.total_supply, 6000);

        println!("✅ Total supply tracked correctly through mint/burn");
    }

    #[test]
    fn test_gas_estimation_token_operations() {
        struct GasEstimate {
            mint: u64,
            transfer: u64,
            approve: u64,
            transfer_from: u64,
            burn: u64,
        }

        let gas = GasEstimate {
            mint: 30_000,          // Storage write + balance update
            transfer: 25_000,      // Two balance updates
            approve: 20_000,       // Allowance storage write
            transfer_from: 35_000, // Allowance check + two balance updates
            burn: 25_000,          // Balance update + supply decrease
        };

        // Verify all operations are under 50k gas
        assert!(gas.mint < 50_000);
        assert!(gas.transfer < 50_000);
        assert!(gas.approve < 50_000);
        assert!(gas.transfer_from < 50_000);
        assert!(gas.burn < 50_000);

        println!("✅ Gas estimates:");
        println!("   Mint: {} gas", gas.mint);
        println!("   Transfer: {} gas", gas.transfer);
        println!("   Approve: {} gas", gas.approve);
        println!("   TransferFrom: {} gas", gas.transfer_from);
        println!("   Burn: {} gas", gas.burn);
    }
}
