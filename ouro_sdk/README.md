# Ouroboros Microchain SDK (Rust)

The official Rust SDK for building decentralized applications on the Ouroboros blockchain platform.

## Features

- **Microchain Management**: Create and manage application-specific blockchains
- **Transaction Handling**: Build, sign, and submit transactions
- **Balance Queries**: Check balances on mainchain and microchains
- **State Inspection**: Query microchain state, blocks, and transaction history
- **Flexible Consensus**: Choose between SingleValidator (fast) or BFT (secure)
- **Automatic Anchoring**: Inherit mainchain security through configurable anchoring
- **Type-Safe API**: Fully typed Rust interfaces with comprehensive error handling

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
ouro_sdk = { path = "../ouro_sdk" }  # Update path as needed
tokio = { version = "1", features = ["full"] }
```

## Quick Start

```rust
use ouro_sdk::{Microchain, MicrochainConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a new microchain
    let config = MicrochainConfig::new("MyDApp", "ouro1owner...");
    let mut microchain = Microchain::create(config, "http://localhost:8001").await?;

    // Transfer tokens
    let tx_id = microchain.transfer(
        "ouro1alice...",
        "ouro1bob...",
        1000
    ).await?;

    println!("Transaction: {}", tx_id);
    Ok(())
}
```

## Architecture Overview

Ouroboros uses a 3-layer architecture:

```
┌─────────────────────────────────────────┐
│         Microchains (Layer 3)           │  ← Your dApp lives here
│  - Unlimited throughput                 │  ← SDK operates here
│  - App-specific consensus               │
│  - Instant finality (local)             │
└──────────────┬──────────────────────────┘
               │ Anchor (Merkle root)
               ▼
┌─────────────────────────────────────────┐
│         Subchains (Layer 2)             │
│  - Aggregate ~1000 microchains          │
│  - Batch Merkle proofs                  │
└──────────────┬──────────────────────────┘
               │ Anchor (batch root)
               ▼
┌─────────────────────────────────────────┐
│         Mainchain (Layer 1)             │
│  - BFT consensus                        │
│  - Highest security                     │
│  - Final settlement layer               │
└─────────────────────────────────────────┘
```

**Security Model**: When you anchor your microchain, you're submitting a Merkle root to a subchain. That subchain batches ~1000 microchain anchors and submits to the mainchain. Result: 1 mainchain transaction secures ~100,000 microchain transactions.

## Core Concepts

### Microchains

Application-specific blockchains that inherit security from the mainchain through anchoring:

```rust
use ouro_sdk::{MicrochainBuilder, ConsensusType, AnchorFrequency};

// Gaming microchain - prioritize speed
let gaming = MicrochainBuilder::new("GameFi", "ouro1owner...")
    .node("http://localhost:8001")
    .block_time(2)  // 2-second blocks
    .consensus(ConsensusType::SingleValidator)
    .anchor_frequency(AnchorFrequency::EveryNSeconds(300))
    .build()
    .await?;

// DeFi microchain - prioritize security
let defi = MicrochainBuilder::new("DeFiProtocol", "ouro1owner...")
    .node("http://localhost:8001")
    .block_time(10)  // 10-second blocks
    .consensus(ConsensusType::Bft { validator_count: 7 })
    .anchor_frequency(AnchorFrequency::EveryNBlocks(50))
    .build()
    .await?;
```

### Consensus Types

**SingleValidator**:
- Fast (millisecond finality)
- Centralized (single validator)
- Ideal for: Gaming, high-frequency trading, real-time apps

**BFT (Byzantine Fault Tolerant)**:
- Slower (seconds finality)
- Decentralized (multiple validators)
- Ideal for: DeFi, DAOs, high-value applications

### Anchor Frequency

Controls how often your microchain submits state to mainchain:

- `EveryNBlocks(100)`: Anchor every 100 blocks
- `EveryNSeconds(300)`: Anchor every 5 minutes
- `Manual`: Trigger anchoring manually via `microchain.anchor()`

More frequent anchoring = higher security but higher costs.

## API Reference

### Microchain

```rust
pub struct Microchain {
    pub id: String,
    // internal fields...
}

impl Microchain {
    // Create new microchain
    pub async fn create(config: MicrochainConfig, node_url: impl Into<String>)
        -> Result<Self>

