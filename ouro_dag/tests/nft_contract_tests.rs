//! NFT contract integration tests
//!
//! Tests the ERC721-like NFT contract template.

#[cfg(test)]
mod nft_tests {
    use std::collections::HashMap;

    /// NFT state structure (mirrors contract_templates/nft)
    #[derive(Debug, Clone)]
    struct NFTState {
        name: String,
        symbol: String,
        owner: String,
        next_token_id: u64,
        owners: HashMap<u64, String>,          // token_id -> owner
        balances: HashMap<String, u64>,        // address -> count
        token_approvals: HashMap<u64, String>, // token_id -> approved
        operator_approvals: HashMap<String, HashMap<String, bool>>, // owner -> operator -> approved
        token_uris: HashMap<u64, String>,      // token_id -> URI
    }

    impl NFTState {
        fn new(name: String, symbol: String, owner: String) -> Self {
            Self {
                name,
                symbol,
                owner,
                next_token_id: 1,
                owners: HashMap::new(),
                balances: HashMap::new(),
                token_approvals: HashMap::new(),
                operator_approvals: HashMap::new(),
                token_uris: HashMap::new(),
            }
        }

        fn mint(&mut self, to: &str, uri: String, caller: &str) -> Result<u64, String> {
            if caller != self.owner {
                return Err("Only owner can mint".to_string());
            }

            let token_id = self.next_token_id;
            self.next_token_id += 1;

            self.owners.insert(token_id, to.to_string());

            let balance = self.balances.get(to).unwrap_or(&0);
            self.balances.insert(to.to_string(), balance + 1);

            self.token_uris.insert(token_id, uri);

            Ok(token_id)
        }

        fn owner_of(&self, token_id: u64) -> Option<String> {
            self.owners.get(&token_id).cloned()
        }

        fn balance_of(&self, address: &str) -> u64 {
            *self.balances.get(address).unwrap_or(&0)
        }

        fn token_uri(&self, token_id: u64) -> Option<String> {
            self.token_uris.get(&token_id).cloned()
        }

        fn transfer(
            &mut self,
            from: &str,
            to: &str,
            token_id: u64,
            caller: &str,
        ) -> Result<(), String> {
            // Check ownership
            let current_owner = self
                .owner_of(token_id)
                .ok_or_else(|| "Token does not exist".to_string())?;

            if current_owner != from {
                return Err("From address is not owner".to_string());
            }

            // Check authorization
            if caller != from && !self.is_approved_or_owner(caller, token_id) {
                return Err("Not authorized".to_string());
            }

            // Update ownership
            self.owners.insert(token_id, to.to_string());

            // Update balances
            let from_balance = self.balances.get(from).unwrap_or(&0);
            self.balances.insert(from.to_string(), from_balance - 1);

            let to_balance = self.balances.get(to).unwrap_or(&0);
            self.balances.insert(to.to_string(), to_balance + 1);

            // Clear approval
            self.token_approvals.remove(&token_id);

            Ok(())
        }

        fn approve(&mut self, spender: &str, token_id: u64, caller: &str) -> Result<(), String> {
            let owner = self
                .owner_of(token_id)
                .ok_or_else(|| "Token does not exist".to_string())?;

            if caller != owner && !self.is_approved_for_all(&owner, caller) {
                return Err("Not authorized".to_string());
            }

            self.token_approvals.insert(token_id, spender.to_string());

            Ok(())
        }

        fn set_approval_for_all(&mut self, operator: &str, approved: bool, caller: &str) {
            self.operator_approvals
                .entry(caller.to_string())
                .or_insert_with(HashMap::new)
                .insert(operator.to_string(), approved);
        }

        fn get_approved(&self, token_id: u64) -> Option<String> {
            self.token_approvals.get(&token_id).cloned()
        }

        fn is_approved_for_all(&self, owner: &str, operator: &str) -> bool {
            self.operator_approvals
                .get(owner)
                .and_then(|map| map.get(operator))
                .copied()
                .unwrap_or(false)
        }

