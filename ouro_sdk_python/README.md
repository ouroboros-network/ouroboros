# Ouroboros Microchain SDK (Python)

The official Python SDK for building decentralized applications on the Ouroboros blockchain platform.

## Features

- **Pythonic API**: Clean, idiomatic Python interface
- **Built-in Signing**: Ed25519 signature support via PyNaCl
- **Microchain Management**: Create and manage application-specific blockchains
- **Balance Queries**: Check balances on mainchain and microchains
- **State Inspection**: Query microchain state, blocks, and transaction history
- **Flexible Consensus**: Choose between SingleValidator (fast) or BFT (secure)
- **Automatic Anchoring**: Inherit mainchain security through configurable anchoring
- **Type Hints**: Full type hint support for better IDE integration

## Installation

```bash
pip install ouroboros-sdk
```

Or install from source:

```bash
git clone https://github.com/ouroboros-network/ouroboros
cd ouroboros/ouro_sdk_python
pip install -e .
```

## Quick Start

```python
from ouro_sdk import Microchain, MicrochainConfig

# Create a new microchain
config = MicrochainConfig(
    name="MyDApp",
    owner="ouro1owner...",
)
microchain = Microchain.create(config, "http://localhost:8001")

# Transfer tokens
tx_id = microchain.transfer("ouro1alice...", "ouro1bob...", 1000)
print(f"Transaction: {tx_id}")
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

```python
from ouro_sdk import MicrochainBuilder, ConsensusType, AnchorFrequency

# Gaming microchain - prioritize speed
gaming = (
    MicrochainBuilder("GameFi", "ouro1owner...")
    .node("http://localhost:8001")
    .block_time(2)  # 2-second blocks
    .consensus(ConsensusType.SINGLE_VALIDATOR)
    .anchor_frequency(AnchorFrequency.every_n_seconds(300))
    .build()
)

# DeFi microchain - prioritize security
defi = (
    MicrochainBuilder("DeFiProtocol", "ouro1owner...")
    .node("http://localhost:8001")
    .block_time(10)  # 10-second blocks
    .consensus(ConsensusType.BFT, 7)  # 7 validators
    .anchor_frequency(AnchorFrequency.every_n_blocks(50))
    .build()
)
```

### Sending Transactions

```python
from ouro_sdk import Transaction

# Simple transfer
tx_id = microchain.transfer("ouro1alice...", "ouro1bob...", 1000)

# Custom transaction with data
tx = (
    microchain.tx()
    .set_from("ouro1alice...")
    .set_to("ouro1contract...")
    .set_amount(500)
    .set_data({
        "method": "mint_nft",
        "params": {
            "token_id": "12345",
            "metadata": "ipfs://Qm..."
        }
    })
    .build()
)

# Sign and submit
tx.sign("private_key_hex")
custom_tx_id = microchain.submit_tx(tx)
```

### Querying State

```python
# Get microchain state
state = microchain.state()
print(f"Block height: {state.block_height}")
print(f"Total transactions: {state.tx_count}")

# Check balance
balance = microchain.balance("ouro1alice...")
print(f"Balance: {balance} tokens")

# Get transaction history
txs = microchain.tx_history(0, 100)
for tx in txs:
    print(f"{tx.from_addr} -> {tx.to}: {tx.amount}")

# Get recent blocks
blocks = microchain.blocks(10)
for block in blocks:
    print(f"Block #{block.height}: {block.tx_count} txs")
```

### Anchoring

```python
# Manual anchor
anchor_id = microchain.anchor()
print(f"Anchored: {anchor_id}")

