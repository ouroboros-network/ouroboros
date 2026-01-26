# Subchain Guide

Build high-scale business applications on Ouroboros using Subchains.

## Overview

Subchains are dedicated blockchain environments designed for:
- **Financial Infrastructure**: Money transfer services, payment processors
- **High-Throughput Services**: Oracles, bridges, aggregators
- **Enterprise Applications**: Business apps with dedicated resources

### Subchains vs Microchains

| Feature | Subchain | Microchain |
|---------|----------|------------|
| **Use Case** | Business infrastructure | dApps, games, NFTs |
| **Deposit** | 5,000 OURO minimum | Free |
| **Resources** | Dedicated | Shared |
| **Validators** | Custom validator set | Inherited |
| **Scale** | 1M+ users | Smaller apps |
| **Rent** | 0.01 OURO/block | None |

**Choose Subchain when**: Building a service with real money, high stakes, or 1M+ users.
**Choose Microchain when**: Building games, NFT projects, or experimental dApps.

## Requirements

- **Minimum Deposit**: 5,000 OURO (500,000,000,000 base units)
- **Rent**: 0.01 OURO per block (~86.4 OURO/day at 1 block/sec)
- **Running Node**: Connected to Ouroboros mainnet

## Quick Start

### JavaScript/TypeScript

```bash
npm install @ouro/sdk
```

```typescript
import { SubchainBuilder, Subchain, MIN_SUBCHAIN_DEPOSIT } from '@ouro/sdk';

// Register a new subchain
const subchain = await new SubchainBuilder('Hermes', 'ouro1owner...')
  .node('http://localhost:8001')
  .deposit(1_000_000_000_000)  // 10,000 OURO
  .anchorFrequency(50)
  .validator('validator1_pubkey', 100_000_000_000)
  .build();

console.log(`Subchain registered: ${subchain.id}`);

// Check status
const status = await subchain.status();
console.log(`Blocks remaining: ${status.blocksRemaining}`);
```

### Python

```bash
pip install ouro-sdk
```

```python
from ouro_sdk import SubchainBuilder, Subchain, MIN_SUBCHAIN_DEPOSIT

# Register a new subchain
subchain = SubchainBuilder("Hermes", "ouro1owner...") \
    .node("http://localhost:8001") \
    .deposit(1_000_000_000_000)  # 10,000 OURO
    .anchor_frequency(50) \
    .validator("validator1_pubkey", 100_000_000_000) \
    .build()

print(f"Subchain registered: {subchain.id}")

# Check status
status = subchain.status()
print(f"Blocks remaining: {status.blocks_remaining}")
```

### Rust

```toml
[dependencies]
ouro_sdk = "0.4"
```

```rust
use ouro_sdk::{SubchainBuilder, SubchainConfig, MIN_SUBCHAIN_DEPOSIT};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Register a new subchain
    let subchain = SubchainBuilder::new("Hermes", "ouro1owner...")
        .node("http://localhost:8001")
        .deposit(1_000_000_000_000)  // 10,000 OURO
        .anchor_frequency(50)
        .validator("validator1_pubkey", 500_000_000_000)
        .build()
        .await?;

    println!("Subchain registered: {}", subchain.id);

    // Check status
    let status = subchain.status().await?;
    println!("Blocks remaining: {}", status.blocks_remaining);

    Ok(())
}
```

## Core Operations

### Connecting to an Existing Subchain

```typescript
// JavaScript
const subchain = await Subchain.connect('hermes-subchain-id', 'http://localhost:8001');
```

```python
# Python
subchain = Subchain.connect("hermes-subchain-id", "http://localhost:8001")
```

```rust
// Rust
let subchain = Subchain::connect("hermes-subchain-id", "http://localhost:8001").await?;
```

### Checking Status and Rent

```typescript
const status = await subchain.status();

console.log(`State: ${status.state}`);           // active, grace_period, terminated
console.log(`Deposit: ${status.depositBalance}`);
console.log(`Blocks remaining: ${status.blocksRemaining}`);
console.log(`Block height: ${status.blockHeight}`);
console.log(`Total transactions: ${status.txCount}`);
console.log(`Validators: ${status.validatorCount}`);
```

### Topping Up Rent

Keep your subchain running by adding to the deposit:

```typescript
// Add 1,000 OURO to deposit
const txId = await subchain.topUpRent(100_000_000_000);
console.log(`Top-up transaction: ${txId}`);
```

### Submitting Transactions

```typescript
// Simple transfer
const txId = await subchain.transfer(
  'ouro1sender...',
  'ouro1receiver...',
  1_000_000_000  // 10 OURO
);

// Using transaction builder
const tx = subchain.tx()
  .from('ouro1sender...')
  .to('ouro1receiver...')
  .amount(1_000_000_000)
  .memo('Payment for services')
  .build();

const txId = await subchain.submitTx(tx);
```

### Anchoring to Mainchain

Anchoring creates a cryptographic proof of your subchain state on the mainchain:

```typescript
const txId = await subchain.anchor();
console.log(`Anchor transaction: ${txId}`);
```

Anchoring happens automatically based on `anchorFrequency`, but you can trigger it manually.