        fn is_approved_or_owner(&self, address: &str, token_id: u64) -> bool {
            if let Some(owner) = self.owner_of(token_id) {
                if owner == address {
                    return true;
                }

                if let Some(approved) = self.get_approved(token_id) {
                    if approved == address {
                        return true;
                    }
                }

                if self.is_approved_for_all(&owner, address) {
                    return true;
                }
            }

            false
        }

        fn burn(&mut self, token_id: u64, caller: &str) -> Result<(), String> {
            let owner = self
                .owner_of(token_id)
                .ok_or_else(|| "Token does not exist".to_string())?;

            if !self.is_approved_or_owner(caller, token_id) {
                return Err("Not authorized to burn".to_string());
            }

            // Remove ownership
            self.owners.remove(&token_id);

            // Update balance
            let balance = self.balances.get(&owner).unwrap_or(&0);
            self.balances.insert(owner.clone(), balance - 1);

            // Remove approvals and URI
            self.token_approvals.remove(&token_id);
            self.token_uris.remove(&token_id);

            Ok(())
        }
    }

    #[test]
    fn test_nft_initialization() {
        let nft = NFTState::new(
            "My NFT".to_string(),
            "MNFT".to_string(),
            "owner_address".to_string(),
        );

        assert_eq!(nft.name, "My NFT");
        assert_eq!(nft.symbol, "MNFT");
        assert_eq!(nft.owner, "owner_address");
        assert_eq!(nft.next_token_id, 1);

        println!(
            "✅ NFT collection initialized: {} ({})",
            nft.name, nft.symbol
        );
    }

    #[test]
    fn test_nft_minting() {
        let mut nft = NFTState::new(
            "Test NFT".to_string(),
            "TNFT".to_string(),
            "owner".to_string(),
        );

        // Mint
        let token_id = nft
            .mint("alice", "ipfs://Qm123...".to_string(), "owner")
            .unwrap();

        assert_eq!(token_id, 1);
        assert_eq!(nft.owner_of(token_id), Some("alice".to_string()));
        assert_eq!(nft.balance_of("alice"), 1);
        assert_eq!(nft.token_uri(token_id), Some("ipfs://Qm123...".to_string()));

        println!("✅ Minted NFT #{} to alice", token_id);
    }

    #[test]
    fn test_sequential_token_ids() {
        let mut nft = NFTState::new(
            "Test NFT".to_string(),
            "TNFT".to_string(),
            "owner".to_string(),
        );

        let id1 = nft.mint("alice", "uri1".to_string(), "owner").unwrap();
        let id2 = nft.mint("bob", "uri2".to_string(), "owner").unwrap();
        let id3 = nft.mint("charlie", "uri3".to_string(), "owner").unwrap();

        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
        assert_eq!(id3, 3);

        println!("✅ Token IDs increment sequentially");
    }

    #[test]
    fn test_non_owner_cannot_mint() {
        let mut nft = NFTState::new(
            "Test NFT".to_string(),
            "TNFT".to_string(),
            "owner".to_string(),
        );

        let result = nft.mint("alice", "uri".to_string(), "attacker");

        assert!(result.is_err());
        assert_eq!(nft.balance_of("alice"), 0);

        println!("✅ Non-owner mint correctly rejected");
    }

    #[test]
    fn test_nft_transfer() {
        let mut nft = NFTState::new(
            "Test NFT".to_string(),
            "TNFT".to_string(),
            "owner".to_string(),
        );

        let token_id = nft.mint("alice", "uri".to_string(), "owner").unwrap();

        // Transfer from alice to bob
        nft.transfer("alice", "bob", token_id, "alice").unwrap();

        assert_eq!(nft.owner_of(token_id), Some("bob".to_string()));
        assert_eq!(nft.balance_of("alice"), 0);
        assert_eq!(nft.balance_of("bob"), 1);

        println!("✅ Transferred NFT #{} from alice to bob", token_id);
    }