# Automatic anchoring (configured during creation)
auto_anchor = (
    MicrochainBuilder("AutoDApp", "ouro1owner...")
    .node("http://localhost:8001")
    .anchor_frequency(AnchorFrequency.every_n_blocks(100))  # Every 100 blocks
    .build()
)
```

## API Reference

### Microchain

```python
class Microchain:
    # Create new microchain
    @classmethod
    def create(cls, config: MicrochainConfig, node_url: str) -> Microchain

    # Connect to existing microchain
    @classmethod
    def connect(cls, microchain_id: str, node_url: str) -> Microchain

    # Get current state
    def state(self) -> MicrochainState

    # Check balance
    def balance(self, address: str) -> int

    # Transfer tokens (simplified)
    def transfer(self, from_addr: str, to: str, amount: int) -> str

    # Submit custom transaction
    def submit_tx(self, tx: Transaction) -> str

    # Get transaction builder
    def tx(self) -> TransactionBuilder

    # Anchor to mainchain
    def anchor(self) -> str

    # Query transaction history
    def tx_history(self, from_block: int, to_block: int) -> List[TransactionData]

    # Get recent blocks
    def blocks(self, limit: int) -> List[BlockHeader]
```

### MicrochainBuilder

```python
class MicrochainBuilder:
    def __init__(self, name: str, owner: str)

    def node(self, url: str) -> MicrochainBuilder
    def consensus(self, consensus_type: ConsensusType,
                  validator_count: Optional[int] = None) -> MicrochainBuilder
    def anchor_frequency(self, frequency: AnchorFrequency) -> MicrochainBuilder
    def block_time(self, seconds: int) -> MicrochainBuilder

    def build(self) -> Microchain
```

### Transaction

```python
class Transaction:
    def __init__(self, from_addr: str, to: str, amount: int)

    def with_nonce(self, nonce: int) -> Transaction
    def with_data(self, data: Dict[str, Any]) -> Transaction
    def sign(self, private_key_hex: str) -> Transaction

    def to_json(self) -> TransactionData
    @staticmethod
    def from_json(data: TransactionData) -> Transaction
```

### TransactionBuilder

```python
class TransactionBuilder:
    def set_from(self, from_addr: str) -> TransactionBuilder
    def set_to(self, to: str) -> TransactionBuilder
    def set_amount(self, amount: int) -> TransactionBuilder
    def set_nonce(self, nonce: int) -> TransactionBuilder
    def set_data(self, data: Dict[str, Any]) -> TransactionBuilder

    def build(self) -> Transaction
```

### OuroClient

```python
class OuroClient:
    def __init__(self, node_url: str)

    def get_balance(self, address: str) -> Balance
    def get_microchain_balance(self, microchain_id: str, address: str) -> int
    def submit_transaction(self, tx: TransactionData) -> str
    def get_transaction_status(self, tx_id: str) -> TxStatus
    def create_microchain(self, config: MicrochainConfig) -> str
    def get_microchain_state(self, microchain_id: str) -> MicrochainState
    def list_microchains(self) -> List[MicrochainState]
    def anchor_microchain(self, microchain_id: str) -> str
    def health_check(self) -> bool
```

## Types

### ConsensusType

```python
class ConsensusType(Enum):
    SINGLE_VALIDATOR = "single_validator"  # Fast, centralized
    BFT = "bft"                            # Slower, decentralized
```

### AnchorFrequency

```python
class AnchorFrequency:
    @staticmethod
    def every_n_blocks(count: int) -> AnchorFrequency

    @staticmethod
    def every_n_seconds(count: int) -> AnchorFrequency

    @staticmethod
    def manual() -> AnchorFrequency
```

### MicrochainConfig

```python
@dataclass
class MicrochainConfig:
    name: str
    owner: str
    consensus: Optional[Dict[str, Any]] = None
    anchor_frequency: Optional[AnchorFrequency] = None
    max_txs_per_block: int = 1000
    block_time_secs: int = 5
```

## Examples

### Gaming dApp

```python
# High-speed gaming microchain
gaming = (
    MicrochainBuilder("MyGame", "ouro1gamedev...")
    .node("http://localhost:8001")
    .block_time(2)  # 2-second blocks for responsive gameplay
    .consensus(ConsensusType.SINGLE_VALIDATOR)
    .anchor_frequency(AnchorFrequency.every_n_seconds(600))  # Anchor every 10 min
    .build()
)

