use ouro_sdk::{
    MicrochainBuilder, ConsensusType, AnchorFrequency, Transaction
};
use serde_json::json;

/// Complete dApp Example: A decentralized marketplace
///
/// This example demonstrates a full workflow for building a dApp on Ouroboros:
/// 1. Creating a microchain for your application
/// 2. Setting up user accounts
/// 3. Handling complex transactions with custom data
/// 4. Querying state and balances
/// 5. Anchoring for security

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🏪 Ouroboros SDK - Complete dApp Example: Marketplace\n");
    println!("{}", "=".repeat(60));
    println!();

    // Step 1: Deploy the marketplace microchain
    println!("📦 Step 1: Deploying Marketplace Microchain");
    println!("{}", "-".repeat(60));

    let mut marketplace = MicrochainBuilder::new("DecentralizedMarket", "ouro1marketplace_owner...")
        .node("http://localhost:8001")
        .block_time(5)  // 5-second blocks
        .consensus(ConsensusType::SingleValidator)  // Fast for good UX
        .anchor_frequency(AnchorFrequency::EveryNBlocks(100))  // Anchor every 500 seconds
        .build()
        .await?;

    println!("✅ Marketplace deployed!");
    println!("   Microchain ID: {}", marketplace.id);
    println!("   Block time: 5 seconds");
    println!("   Consensus: SingleValidator (fast)");
    println!();

    // Step 2: Setup - Create initial liquidity
    println!("💰 Step 2: Initial Setup - Creating Liquidity");
    println!("{}", "-".repeat(60));

    let seller = "ouro1seller_alice...";
    let buyer = "ouro1buyer_bob...";
    let marketplace_contract = "ouro1marketplace_contract...";

    println!("   Checking initial balances...");
    let seller_balance = marketplace.balance(seller).await.unwrap_or(0);
    let buyer_balance = marketplace.balance(buyer).await.unwrap_or(0);

    println!("   Seller balance: {} tokens", seller_balance);
    println!("   Buyer balance: {} tokens", buyer_balance);
    println!();

    // Step 3: Seller lists an item
    println!("📝 Step 3: Seller Lists Item for Sale");
    println!("{}", "-".repeat(60));

    let mut list_tx = marketplace.tx()
        .from(seller)
        .to(marketplace_contract)
        .amount(0)  // No tokens transferred, just a contract call
        .data(json!({
            "action": "list_item",
            "item": {
                "id": "item_12345",
                "name": "Rare Digital Art",
                "description": "Limited edition NFT artwork",
                "price": 1000,
                "seller": seller,
                "metadata": {
                    "image": "ipfs://Qm...",
                    "attributes": ["rare", "digital_art", "limited"]
                }
            }
        }))
        .build()?;

    // In production, sign with seller's private key
    // list_tx.sign_with_key("seller_private_key")?;
    list_tx.signature = "mock_seller_signature".to_string();

    let list_tx_id = marketplace.submit_tx(&list_tx).await?;
    println!("✅ Item listed!");
    println!("   Transaction ID: {}", list_tx_id);
    println!("   Item ID: item_12345");
    println!("   Price: 1000 tokens");
    println!();

    // Step 4: Buyer purchases the item
    println!("🛒 Step 4: Buyer Purchases Item");
    println!("{}", "-".repeat(60));

    let mut purchase_tx = marketplace.tx()
        .from(buyer)
        .to(marketplace_contract)
        .amount(1000)  // Payment for the item
        .data(json!({
            "action": "purchase_item",
            "item_id": "item_12345",
            "buyer": buyer,
            "payment_method": "ouro_tokens"
        }))
        .build()?;

    // Sign with buyer's key
    // purchase_tx.sign_with_key("buyer_private_key")?;
    purchase_tx.signature = "mock_buyer_signature".to_string();

    let purchase_tx_id = marketplace.submit_tx(&purchase_tx).await?;
    println!("✅ Purchase successful!");
    println!("   Transaction ID: {}", purchase_tx_id);
    println!("   Buyer: {}", buyer);
    println!("   Amount: 1000 tokens");
    println!();

    // Step 5: Transfer item ownership
    println!("🔄 Step 5: Transfer Item Ownership");
    println!("{}", "-".repeat(60));

    let mut transfer_tx = marketplace.tx()
        .from(marketplace_contract)
        .to(buyer)
        .amount(0)
        .data(json!({
            "action": "transfer_ownership",
            "item_id": "item_12345",
            "from": seller,
            "to": buyer,
            "transfer_type": "sale"
        }))
        .build()?;

    transfer_tx.signature = "mock_contract_signature".to_string();
    let transfer_tx_id = marketplace.submit_tx(&transfer_tx).await?;
    println!("✅ Ownership transferred!");
    println!("   Transaction ID: {}", transfer_tx_id);
    println!("   New owner: {}", buyer);
    println!();

    // Step 6: Pay seller (minus marketplace fee)
    println!("💸 Step 6: Payment to Seller");
    println!("{}", "-".repeat(60));

    let marketplace_fee = 50;  // 5% fee
    let seller_payment = 950;  // 95% to seller

    let payment_tx_id = marketplace.transfer(
        marketplace_contract,
        seller,
        seller_payment
    ).await?;

    println!("✅ Seller paid!");
    println!("   Transaction ID: {}", payment_tx_id);
    println!("   Seller received: {} tokens", seller_payment);
    println!("   Marketplace fee: {} tokens", marketplace_fee);
    println!();

    // Step 7: Query transaction history
    println!("📜 Step 7: Query Transaction History");
    println!("{}", "-".repeat(60));

    let history = marketplace.tx_history(0, 100).await?;
    println!("   Total transactions: {}", history.len());
    println!("   Recent transactions:");
    for (i, tx) in history.iter().take(5).enumerate() {
        println!("   {}. TX {} ({} -> {}): {} tokens",
                 i + 1,
                 &tx.id[..8],
                 &tx.from[..15],
                 &tx.to[..15],
                 tx.amount);
    }
    println!();

    // Step 8: Check microchain state
    println!("📊 Step 8: Check Microchain State");
    println!("{}", "-".repeat(60));

    let state = marketplace.state().await?;
    println!("   Name: {}", state.name);
    println!("   Owner: {}", state.owner);
    println!("   Block Height: {}", state.block_height);
    println!("   Total Transactions: {}", state.tx_count);
    if let Some(anchor) = state.last_anchor_height {
        println!("   Last Anchor: Block #{}", anchor);
    }
    println!();

    // Step 9: Anchor to mainchain for security
    println!("⚓ Step 9: Anchor to Mainchain");
    println!("{}", "-".repeat(60));
    println!("   Anchoring ensures all marketplace transactions are");
    println!("   secured by the Ouroboros mainchain's BFT consensus.");
    println!();

    let anchor_id = marketplace.anchor().await?;
    println!("✅ Anchored to mainchain!");
    println!("   Anchor ID: {}", anchor_id);
    println!("   Security: All transactions now inherit mainchain security");
    println!();

    // Step 10: Query blocks
    println!("🧱 Step 10: Query Recent Blocks");
    println!("{}", "-".repeat(60));

    let blocks = marketplace.blocks(5).await?;
    println!("   Recent {} blocks:", blocks.len());
    for block in &blocks {
        println!("   Block #{}: {} txs (hash: {}...)",
                 block.height,
                 block.tx_count,
                 &block.hash[..16]);
    }
    println!();

    // Summary
    println!("{}", "=".repeat(60));
    println!("🎉 Marketplace dApp Demonstration Complete!");
    println!("{}", "=".repeat(60));
    println!();
    println!("Summary:");
    println!("  ✅ Created marketplace microchain");
    println!("  ✅ Listed item for sale");
    println!("  ✅ Processed purchase transaction");
    println!("  ✅ Transferred ownership");
    println!("  ✅ Distributed payment to seller");
    println!("  ✅ Anchored to mainchain for security");
    println!();
    println!("Key Advantages of Ouroboros Microchains:");
    println!("  🚀 Fast: 5-second block time, instant user feedback");
    println!("  💰 Low Cost: Minimal fees on microchain layer");
    println!("  🔒 Secure: Inherits mainchain security via anchoring");
    println!("  📈 Scalable: Unlimited throughput per microchain");
    println!("  🎯 Flexible: Custom consensus and configuration");
    println!();

    Ok(())
}
