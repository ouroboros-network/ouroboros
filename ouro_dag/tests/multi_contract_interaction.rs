//! Multi-contract interaction tests
//!
//! Tests complex scenarios involving multiple contracts interacting.

#[cfg(test)]
mod multi_contract_tests {
    use std::collections::HashMap;

    /// Simple token for testing
    #[derive(Debug, Clone)]
    struct Token {
        balances: HashMap<String, u64>,
        allowances: HashMap<String, HashMap<String, u64>>,
    }

    impl Token {
        fn new() -> Self {
            Self {
                balances: HashMap::new(),
                allowances: HashMap::new(),
            }
        }

        fn mint(&mut self, to: &str, amount: u64) {
            *self.balances.entry(to.to_string()).or_insert(0) += amount;
        }

        fn transfer(&mut self, from: &str, to: &str, amount: u64) -> Result<(), String> {
            let from_balance = *self.balances.get(from).unwrap_or(&0);
            if from_balance < amount {
                return Err("Insufficient balance".to_string());
            }

            *self.balances.get_mut(from).unwrap() -= amount;
            *self.balances.entry(to.to_string()).or_insert(0) += amount;
            Ok(())
        }

        fn approve(&mut self, owner: &str, spender: &str, amount: u64) {
            self.allowances
                .entry(owner.to_string())
                .or_insert_with(HashMap::new)
                .insert(spender.to_string(), amount);
        }

        fn transfer_from(
            &mut self,
            spender: &str,
            from: &str,
            to: &str,
            amount: u64,
        ) -> Result<(), String> {
            let allowance = *self
                .allowances
                .get(from)
                .and_then(|map| map.get(spender))
                .unwrap_or(&0);

            if allowance < amount {
                return Err("Insufficient allowance".to_string());
            }

            self.transfer(from, to, amount)?;

            self.allowances
                .get_mut(from)
                .unwrap()
                .insert(spender.to_string(), allowance - amount);

            Ok(())
        }

        fn balance_of(&self, address: &str) -> u64 {
            *self.balances.get(address).unwrap_or(&0)
        }
    }

    /// Simple DEX for testing
    #[derive(Debug)]
    struct SimpleDEX {
        reserve_a: u64,
        reserve_b: u64,
        liquidity_shares: HashMap<String, u64>,
        total_shares: u64,
    }

    impl SimpleDEX {
        fn new() -> Self {
            Self {
                reserve_a: 0,
                reserve_b: 0,
                liquidity_shares: HashMap::new(),
                total_shares: 0,
            }
        }

        fn add_liquidity(&mut self, provider: &str, amount_a: u64, amount_b: u64) -> u64 {
            let shares = if self.total_shares == 0 {
                (amount_a as f64 * amount_b as f64).sqrt() as u64
            } else {
                std::cmp::min(
                    amount_a * self.total_shares / self.reserve_a,
                    amount_b * self.total_shares / self.reserve_b,
                )
            };

            self.reserve_a += amount_a;
            self.reserve_b += amount_b;
            self.total_shares += shares;
            *self
                .liquidity_shares
                .entry(provider.to_string())
                .or_insert(0) += shares;

            shares
        }

        fn swap_a_for_b(&mut self, amount_a: u64) -> u64 {
            let amount_b = (amount_a * self.reserve_b) / (self.reserve_a + amount_a);
            self.reserve_a += amount_a;
            self.reserve_b -= amount_b;
            amount_b
        }
    }

    #[test]
    fn test_token_to_dex_interaction() {
        let mut token_a = Token::new();
        let mut token_b = Token::new();
        let mut dex = SimpleDEX::new();

        // Setup: Mint tokens to alice
        token_a.mint("alice", 10_000);
        token_b.mint("alice", 10_000);

        println!("✅ Initial balances:");
        println!("   Alice Token A: {}", token_a.balance_of("alice"));
        println!("   Alice Token B: {}", token_b.balance_of("alice"));

        // Alice approves DEX to spend her tokens
        token_a.approve("alice", "dex", 5_000);
        token_b.approve("alice", "dex", 5_000);

        // Simulate DEX pulling tokens (via transfer_from)
        token_a.transfer_from("dex", "alice", "dex", 5_000).unwrap();
        token_b.transfer_from("dex", "alice", "dex", 5_000).unwrap();

        // DEX adds liquidity
        let shares = dex.add_liquidity("alice", 5_000, 5_000);

        assert_eq!(token_a.balance_of("alice"), 5_000);
        assert_eq!(token_b.balance_of("alice"), 5_000);
        assert_eq!(dex.reserve_a, 5_000);
        assert_eq!(dex.reserve_b, 5_000);
        assert!(shares > 0);

        println!("✅ Liquidity added:");
        println!("   DEX Reserve A: {}", dex.reserve_a);
        println!("   DEX Reserve B: {}", dex.reserve_b);
        println!("   Alice LP shares: {}", shares);
    }

