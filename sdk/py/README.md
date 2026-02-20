# ouro-sdk

Python SDK for the [Ouroboros](https://github.com/ouroboros-network/ouroboros) decentralized blockchain network.

## Install

```bash
pip install ouro-sdk

# For async support (AsyncOuroClient):
pip install ouro-sdk[async]
```

## Quick Start (sync)

```python
from ouro_sdk import OuroClient

client = OuroClient("http://localhost:8000", api_key="your-api-key")

# Check health
print(client.health())  # {'status': 'healthy', ...}

# Get consensus state
c = client.consensus()
print(f"View: {c['view']}, Leader: {c['leader']}")

# Get balance
bal = client.balance("ouro1myaddress")
print(f"Balance: {bal['balance']} nanoouro")

# Submit a transaction
result = client.submit_transaction({
    "sender": "ouro1abc...",
    "recipient": "ouro1xyz...",
    "amount": 1_000_000_000,  # 1 OURO
    "signature": "hex-encoded-signature",
})
print(f"TX: {result['tx_id']}")

# Combined status snapshot
status = client.status()
print(f"Online: {status['online']}, Block: {status.get('metrics', {}).get('block_height')}")
```

## Quick Start (async)

```python
import asyncio
from ouro_sdk import AsyncOuroClient

async def main():
    client = AsyncOuroClient("http://localhost:8000", api_key="your-api-key")
    health = await client.health()
    consensus = await client.consensus()
    status = await client.status()
    print(status)

asyncio.run(main())
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
| `network_stats()` | GET /network/stats | Yes | Network stats |
| `get_transaction(id)` | GET /tx/:id | Yes | Transaction lookup |
| `submit_transaction(tx)` | POST /tx/submit | Yes | Submit transaction |
| `transfer(from, to, amt)` | POST /ouro/transfer | Yes | Transfer OURO |
| `status()` | combined | - | Full snapshot |

## Environment Variables

- `OURO_API_KEY` — API key for protected endpoints

## License

MIT