### Managing Validators

```typescript
// Add a validator
await subchain.addValidator({
  pubkey: 'validator2_pubkey',
  stake: 100_000_000_000,
  endpoint: 'https://validator2.example.com:9001'
});

// List validators
const validators = await subchain.validators();
for (const v of validators) {
  console.log(`${v.pubkey}: ${v.stake} stake`);
}

// Remove a validator
await subchain.removeValidator('validator2_pubkey');
```

### Transaction History

```typescript
// Get transactions from blocks 1000 to 2000
const txs = await subchain.txHistory(1000, 2000);
for (const tx of txs) {
  console.log(`${tx.from} -> ${tx.to}: ${tx.amount}`);
}
```

## Subchain States

| State | Description |
|-------|-------------|
| `active` | Normal operation, rent being deducted |
| `grace_period` | Rent depleted, limited functionality (typically 1000 blocks) |
| `terminated` | Subchain stopped, deposit can be withdrawn |

## Cost Calculator

```
Daily cost = blocks_per_day * RENT_RATE_PER_BLOCK
           = 86,400 * 0.01 OURO
           = 864 OURO/day (at 1 block/second)

Monthly cost ~= 25,920 OURO
Yearly cost  ~= 315,360 OURO

Minimum deposit (5,000 OURO) lasts ~5.8 days
```

Adjust `anchorFrequency` to balance cost vs security:
- Higher frequency = more secure, higher cost
- Lower frequency = less secure, lower cost

## Example: Money Transfer Service (Hermes)

```typescript
import { SubchainBuilder, Subchain } from '@ouro/sdk';

class HermesService {
  private subchain: Subchain;

  async initialize() {
    // Connect to existing subchain or register new one
    try {
      this.subchain = await Subchain.connect(
        'hermes-main',
        'http://localhost:8001'
      );
    } catch {
      this.subchain = await new SubchainBuilder('hermes-main', 'ouro1hermes...')
        .node('http://localhost:8001')
        .deposit(10_000_000_000_000)  // 100,000 OURO (~115 days)
        .anchorFrequency(100)
        .validator('hermes-val-1', 1_000_000_000_000)
        .validator('hermes-val-2', 1_000_000_000_000)
        .validator('hermes-val-3', 1_000_000_000_000)
        .build();
    }
  }

  async sendMoney(from: string, to: string, amount: number): Promise<string> {
    return this.subchain.transfer(from, to, amount);
  }

  async getBalance(address: string): Promise<number> {
    return this.subchain.balance(address);
  }

  async checkHealth(): Promise<void> {
    const status = await this.subchain.status();

    // Alert if running low on rent
    if (status.blocksRemaining < 86400) {  // Less than 1 day
      console.warn('Low rent! Topping up...');
      await this.subchain.topUpRent(10_000_000_000_000);
    }
  }
}

// Usage
const hermes = new HermesService();
await hermes.initialize();

const txId = await hermes.sendMoney(
  'ouro1alice...',
  'ouro1bob...',
  100_000_000_000  // 1,000 OURO
);
console.log(`Transfer complete: ${txId}`);
```

## API Reference

### SubchainBuilder

| Method | Description |
|--------|-------------|
| `new SubchainBuilder(name, owner)` | Create builder |
| `.node(url)` | Set node URL |
| `.deposit(amount)` | Set initial deposit |
| `.anchorFrequency(blocks)` | Set anchor frequency |
| `.rpcEndpoint(url)` | Set RPC endpoint |
| `.validator(pubkey, stake, endpoint?)` | Add validator |
| `.build()` | Register subchain |

### Subchain

| Method | Description |
|--------|-------------|
| `Subchain.connect(id, nodeUrl)` | Connect to existing |
| `Subchain.register(config, nodeUrl)` | Register new |
| `.status()` | Get status |
| `.depositBalance()` | Get deposit balance |
| `.blocksRemaining()` | Get blocks remaining |
| `.topUpRent(amount)` | Add to deposit |
| `.balance(address)` | Get address balance |
| `.submitTx(tx)` | Submit transaction |
| `.transfer(from, to, amount)` | Simple transfer |
| `.anchor()` | Anchor to mainchain |
| `.txHistory(from, to)` | Get transaction history |
| `.addValidator(config)` | Add validator |
| `.removeValidator(pubkey)` | Remove validator |
| `.validators()` | List validators |
| `.withdrawDeposit()` | Withdraw (after termination) |

## Troubleshooting

### "Insufficient deposit"
Your deposit is below the minimum 5,000 OURO. Ensure you have enough balance.

### "Subchain in grace period"
Rent has run out. Top up immediately with `topUpRent()` to restore full functionality.

### "Subchain terminated"
The subchain has been terminated. You can only withdraw remaining deposit.

### "Validator not found"
The validator pubkey doesn't exist. Check the pubkey format.

## Next Steps

- [Architecture Overview](./ARCHITECTURE.md)
- [Microchain Guide](./MICROCHAIN_GUIDE.md) - For smaller dApps
- [API Reference](./API_REFERENCE.md)
- [Examples](../examples/)