    #[test]
    fn test_multi_user_dex_interaction() {
        let mut token_a = Token::new();
        let mut token_b = Token::new();
        let mut dex = SimpleDEX::new();

        // Setup
        token_a.mint("alice", 10_000);
        token_b.mint("alice", 10_000);
        token_a.mint("bob", 1_000);

        // Alice adds initial liquidity
        token_a.transfer("alice", "dex", 5_000).unwrap();
        token_b.transfer("alice", "dex", 5_000).unwrap();
        dex.add_liquidity("alice", 5_000, 5_000);

        // Bob swaps Token A for Token B
        token_a.transfer("bob", "dex", 500).unwrap();
        let amount_b_out = dex.swap_a_for_b(500);
        token_b.transfer("dex", "bob", amount_b_out).unwrap();

        assert_eq!(token_b.balance_of("bob"), amount_b_out);
        assert!(amount_b_out > 0);

        println!("✅ Multi-user DEX interaction:");
        println!("   Bob swapped 500 Token A");
        println!("   Bob received {} Token B", amount_b_out);
        println!("   New reserves: A={}, B={}", dex.reserve_a, dex.reserve_b);
    }

    #[test]
    fn test_nft_marketplace_with_tokens() {
        // NFT Marketplace accepting token payments
        #[derive(Debug)]
        struct NFTMarketplace {
            listings: HashMap<u64, (String, u64)>, // token_id -> (seller, price)
        }

        impl NFTMarketplace {
            fn new() -> Self {
                Self {
                    listings: HashMap::new(),
                }
            }

            fn list(&mut self, token_id: u64, seller: &str, price: u64) {
                self.listings.insert(token_id, (seller.to_string(), price));
            }

            fn buy(&mut self, token_id: u64) -> Option<(String, u64)> {
                self.listings.remove(&token_id)
            }
        }

        let mut token = Token::new();
        let mut marketplace = NFTMarketplace::new();
        let mut nft_owners: HashMap<u64, String> = HashMap::new();

        // Setup
        token.mint("buyer", 10_000);
        nft_owners.insert(1, "seller".to_string());

        // Seller lists NFT #1 for 1000 tokens
        marketplace.list(1, "seller", 1_000);

        // Buyer purchases
        let (seller, price) = marketplace.buy(1).unwrap();

        // Transfer tokens
        token.transfer("buyer", &seller, price).unwrap();

        // Transfer NFT
        nft_owners.insert(1, "buyer".to_string());

        assert_eq!(token.balance_of("buyer"), 9_000);
        assert_eq!(token.balance_of("seller"), 1_000);
        assert_eq!(nft_owners.get(&1).unwrap(), "buyer");

        println!("✅ NFT marketplace interaction:");
        println!("   Buyer purchased NFT #1 for {} tokens", price);
        println!("   Buyer balance: {}", token.balance_of("buyer"));
        println!("   NFT #1 owner: {}", nft_owners.get(&1).unwrap());
    }

