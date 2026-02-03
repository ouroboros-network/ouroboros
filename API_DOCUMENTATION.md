# Ouroboros API Documentation

Complete REST API reference for interacting with Ouroboros nodes.

**Base URL**: `http://localhost:8000` (local node) or `http://136.112.101.176:8000` (seed node)

---

## Health & Status

### GET /health
Check if the node is running.

**Response:**
```json
{
  "status": "ok",
  "timestamp": "2025-12-10T10:30:00Z"
}
```

### GET /health/detailed
Get detailed node health information.

**Response:**
```json
{
  "status": "ok",
  "uptime_seconds": 3600,
  "peers_connected": 5,
  "last_block_height": 1250,
  "mempool_size": 42
}
```

---

## Node Metrics & Rewards

### GET /metrics/:address
Get metrics for a specific validator.

**Parameters:**
- `address` (path): Validator public key/address

**Example:**
```bash
curl http://localhost:8000/metrics/validator_abc123def456
```

**Response:**
```json
{
  "node_address": "validator_abc123def456",
  "blocks_proposed": 150,
  "blocks_validated": 3420,
  "transactions_processed": 12500,
  "uptime_seconds": 86400,
  "first_seen": "2025-12-01T00:00:00Z",
  "last_active": "2025-12-10T10:30:00Z",
  "total_rewards": 13575
}
```

**Metrics Explained:**
- `blocks_proposed`: Total blocks this validator proposed
- `blocks_validated`: Total blocks this validator voted for
- `transactions_processed`: Total transactions in proposed blocks
- `uptime_seconds`: Cumulative online time
- `total_rewards`: Total OURO coins earned

---

### GET /metrics/leaderboard
Get top 100 validators by total rewards.

**Example:**
```bash
curl http://localhost:8000/metrics/leaderboard
```

**Response:**
```json
[
  {
    "node_address": "validator_top1",
    "blocks_proposed": 500,
    "blocks_validated": 12000,
    "transactions_processed": 45000,
    "uptime_seconds": 259200,
    "first_seen": "2025-12-01T00:00:00Z",
    "last_active": "2025-12-10T10:30:00Z",
    "total_rewards": 46200
  },
  {
    "node_address": "validator_top2",
    "blocks_proposed": 420,
    "blocks_validated": 10500,
    "transactions_processed": 38000,
    "uptime_seconds": 234000,
    "total_rewards": 40200
  }
]
```

---

### GET /rewards/:address
Get reward history for a validator (last 100 rewards).

**Parameters:**
- `address` (path): Validator public key/address

**Example:**
```bash
curl http://localhost:8000/rewards/validator_abc123def456
```

**Response:**
```json
[
  {
    "id": 12345,
    "node_address": "validator_abc123def456",
    "reward_type": "block_proposal",
    "amount": 20,
    "block_height": 1250,
    "awarded_at": "2025-12-10T10:25:00Z"
  },
  {
    "id": 12344,
    "node_address": "validator_abc123def456",
    "reward_type": "block_validation",
    "amount": 3,
    "block_height": 1249,
    "awarded_at": "2025-12-10T10:24:30Z"
  },
  {
    "id": 12300,
    "node_address": "validator_abc123def456",
    "reward_type": "uptime_bonus",
    "amount": 1,
    "block_height": null,
    "awarded_at": "2025-12-10T10:00:00Z"
  }
]
```

**Reward Types:**
- `block_proposal`: Earned for proposing a block (20 OURO)
- `block_validation`: Earned for validating/voting (3 OURO)
- `uptime_bonus`: Earned for staying online (1.5 OURO/hour)

---

## Transactions

### POST /tx/submit
Submit a new transaction to the network.

**Headers:**
- `X-API-Key`: Your API key (default: `default_api_key`)

**Request Body:**
```json
{
  "sender": "address_abc123",
  "recipient": "address_xyz789",
  "tx_hash": "generated_hash",
  "signature": "ed25519_signature",
  "payload": {
    "amount": 100,
    "fee": 1,
    "public_key": "sender_public_key"
  }
}
```

**Response:**
```json
{
  "tx_id": "uuid-generated-id",
  "status": "pending"
}
```