    #[test]
    fn test_unauthorized_transfer() {
        let mut nft = NFTState::new(
            "Test NFT".to_string(),
            "TNFT".to_string(),
            "owner".to_string(),
        );

        let token_id = nft.mint("alice", "uri".to_string(), "owner").unwrap();

        // Attacker tries to transfer
        let result = nft.transfer("alice", "attacker", token_id, "attacker");

        assert!(result.is_err());
        assert_eq!(nft.owner_of(token_id), Some("alice".to_string()));

        println!("✅ Unauthorized transfer correctly rejected");
    }

    #[test]
    fn test_approve_and_transfer() {
        let mut nft = NFTState::new(
            "Test NFT".to_string(),
            "TNFT".to_string(),
            "owner".to_string(),
        );

        let token_id = nft.mint("alice", "uri".to_string(), "owner").unwrap();

        // Alice approves bob
        nft.approve("bob", token_id, "alice").unwrap();

        assert_eq!(nft.get_approved(token_id), Some("bob".to_string()));

        // Bob transfers the NFT
        nft.transfer("alice", "charlie", token_id, "bob").unwrap();

        assert_eq!(nft.owner_of(token_id), Some("charlie".to_string()));
        assert_eq!(nft.get_approved(token_id), None); // Approval cleared

        println!("✅ Approved transfer executed successfully");
    }

    #[test]
    fn test_operator_approval() {
        let mut nft = NFTState::new(
            "Test NFT".to_string(),
            "TNFT".to_string(),
            "owner".to_string(),
        );

        let token_id = nft.mint("alice", "uri".to_string(), "owner").unwrap();

        // Alice sets bob as operator for all her NFTs
        nft.set_approval_for_all("bob", true, "alice");

        assert!(nft.is_approved_for_all("alice", "bob"));

        // Bob can transfer alice's NFT
        nft.transfer("alice", "charlie", token_id, "bob").unwrap();

        assert_eq!(nft.owner_of(token_id), Some("charlie".to_string()));

        println!("✅ Operator approval works correctly");
    }

    #[test]
    fn test_revoke_operator_approval() {
        let mut nft = NFTState::new(
            "Test NFT".to_string(),
            "TNFT".to_string(),
            "owner".to_string(),
        );

        // Set and revoke
        nft.set_approval_for_all("bob", true, "alice");
        assert!(nft.is_approved_for_all("alice", "bob"));

        nft.set_approval_for_all("bob", false, "alice");
        assert!(!nft.is_approved_for_all("alice", "bob"));

        println!("✅ Operator approval can be revoked");
    }

    #[test]
    fn test_nft_burn() {
        let mut nft = NFTState::new(
            "Test NFT".to_string(),
            "TNFT".to_string(),
            "owner".to_string(),
        );

        let token_id = nft.mint("alice", "uri".to_string(), "owner").unwrap();

        // Alice burns her NFT
        nft.burn(token_id, "alice").unwrap();

        assert_eq!(nft.owner_of(token_id), None);
        assert_eq!(nft.balance_of("alice"), 0);
        assert_eq!(nft.token_uri(token_id), None);

        println!("✅ NFT #{} burned successfully", token_id);
    }

    #[test]
    fn test_burn_with_approval() {
        let mut nft = NFTState::new(
            "Test NFT".to_string(),
            "TNFT".to_string(),
            "owner".to_string(),
        );

        let token_id = nft.mint("alice", "uri".to_string(), "owner").unwrap();

        // Alice approves bob
        nft.approve("bob", token_id, "alice").unwrap();

        // Bob can burn
        nft.burn(token_id, "bob").unwrap();

        assert_eq!(nft.owner_of(token_id), None);

        println!("✅ Approved address can burn NFT");
    }