    // Connect to existing microchain
    pub async fn connect(microchain_id: impl Into<String>, node_url: impl Into<String>)
        -> Result<Self>

    // Get current state
    pub async fn state(&self) -> Result<MicrochainState>

    // Check balance for address
    pub async fn balance(&self, address: &str) -> Result<u64>

    // Transfer tokens (simplified)
    pub async fn transfer(&mut self, from: &str, to: &str, amount: u64)
        -> Result<String>

    // Submit custom transaction
    pub async fn submit_tx(&mut self, tx: &Transaction) -> Result<String>

    // Get transaction builder
    pub fn tx(&self) -> TransactionBuilder

    // Anchor to mainchain
    pub async fn anchor(&self) -> Result<String>

    // Query transaction history
    pub async fn tx_history(&self, from: u64, to: u64)
        -> Result<Vec<Transaction>>

    // Get recent blocks
    pub async fn blocks(&self, limit: u32) -> Result<Vec<BlockHeader>>
}
```

### MicrochainConfig

```rust
pub struct MicrochainConfig {
    pub name: String,
    pub owner: String,
    pub consensus: ConsensusType,
    pub anchor_frequency: AnchorFrequency,
    pub max_txs_per_block: u32,
    pub block_time_secs: u64,
}

impl MicrochainConfig {
    pub fn new(name: impl Into<String>, owner: impl Into<String>) -> Self
    pub fn with_consensus(mut self, consensus: ConsensusType) -> Self
    pub fn with_anchor_frequency(mut self, frequency: AnchorFrequency) -> Self
    pub fn with_block_time(mut self, seconds: u64) -> Self
}
```

### Transaction

```rust
pub struct Transaction {
    pub id: String,
    pub from: String,
    pub to: String,
    pub amount: u64,
    pub nonce: u64,
    pub signature: String,
    pub data: Option<serde_json::Value>,
    pub timestamp: Option<String>,
}

impl Transaction {
    pub fn new(from: impl Into<String>, to: impl Into<String>, amount: u64) -> Self
    pub fn with_nonce(mut self, nonce: u64) -> Self
    pub fn with_data(mut self, data: serde_json::Value) -> Self
    pub fn sign(&mut self, keypair: &Keypair) -> Result<()>
    pub fn sign_with_key(&mut self, private_key_hex: &str) -> Result<()>
}
```

### TransactionBuilder

```rust
pub struct TransactionBuilder;

impl TransactionBuilder {
    pub fn new() -> Self
    pub fn from(mut self, from: impl Into<String>) -> Self
    pub fn to(mut self, to: impl Into<String>) -> Self
    pub fn amount(mut self, amount: u64) -> Self
    pub fn nonce(mut self, nonce: u64) -> Self
    pub fn data(mut self, data: serde_json::Value) -> Self
    pub fn build(self) -> Result<Transaction>
}
```

### OuroClient

Low-level client for direct node interaction:

```rust
pub struct OuroClient;

impl OuroClient {
    pub fn new(node_url: impl Into<String>) -> Self
    pub async fn get_balance(&self, address: &str) -> Result<Balance>
    pub async fn get_microchain_balance(&self, microchain_id: &str, address: &str)
        -> Result<u64>
    pub async fn submit_transaction(&self, tx: &Transaction) -> Result<String>
    pub async fn create_microchain(&self, config: &MicrochainConfig) -> Result<String>
    pub async fn get_microchain_state(&self, microchain_id: &str)
        -> Result<MicrochainState>
    pub async fn list_microchains(&self) -> Result<Vec<MicrochainState>>
    pub async fn anchor_microchain(&self, microchain_id: &str) -> Result<String>
    pub async fn health_check(&self) -> Result<bool>
}
```

## Examples

### Basic Microchain Operations

See `examples/basic_microchain.rs`:

```bash
cargo run --example basic_microchain
```

### Advanced Features

See `examples/advanced_microchain.rs`:

```bash
cargo run --example advanced_microchain
```

### Builder Pattern

See `examples/builder_pattern.rs`:

```bash
cargo run --example builder_pattern
```

### Client Usage

See `examples/client_usage.rs`:

```bash
cargo run --example client_usage
```

## Use Cases

### Gaming

```rust
// High-speed gaming microchain
let gaming = MicrochainBuilder::new("MyGame", "ouro1gamedev...")
    .node("http://localhost:8001")
    .block_time(2)  // 2-second blocks for responsive gameplay
    .consensus(ConsensusType::SingleValidator)
    .anchor_frequency(AnchorFrequency::EveryNSeconds(600))  // Anchor every 10 min
    .build()
    .await?;

