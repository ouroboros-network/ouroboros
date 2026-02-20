# ouro-sdk

JavaScript SDK for the [Ouroboros](https://github.com/ouroboros-network/ouroboros) decentralized blockchain network.

## Install

```bash
npm install ouro-sdk
```

## Quick Start

```javascript
const { OuroClient } = require('ouro-sdk');

const client = new OuroClient('http://localhost:8000', 'your-api-key');

// Check node health
const health = await client.health();
console.log(health); // { status: 'healthy', ... }

// Get consensus state
const consensus = await client.consensus();
console.log(`View: ${consensus.view}, Leader: ${consensus.leader}`);

// Get balance
const bal = await client.balance('ouro1myaddress');
console.log(`Balance: ${bal.balance} nanoouro`);

// Submit a transaction
const result = await client.submitTransaction({
  sender: 'ouro1abc...',
  recipient: 'ouro1xyz...',
  amount: 1_000_000_000, // 1 OURO
  signature: 'hex-encoded-ed25519-signature',
});
console.log(`TX submitted: ${result.tx_id}`);

// Combined status snapshot
const status = await client.status();
console.log(`Online: ${status.online}, Block: ${status.metrics?.block_height}`);
```

## API Reference

| Method | Endpoint | Auth | Description |
|--------|----------|------|-------------|
| `health()` | GET /health | No | Node liveness |
| `identity()` | GET /identity | No | Node ID, role, uptime |
| `consensus()` | GET /consensus | No | View, leader, last block |
| `peers()` | GET /peers | No | Connected peers |
| `balance(address)` | GET /ouro/balance/:addr | No | OURO balance |
| `nonce(address)` | GET /ouro/nonce/:addr | No | Account nonce |
| `metrics()` | GET /metrics/json | Yes | TPS, sync %, mempool |
| `resources()` | GET /resources | Yes | CPU, RAM, disk |
| `mempool()` | GET /mempool | Yes | Mempool contents |
| `getTransaction(id)` | GET /tx/:id | Yes | Transaction lookup |
| `submitTransaction(tx)` | POST /tx/submit | Yes | Submit transaction |
| `transfer(from, to, amt)` | POST /ouro/transfer | Yes | Transfer OURO |
| `status()` | combined | - | Full snapshot |
| `waitForNode(ms)` | polls /health | No | Wait for startup |

## Environment Variables

- `OURO_API_KEY` — API key for protected endpoints (alternative to passing in constructor)

## Requirements

- Node.js 18+ (uses native `fetch`)
- For older Node.js: polyfill with `node-fetch`

## License

MIT