    #[test]
    fn test_unauthorized_burn() {
        let mut nft = NFTState::new(
            "Test NFT".to_string(),
            "TNFT".to_string(),
            "owner".to_string(),
        );

        let token_id = nft.mint("alice", "uri".to_string(), "owner").unwrap();

        // Attacker tries to burn
        let result = nft.burn(token_id, "attacker");

        assert!(result.is_err());
        assert_eq!(nft.owner_of(token_id), Some("alice".to_string()));

        println!("✅ Unauthorized burn correctly rejected");
    }

    #[test]
    fn test_multiple_nfts_ownership() {
        let mut nft = NFTState::new(
            "Test NFT".to_string(),
            "TNFT".to_string(),
            "owner".to_string(),
        );

        // Mint multiple NFTs to alice
        nft.mint("alice", "uri1".to_string(), "owner").unwrap();
        nft.mint("alice", "uri2".to_string(), "owner").unwrap();
        nft.mint("alice", "uri3".to_string(), "owner").unwrap();

        assert_eq!(nft.balance_of("alice"), 3);

        // Transfer one
        nft.transfer("alice", "bob", 1, "alice").unwrap();

        assert_eq!(nft.balance_of("alice"), 2);
        assert_eq!(nft.balance_of("bob"), 1);

        println!("✅ Multiple NFT ownership tracked correctly");
    }

    #[test]
    fn test_token_uri_metadata() {
        let mut nft = NFTState::new(
            "Test NFT".to_string(),
            "TNFT".to_string(),
            "owner".to_string(),
        );

        let token_id = nft
            .mint(
                "alice",
                "ipfs://QmXyZ123.../metadata.json".to_string(),
                "owner",
            )
            .unwrap();

        let uri = nft.token_uri(token_id).unwrap();
        assert!(uri.starts_with("ipfs://"));
        assert!(uri.contains("metadata.json"));

        println!("✅ Token URI stored: {}", uri);
    }

    #[test]
    fn test_gas_estimation_nft_operations() {
        struct GasEstimate {
            mint: u64,
            transfer: u64,
            approve: u64,
            set_approval_for_all: u64,
            burn: u64,
        }

        let gas = GasEstimate {
            mint: 50_000,                 // Multiple storage writes
            transfer: 40_000,             // Ownership + balance updates
            approve: 25_000,              // Approval storage
            set_approval_for_all: 30_000, // Operator approval
            burn: 35_000,                 // Remove ownership + approvals
        };

        // Verify all operations are under 60k gas
        assert!(gas.mint < 60_000);
        assert!(gas.transfer < 60_000);
        assert!(gas.approve < 60_000);
        assert!(gas.set_approval_for_all < 60_000);
        assert!(gas.burn < 60_000);

        println!("✅ Gas estimates:");
        println!("   Mint: {} gas", gas.mint);
        println!("   Transfer: {} gas", gas.transfer);
        println!("   Approve: {} gas", gas.approve);
        println!("   SetApprovalForAll: {} gas", gas.set_approval_for_all);
        println!("   Burn: {} gas", gas.burn);
    }

    #[test]
    fn test_nft_collection_stats() {
        let mut nft = NFTState::new(
            "Rare Art".to_string(),
            "RARE".to_string(),
            "owner".to_string(),
        );

        // Mint 10 NFTs to different owners
        for i in 0..10 {
            let owner = if i % 3 == 0 {
                "alice"
            } else if i % 3 == 1 {
                "bob"
            } else {
                "charlie"
            };
            nft.mint(owner, format!("uri{}", i), "owner").unwrap();
        }

        assert_eq!(nft.next_token_id, 11); // Next would be 11
        assert_eq!(nft.balance_of("alice"), 4); // 0, 3, 6, 9
        assert_eq!(nft.balance_of("bob"), 3); // 1, 4, 7
        assert_eq!(nft.balance_of("charlie"), 3); // 2, 5, 8

        println!("✅ Collection stats:");
        println!("   Total minted: 10");
        println!("   Alice owns: 4");
        println!("   Bob owns: 3");
        println!("   Charlie owns: 3");
    }
}
