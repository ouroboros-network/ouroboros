# Ouroboros Microchain SDK (JavaScript/TypeScript)

The official JavaScript/TypeScript SDK for building decentralized applications on the Ouroboros blockchain platform.

## Features

- **TypeScript First**: Full type safety with comprehensive TypeScript definitions
- **Promise-Based**: Modern async/await API for all operations
- **Built-in Signing**: Ed25519 signature support via TweetNaCl
- **Microchain Management**: Create and manage application-specific blockchains
- **Balance Queries**: Check balances on mainchain and microchains
- **State Inspection**: Query microchain state, blocks, and transaction history
- **Flexible Consensus**: Choose between SingleValidator (fast) or BFT (secure)
- **Automatic Anchoring**: Inherit mainchain security through configurable anchoring

## Installation

```bash
npm install @ouroboros/sdk
# or
yarn add @ouroboros/sdk
# or
pnpm add @ouroboros/sdk
```

## Quick Start

```typescript
import { Microchain } from '@ouroboros/sdk';

// Create a new microchain
const microchain = await Microchain.create(
  {
    name: 'MyDApp',
    owner: 'ouro1owner...',
  },
  'http://localhost:8001'
);

// Transfer tokens
const txId = await microchain.transfer(
  'ouro1alice...',
  'ouro1bob...',
  1000
);

console.log('Transaction:', txId);
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

**Security Model**: When you anchor your microchain, you submit a Merkle root to a subchain. That subchain batches ~1000 microchain anchors and submits to mainchain. Result: 1 mainchain transaction secures ~100,000 microchain transactions.

## Usage

### Creating Microchains

```typescript
import { MicrochainBuilder, ConsensusType } from '@ouroboros/sdk';

// Gaming microchain - prioritize speed
const gaming = await new MicrochainBuilder('GameFi', 'ouro1owner...')
  .node('http://localhost:8001')
  .blockTime(2) // 2-second blocks
  .consensus(ConsensusType.SingleValidator)
  .anchorFrequency({ type: 'seconds', count: 300 })
  .build();

// DeFi microchain - prioritize security
const defi = await new MicrochainBuilder('DeFiProtocol', 'ouro1owner...')
  .node('http://localhost:8001')
  .blockTime(10) // 10-second blocks
  .consensus(ConsensusType.Bft, 7) // 7 validators
  .anchorFrequency({ type: 'blocks', count: 50 })
  .build();
```

### Sending Transactions

```typescript
import { Transaction } from '@ouroboros/sdk';

// Simple transfer
const txId = await microchain.transfer(
  'ouro1alice...',
  'ouro1bob...',
  1000
);

// Custom transaction with data
const tx = microchain
  .tx()
  .setFrom('ouro1alice...')
  .setTo('ouro1contract...')
  .setAmount(500)
  .setData({
    method: 'mint_nft',
    params: {
      token_id: '12345',
      metadata: 'ipfs://Qm...',
    },
  })
  .build();

// Sign and submit
tx.sign('private_key_hex');
const customTxId = await microchain.submitTx(tx);
```

### Querying State

```typescript
// Get microchain state
const state = await microchain.state();
console.log(`Block height: ${state.blockHeight}`);
console.log(`Total transactions: ${state.txCount}`);

// Check balance
const balance = await microchain.balance('ouro1alice...');
console.log(`Balance: ${balance} tokens`);

// Get transaction history
const txs = await microchain.txHistory(0, 100);
for (const tx of txs) {
  console.log(`${tx.from} -> ${tx.to}: ${tx.amount}`);
}

// Get recent blocks
const blocks = await microchain.blocks(10);
for (const block of blocks) {
  console.log(`Block #${block.height}: ${block.txCount} txs`);
}
```

### Anchoring

```typescript
// Manual anchor
const anchorId = await microchain.anchor();
console.log(`Anchored: ${anchorId}`);