---

### GET /mempool
Get pending transactions in the mempool (last 100).

**Example:**
```bash
curl http://localhost:8000/mempool
```

**Response:**
```json
[
  {
    "tx_id": "uuid-123",
    "tx_hash": "hash_abc",
    "payload": {
      "amount": 100,
      "fee": 1
    },
    "received_at": "2025-12-10T10:30:00Z"
  }
]
```

---

### GET /tx/:id
Get transaction by ID or hash.

**Parameters:**
- `id` (path): Transaction UUID or hash

**Example:**
```bash
curl http://localhost:8000/tx/uuid-or-hash
```

**Response:**
```json
{
  "tx_id": "uuid-123",
  "tx_hash": "hash_abc",
  "sender": "address_abc123",
  "recipient": "address_xyz789",
  "payload": {
    "amount": 100,
    "fee": 1
  },
  "status": "confirmed",
  "included_in_block": "block-uuid"
}
```

---

### GET /tx/hash/:hash
Get transaction by hash (explicit hash lookup).

Same as `/tx/:id` but specifically for hash lookups.

---

## Blocks

### GET /block/:id
Get block information by block ID.

**Parameters:**
- `id` (path): Block UUID

**Example:**
```bash
curl http://localhost:8000/block/block-uuid
```

**Response:**
```json
{
  "block_id": "block-uuid",
  "block_height": 1250,
  "parent_ids": ["parent-block-1", "parent-block-2"],
  "merkle_root": "merkle_root_hash",
  "timestamp": 1702204800,
  "tx_count": 150,
  "signer": "validator_address",
  "signature": "block_signature"
}
```

---

## Network

### GET /peers
Get list of connected peers.

**Example:**
```bash
curl http://localhost:8000/peers
```

**Response:**
```json
{
  "peers": [
    {
      "address": "192.168.1.100:9001",
      "connected_since": "2025-12-10T08:00:00Z",
      "last_seen": "2025-12-10T10:30:00Z"
    }
  ]
}
```

---

## Error Responses

All endpoints return standard error responses:

### 400 Bad Request
```json
{
  "error": "Invalid request parameters"
}
```

### 404 Not Found
```json
{
  "error": "Resource not found"
}
```

### 500 Internal Server Error
```json
{
  "error": "Internal server error"
}
```

---

## Rate Limiting

Default rate limits (configurable):
- **100 requests per 60 seconds** per IP address
- Applies to all protected endpoints
- Public endpoints (`/health`, `/metrics/*`) are not rate-limited

**Rate Limit Headers:**
- `X-RateLimit-Limit`: Maximum requests allowed
- `X-RateLimit-Remaining`: Requests remaining in window
- `X-RateLimit-Reset`: Time when limit resets (Unix timestamp)

---

## Authentication

Protected endpoints require an API key in the header:

```bash
curl -H "X-API-Key: your_api_key" http://localhost:8000/tx/submit
```

Default API key: `default_api_key` (change this in production!)

---

## Examples

### Check Your Validator Performance
```bash
# Get your node's metrics
MY_ADDRESS="your_validator_address"
curl http://localhost:8000/metrics/$MY_ADDRESS | jq

# Calculate your earnings rate
curl http://localhost:8000/metrics/$MY_ADDRESS | \
  jq '.total_rewards / (.uptime_seconds / 3600)'
```

### Monitor Top Validators
```bash
# Get top 10 validators
curl http://localhost:8000/metrics/leaderboard | jq '.[0:10]'

# Find validator with most blocks proposed
curl http://localhost:8000/metrics/leaderboard | \
  jq 'sort_by(.blocks_proposed) | reverse | .[0]'
```

### Track Your Rewards
```bash
# Get recent rewards
curl http://localhost:8000/rewards/$MY_ADDRESS | jq '.[0:10]'

# Calculate total earned today
TODAY=$(date +%Y-%m-%d)
curl http://localhost:8000/rewards/$MY_ADDRESS | \
  jq --arg today "$TODAY" '[.[] | select(.awarded_at | startswith($today))] | map(.amount) | add'
```

---

**Need Help?** Open an issue on GitHub with your API question!