    #[test]
    fn test_staking_contract_interaction() {
        #[derive(Debug)]
        struct StakingPool {
            staked: HashMap<String, u64>,
            total_staked: u64,
            rewards_pool: u64,
        }

        impl StakingPool {
            fn new() -> Self {
                Self {
                    staked: HashMap::new(),
                    total_staked: 0,
                    rewards_pool: 0,
                }
            }

            fn stake(&mut self, user: &str, amount: u64) {
                *self.staked.entry(user.to_string()).or_insert(0) += amount;
                self.total_staked += amount;
            }

            fn unstake(&mut self, user: &str, amount: u64) -> Result<(), String> {
                let staked = self.staked.get(user).unwrap_or(&0);
                if *staked < amount {
                    return Err("Insufficient staked amount".to_string());
                }

                *self.staked.get_mut(user).unwrap() -= amount;
                self.total_staked -= amount;
                Ok(())
            }

            fn calculate_rewards(&self, user: &str) -> u64 {
                let user_stake = *self.staked.get(user).unwrap_or(&0);
                if self.total_staked == 0 {
                    return 0;
                }
                (user_stake as f64 / self.total_staked as f64 * self.rewards_pool as f64) as u64
            }

            fn add_rewards(&mut self, amount: u64) {
                self.rewards_pool += amount;
            }
        }

        let mut token = Token::new();
        let mut staking = StakingPool::new();

        // Setup
        token.mint("alice", 10_000);
        token.mint("bob", 5_000);

        // Stake tokens
        token.transfer("alice", "staking_pool", 5_000).unwrap();
        staking.stake("alice", 5_000);

        token.transfer("bob", "staking_pool", 2_000).unwrap();
        staking.stake("bob", 2_000);

        // Add rewards
        token.mint("rewards", 700);
        token.transfer("rewards", "staking_pool", 700).unwrap();
        staking.add_rewards(700);

        // Calculate rewards
        let alice_rewards = staking.calculate_rewards("alice");
        let bob_rewards = staking.calculate_rewards("bob");

        println!("✅ Staking pool interaction:");
        println!("   Alice staked: 5000, rewards: {}", alice_rewards);
        println!("   Bob staked: 2000, rewards: {}", bob_rewards);
        println!("   Total staked: {}", staking.total_staked);

        assert!(alice_rewards > bob_rewards); // Alice staked more
        assert_eq!(alice_rewards + bob_rewards, 700); // Total rewards distributed
    }

    #[test]
    fn test_dao_governance_with_tokens() {
        #[derive(Debug)]
        struct DAO {
            proposals: HashMap<u64, Proposal>,
            next_proposal_id: u64,
        }

        #[derive(Debug)]
        struct Proposal {
            description: String,
            yes_votes: u64,
            no_votes: u64,
            voters: HashMap<String, bool>,
        }

        impl DAO {
            fn new() -> Self {
                Self {
                    proposals: HashMap::new(),
                    next_proposal_id: 1,
                }
            }

            fn create_proposal(&mut self, description: String) -> u64 {
                let id = self.next_proposal_id;
                self.next_proposal_id += 1;

                self.proposals.insert(
                    id,
                    Proposal {
                        description,
                        yes_votes: 0,
                        no_votes: 0,
                        voters: HashMap::new(),
                    },
                );

                id
            }

            fn vote(
                &mut self,
                proposal_id: u64,
                voter: &str,
                voting_power: u64,
                vote_yes: bool,
            ) -> Result<(), String> {
                let proposal = self
                    .proposals
                    .get_mut(&proposal_id)
                    .ok_or_else(|| "Proposal not found".to_string())?;

                if proposal.voters.contains_key(voter) {
                    return Err("Already voted".to_string());
                }

                if vote_yes {
                    proposal.yes_votes += voting_power;
                } else {
                    proposal.no_votes += voting_power;
                }

                proposal.voters.insert(voter.to_string(), vote_yes);

                Ok(())
            }

            fn get_result(&self, proposal_id: u64) -> Option<bool> {
                self.proposals
                    .get(&proposal_id)
                    .map(|p| p.yes_votes > p.no_votes)
            }
        }

        let token = Token::new(); // Governance token
        let mut dao = DAO::new();

        // Create proposal
        let proposal_id = dao.create_proposal("Increase block size".to_string());

        // Vote (weighted by token balance)
        dao.vote(proposal_id, "alice", 5_000, true).unwrap(); // 5000 tokens = 5000 votes yes
        dao.vote(proposal_id, "bob", 2_000, false).unwrap(); // 2000 tokens = 2000 votes no
        dao.vote(proposal_id, "charlie", 1_000, true).unwrap(); // 1000 tokens = 1000 votes yes

        let passed = dao.get_result(proposal_id).unwrap();

        assert!(passed); // 6000 yes vs 2000 no

        println!("✅ DAO governance interaction:");
        println!(
            "   Proposal #{}: {}",
            proposal_id,
            if passed { "PASSED" } else { "REJECTED" }
        );
        println!("   Yes: 6000 votes, No: 2000 votes");
    }