# Process in-game transactions instantly
gaming.transfer("ouro1player1...", "ouro1player2...", 100)
```

### DeFi Protocol

```python
# High-security DeFi microchain
defi = (
    MicrochainBuilder("LendingProtocol", "ouro1defi...")
    .node("http://localhost:8001")
    .block_time(10)
    .consensus(ConsensusType.BFT, 7)
    .anchor_frequency(AnchorFrequency.every_n_blocks(50))
    .build()
)

# Submit smart contract call
tx = (
    defi.tx()
    .set_from("ouro1user...")
    .set_to("ouro1contract...")
    .set_amount(10000)
    .set_data({
        "method": "deposit",
        "collateral_ratio": 150
    })
    .build()
)

tx.sign("private_key")
defi.submit_tx(tx)
```

### NFT Marketplace

```python
nft = (
    MicrochainBuilder("NFTMarket", "ouro1nft...")
    .node("http://localhost:8001")
    .block_time(5)
    .consensus(ConsensusType.SINGLE_VALIDATOR)
    .build()
)

# Mint NFT
mint_tx = (
    nft.tx()
    .set_from("ouro1artist...")
    .set_to("ouro1marketplace...")
    .set_amount(0)
    .set_data({
        "action": "mint",
        "token_id": "unique_nft_123",
        "metadata": "ipfs://Qm...",
        "royalty": 5
    })
    .build()
)

mint_tx.sign("artist_key")
nft.submit_tx(mint_tx)
```

## Error Handling

```python
from ouro_sdk import (
    TransactionFailedError,
    InsufficientBalanceError,
    NetworkError
)

try:
    microchain.transfer("alice", "bob", 1000)
except InsufficientBalanceError as e:
    print(f"Need {e.required}, have {e.available}")
except TransactionFailedError as e:
    print(f"Transaction failed: {e}")
except NetworkError as e:
    print(f"Network error: {e}")
```

## Development

```bash
# Install development dependencies
pip install -e ".[dev]"

# Run tests
pytest

# Format code
black ouro_sdk

# Type checking
mypy ouro_sdk

# Linting
pylint ouro_sdk
```

## Running Examples

```bash
# Run examples
python examples/basic_microchain.py
python examples/advanced_microchain.py
python examples/builder_pattern.py
python examples/client_usage.py
```

## Django Integration

```python
from django.db import models
from ouro_sdk import Microchain

class DAppModel(models.Model):
    microchain_id = models.CharField(max_length=100)

    def get_microchain(self):
        return Microchain.connect(
            self.microchain_id,
            "http://localhost:8001"
        )

    def process_payment(self, from_addr, to_addr, amount):
        mc = self.get_microchain()
        return mc.transfer(from_addr, to_addr, amount)
```

## FastAPI Integration

```python
from fastapi import FastAPI
from ouro_sdk import Microchain, MicrochainConfig

app = FastAPI()

@app.post("/create-microchain")
async def create_microchain(name: str, owner: str):
    config = MicrochainConfig(name=name, owner=owner)
    microchain = Microchain.create(config, "http://localhost:8001")
    return {"microchain_id": microchain.id}

@app.post("/transfer")
async def transfer(
    microchain_id: str,
    from_addr: str,
    to_addr: str,
    amount: int
):
    mc = Microchain.connect(microchain_id, "http://localhost:8001")
    tx_id = mc.transfer(from_addr, to_addr, amount)
    return {"tx_id": tx_id}
```

## Async Support

```python
# For async frameworks, wrap SDK calls in async functions
import asyncio
from ouro_sdk import Microchain

async def create_and_transfer():
    # SDK operations are sync, but you can run them in executor
    loop = asyncio.get_event_loop()

    microchain = await loop.run_in_executor(
        None,
        Microchain.create,
        config,
        "http://localhost:8001"
    )

    tx_id = await loop.run_in_executor(
        None,
        microchain.transfer,
        "alice",
        "bob",
        1000
    )

    return tx_id
```

## Roadmap

- Core SDK implementation (Python) - Complete
- Async/await support - In Progress
- Django ORM integration package - In Progress
- Flask extension - Planned
- Smart contract support - Planned
- WebSocket real-time updates - Planned

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