// Automatic anchoring (configured during creation)
const autoAnchor = await new MicrochainBuilder('AutoDApp', 'ouro1owner...')
  .node('http://localhost:8001')
  .anchorFrequency({ type: 'blocks', count: 100 }) // Every 100 blocks
  .build();
```

## API Reference

### Microchain

```typescript
class Microchain {
  // Create new microchain
  static async create(config: MicrochainConfig, nodeUrl: string): Promise<Microchain>

  // Connect to existing microchain
  static async connect(microchainId: string, nodeUrl: string): Promise<Microchain>

  // Get current state
  async state(): Promise<MicrochainState>

  // Check balance
  async balance(address: string): Promise<number>

  // Transfer tokens (simplified)
  async transfer(from: string, to: string, amount: number): Promise<string>

  // Submit custom transaction
  async submitTx(tx: Transaction): Promise<string>

  // Get transaction builder
  tx(): TransactionBuilder

  // Anchor to mainchain
  async anchor(): Promise<string>

  // Query transaction history
  async txHistory(from: number, to: number): Promise<TransactionData[]>

  // Get recent blocks
  async blocks(limit: number): Promise<BlockHeader[]>
}
```

### MicrochainBuilder

```typescript
class MicrochainBuilder {
  constructor(name: string, owner: string)

  node(url: string): this
  consensus(type: ConsensusType, validatorCount?: number): this
  anchorFrequency(frequency: AnchorFrequency): this
  blockTime(seconds: number): this

  async build(): Promise<Microchain>
}
```

### Transaction

```typescript
class Transaction {
  constructor(from: string, to: string, amount: number)

  withNonce(nonce: number): this
  withData(data: Record<string, any>): this
  sign(privateKeyHex: string): this

  toJSON(): TransactionData
  static fromJSON(data: TransactionData): Transaction
}
```

### TransactionBuilder

```typescript
class TransactionBuilder {
  setFrom(from: string): this
  setTo(to: string): this
  setAmount(amount: number): this
  setNonce(nonce: number): this
  setData(data: Record<string, any>): this

  build(): Transaction
}
```

### OuroClient

```typescript
class OuroClient {
  constructor(nodeUrl: string)

  async getBalance(address: string): Promise<Balance>
  async getMicrochainBalance(microchainId: string, address: string): Promise<number>
  async submitTransaction(tx: TransactionData): Promise<string>
  async getTransactionStatus(txId: string): Promise<TxStatus>
  async createMicrochain(config: MicrochainConfig): Promise<string>
  async getMicrochainState(microchainId: string): Promise<MicrochainState>
  async listMicrochains(): Promise<MicrochainState[]>
  async anchorMicrochain(microchainId: string): Promise<string>
  async healthCheck(): Promise<boolean>
}
```

## Types

### ConsensusType

```typescript
enum ConsensusType {
  SingleValidator = 'single_validator', // Fast, centralized
  Bft = 'bft',                          // Slower, decentralized
}
```

### AnchorFrequency

```typescript
type AnchorFrequency =
  | { type: 'blocks'; count: number }    // Every N blocks
  | { type: 'seconds'; count: number }   // Every N seconds
  | { type: 'manual' }                   // Manual only
```

### MicrochainConfig

```typescript
interface MicrochainConfig {
  name: string;
  owner: string;
  consensus?: {
    type: ConsensusType;
    validatorCount?: number;
  };
  anchorFrequency?: AnchorFrequency;
  maxTxsPerBlock?: number;
  blockTimeSecs?: number;
}
```

## Examples

### Gaming dApp

```typescript
// High-speed gaming microchain
const gaming = await new MicrochainBuilder('MyGame', 'ouro1gamedev...')
  .node('http://localhost:8001')
  .blockTime(2) // 2-second blocks for responsive gameplay
  .consensus(ConsensusType.SingleValidator)
  .anchorFrequency({ type: 'seconds', count: 600 }) // Anchor every 10 min
  .build();