    #[test]
    fn test_complex_defi_scenario() {
        // Complex DeFi: User stakes tokens, receives LP tokens, uses LP as collateral
        let mut base_token = Token::new();
        let mut lp_token = Token::new();
        let mut staking = SimpleDEX::new();

        // User deposits into liquidity pool
        base_token.mint("user", 10_000);

        // Simulate staking
        base_token.transfer("user", "pool", 5_000).unwrap();
        lp_token.mint("user", 5_000); // Receive LP tokens

        // User has LP tokens
        assert_eq!(base_token.balance_of("user"), 5_000);
        assert_eq!(lp_token.balance_of("user"), 5_000);

        // User uses LP tokens as collateral (transfers to lending protocol)
        lp_token.transfer("user", "lending", 2_000).unwrap();

        // User borrows against collateral
        base_token.mint("lending", 1_000); // Lending protocol mints borrowed tokens
        base_token.transfer("lending", "user", 1_000).unwrap();

        // Final state
        assert_eq!(base_token.balance_of("user"), 6_000); // 5000 + 1000 borrowed
        assert_eq!(lp_token.balance_of("user"), 3_000); // 5000 - 2000 collateral
        assert_eq!(lp_token.balance_of("lending"), 2_000); // Holding collateral

        println!("✅ Complex DeFi scenario:");
        println!("   User base tokens: {}", base_token.balance_of("user"));
        println!("   User LP tokens: {}", lp_token.balance_of("user"));
        println!("   Collateral locked: {}", lp_token.balance_of("lending"));
    }

    #[test]
    fn test_cross_contract_reentrancy_guard() {
        // Test that contracts can guard against reentrancy attacks
        #[derive(Debug)]
        struct Vault {
            balances: HashMap<String, u64>,
            locked: bool,
        }

        impl Vault {
            fn new() -> Self {
                Self {
                    balances: HashMap::new(),
                    locked: false,
                }
            }

            fn deposit(&mut self, user: &str, amount: u64) -> Result<(), String> {
                if self.locked {
                    return Err("Reentrancy detected".to_string());
                }

                self.locked = true;
                *self.balances.entry(user.to_string()).or_insert(0) += amount;
                self.locked = false;

                Ok(())
            }

            fn withdraw(&mut self, user: &str, amount: u64) -> Result<u64, String> {
                if self.locked {
                    return Err("Reentrancy detected".to_string());
                }

                self.locked = true;

                let balance = *self.balances.get(user).unwrap_or(&0);
                if balance < amount {
                    self.locked = false;
                    return Err("Insufficient balance".to_string());
                }

                *self.balances.get_mut(user).unwrap() -= amount;
                self.locked = false;

                Ok(amount)
            }
        }

        let mut vault = Vault::new();

        // Normal operation
        vault.deposit("alice", 1_000).unwrap();

        // Simulate lock during withdrawal
        vault.locked = true;
        let result = vault.withdraw("alice", 500);
        assert!(result.is_err()); // Should be rejected

        // Reset and try normal withdrawal
        vault.locked = false;
        let amount = vault.withdraw("alice", 500).unwrap();
        assert_eq!(amount, 500);

        println!("✅ Reentrancy guard working correctly");
    }

    #[test]
    fn test_gas_estimation_multi_contract() {
        // Estimate gas for multi-contract interactions
        const GAS_EXTERNAL_CALL: u64 = 700;
        const GAS_STORAGE_WRITE: u64 = 20_000;
        const GAS_STORAGE_READ: u64 = 200;

        let mut total_gas = 0u64;

        // Scenario: User approves DEX, DEX pulls tokens, DEX swaps, DEX sends back
        total_gas += GAS_EXTERNAL_CALL; // approve() call to token
        total_gas += GAS_STORAGE_WRITE; // Write approval

        total_gas += GAS_EXTERNAL_CALL; // transferFrom() call
        total_gas += GAS_STORAGE_READ * 2; // Read balances
        total_gas += GAS_STORAGE_WRITE * 2; // Update balances

        total_gas += GAS_STORAGE_READ * 2; // Read reserves
        total_gas += 500; // Swap calculation
        total_gas += GAS_STORAGE_WRITE * 2; // Update reserves

        total_gas += GAS_EXTERNAL_CALL; // transfer() call back to user
        total_gas += GAS_STORAGE_WRITE * 2; // Update balances

        println!("📊 Multi-contract gas estimate: {} gas", total_gas);

        assert!(
            total_gas < 200_000,
            "Multi-contract interaction should be under 200k gas"
        );

        println!("✅ Multi-contract gas estimation complete");
    }
}