// Process in-game transactions instantly
gaming.transfer("ouro1player1...", "ouro1player2...", 100).await?;
```

### DeFi

```rust
// High-security DeFi microchain
let defi = MicrochainBuilder::new("LendingProtocol", "ouro1defi...")
    .node("http://localhost:8001")
    .block_time(10)  // 10-second blocks for stability
    .consensus(ConsensusType::Bft { validator_count: 7 })
    .anchor_frequency(AnchorFrequency::EveryNBlocks(50))  // Frequent anchoring
    .build()
    .await?;

// Submit smart contract call
let mut tx = defi.tx()
    .from("ouro1user...")
    .to("ouro1contract...")
    .amount(10000)
    .data(json!({
        "method": "deposit",
        "collateral_ratio": 150
    }))
    .build()?;

tx.sign_with_key("private_key")?;
defi.submit_tx(&tx).await?;
```

### NFT Marketplace

```rust
let nft = MicrochainBuilder::new("NFTMarket", "ouro1nft...")
    .node("http://localhost:8001")
    .block_time(5)
    .consensus(ConsensusType::SingleValidator)
    .build()
    .await?;

// Mint NFT
let mut mint_tx = nft.tx()
    .from("ouro1artist...")
    .to("ouro1marketplace...")
    .amount(0)
    .data(json!({
        "action": "mint",
        "token_id": "unique_nft_123",
        "metadata": "ipfs://Qm...",
        "royalty": 5
    }))
    .build()?;

mint_tx.sign_with_key("artist_key")?;
nft.submit_tx(&mint_tx).await?;
```

### DAO Governance

```rust
let dao = MicrochainBuilder::new("CommunityDAO", "ouro1dao...")
    .node("http://localhost:8001")
    .consensus(ConsensusType::Bft { validator_count: 5 })
    .anchor_frequency(AnchorFrequency::Manual)  // Anchor after each proposal
    .build()
    .await?;

// Submit governance proposal
let mut proposal = dao.tx()
    .from("ouro1member...")
    .to("ouro1governance...")
    .amount(0)
    .data(json!({
        "action": "create_proposal",
        "title": "Upgrade Protocol",
        "description": "...",
        "voting_period": 7  // days
    }))
    .build()?;

proposal.sign_with_key("member_key")?;
dao.submit_tx(&proposal).await?;

// Manually anchor after voting concludes
dao.anchor().await?;
```

## Error Handling

The SDK uses the `Result<T, SdkError>` type for error handling:

```rust
use ouro_sdk::SdkError;

match microchain.transfer("alice", "bob", 1000).await {
    Ok(tx_id) => println!("Success: {}", tx_id),
    Err(SdkError::InsufficientBalance { required, available }) => {
        println!("Insufficient balance: need {}, have {}", required, available);
    }
    Err(SdkError::TransactionFailed(msg)) => {
        println!("Transaction failed: {}", msg);
    }
    Err(SdkError::Network(e)) => {
        println!("Network error: {}", e);
    }
    Err(e) => {
        println!("Other error: {}", e);
    }
}
```

## Testing

Run the SDK tests:

```bash
cd ouro_sdk
cargo test
```

Run examples:

```bash
cargo run --example basic_microchain
cargo run --example advanced_microchain
cargo run --example builder_pattern
cargo run --example client_usage
```

## Roadmap

- Core SDK implementation (Rust) - Complete
- JavaScript/TypeScript SDK - In Progress
- Python SDK bindings - In Progress
- Smart contract support - Planned
- WebAssembly compilation - Planned
- GraphQL query interface - Planned

## Contributing

This SDK is part of the Ouroboros blockchain project. For issues and contributions, visit the main repository.

## License

MIT License - see LICENSE file for details

## Support

- Documentation: https://docs.ouroboros.network (coming soon)
- Discord: https://discord.gg/ouroboros (coming soon)
- GitHub: https://github.com/ouroboros-network

## Version

Current version: 0.1.0 (Alpha)

This SDK is under active development. APIs may change before 1.0.0 release.