// Process in-game transactions instantly
await gaming.transfer('ouro1player1...', 'ouro1player2...', 100);
```

### DeFi Protocol

```typescript
// High-security DeFi microchain
const defi = await new MicrochainBuilder('LendingProtocol', 'ouro1defi...')
  .node('http://localhost:8001')
  .blockTime(10)
  .consensus(ConsensusType.Bft, 7)
  .anchorFrequency({ type: 'blocks', count: 50 })
  .build();

// Submit smart contract call
const tx = defi
  .tx()
  .setFrom('ouro1user...')
  .setTo('ouro1contract...')
  .setAmount(10000)
  .setData({
    method: 'deposit',
    collateralRatio: 150,
  })
  .build();

tx.sign('private_key');
await defi.submitTx(tx);
```

### NFT Marketplace

```typescript
const nft = await new MicrochainBuilder('NFTMarket', 'ouro1nft...')
  .node('http://localhost:8001')
  .blockTime(5)
  .consensus(ConsensusType.SingleValidator)
  .build();

// Mint NFT
const mintTx = nft
  .tx()
  .setFrom('ouro1artist...')
  .setTo('ouro1marketplace...')
  .setAmount(0)
  .setData({
    action: 'mint',
    tokenId: 'unique_nft_123',
    metadata: 'ipfs://Qm...',
    royalty: 5,
  })
  .build();

mintTx.sign('artist_key');
await nft.submitTx(mintTx);
```

## Error Handling

```typescript
import {
  TransactionFailedError,
  InsufficientBalanceError,
  NetworkError,
} from '@ouroboros/sdk';

try {
  await microchain.transfer('alice', 'bob', 1000);
} catch (error) {
  if (error instanceof InsufficientBalanceError) {
    console.log(`Need ${error.required}, have ${error.available}`);
  } else if (error instanceof TransactionFailedError) {
    console.log('Transaction failed:', error.message);
  } else if (error instanceof NetworkError) {
    console.log('Network error:', error.message);
  }
}
```

## Development

```bash
# Install dependencies
npm install

# Build
npm run build

# Run tests
npm test

# Lint
npm run lint

# Watch mode
npm run dev
```

## Running Examples

```bash
# Build first
npm run build

# Run examples with ts-node
npx ts-node examples/basic-microchain.ts
npx ts-node examples/advanced-microchain.ts
npx ts-node examples/builder-pattern.ts
npx ts-node examples/client-usage.ts
```

## Browser Usage

The SDK works in both Node.js and browser environments:

```typescript
import { Microchain } from '@ouroboros/sdk';

// Same API in browser
const microchain = await Microchain.create(
  { name: 'WebDApp', owner: 'ouro1...' },
  'https://node.ouroboros.network'
);
```

## React Integration

```typescript
import { useState, useEffect } from 'react';
import { Microchain } from '@ouroboros/sdk';

function useMicrochain(microchainId: string, nodeUrl: string) {
  const [microchain, setMicrochain] = useState<Microchain | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    Microchain.connect(microchainId, nodeUrl)
      .then(setMicrochain)
      .finally(() => setLoading(false));
  }, [microchainId, nodeUrl]);

  return { microchain, loading };
}

// Usage
function MyComponent() {
  const { microchain, loading } = useMicrochain('mc_123', 'http://localhost:8001');

  const sendTokens = async () => {
    if (!microchain) return;
    const txId = await microchain.transfer('alice', 'bob', 100);
    console.log('Sent:', txId);
  };

  return loading ? <div>Loading...</div> : <button onClick={sendTokens}>Send</button>;
}
```

## Roadmap

- Core SDK implementation (TypeScript) - Complete
- React hooks package - In Progress
- Vue composables package - In Progress
- WebSocket real-time updates - Planned
- Smart contract support - Planned
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

Current version: 0.3.0 (Alpha)

This SDK is under active development. APIs may change before 1.0.0 release.
