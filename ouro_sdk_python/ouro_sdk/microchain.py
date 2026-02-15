"""Microchain interface for building dApps"""

from typing import List, Dict, Any, Optional
import requests
from .client import OuroClient
from .transaction import Transaction, TransactionBuilder
from .types import (
    MicrochainConfig,
    MicrochainState,
    BlockHeader,
    TransactionData,
    ConsensusType,
    AnchorFrequency,
)
from .errors import InvalidConfigError, TransactionFailedError


class Microchain:
    """Microchain interface for building dApps"""

    def __init__(
        self, microchain_id: str, client: OuroClient, base_url: str, nonce: int = 0
    ):
        self.id = microchain_id
        self._client = client
        self._base_url = base_url
        self._nonce = nonce

    @classmethod
    def connect(cls, microchain_id: str, node_url: str) -> "Microchain":
        """Connect to an existing microchain"""
        client = OuroClient(node_url)

        # Verify microchain exists
        client.get_microchain_state(microchain_id)

        return cls(microchain_id, client, node_url, 0)

    @classmethod
    def create(cls, config: MicrochainConfig, node_url: str) -> "Microchain":
        """Create a new microchain"""
        client = OuroClient(node_url)
        microchain_id = client.create_microchain(config)

        return cls(microchain_id, client, node_url, 0)

    def state(self) -> MicrochainState:
        """Get microchain state"""
        return self._client.get_microchain_state(self.id)

    def balance(self, address: str) -> int:
        """Get balance for an address on this microchain"""
        return self._client.get_microchain_balance(self.id, address)

    def submit_tx(self, tx: Transaction) -> str:
        """Submit a transaction to this microchain"""
        try:
            url = f"{self._base_url}/microchain/{self.id}/tx"
            response = requests.post(url, json=tx.to_json().to_dict())
            response.raise_for_status()
            data = response.json()

            if data.get("success"):
                self._nonce += 1
                return data["tx_id"]
            else:
                raise TransactionFailedError(data.get("message", "Unknown error"))
        except TransactionFailedError:
            raise
        except Exception as e:
            raise TransactionFailedError(str(e))

    def tx(self) -> TransactionBuilder:
        """Create a transaction builder for this microchain"""
        builder = TransactionBuilder()
        builder.set_nonce(self._nonce)
        return builder

    def transfer(self, from_addr: str, to: str, amount: int) -> str:
        """Transfer tokens on this microchain (simplified)"""
        tx = Transaction(from_addr, to, amount)
        tx.nonce = self._nonce

        return self.submit_tx(tx)

    def anchor(self) -> str:
        """Anchor this microchain to subchain/mainchain"""
        return self._client.anchor_microchain(self.id)

    def tx_history(self, from_block: int, to_block: int) -> List[TransactionData]:
        """Get transaction history for this microchain"""
        try:
            url = f"{self._base_url}/microchain/{self.id}/txs?from={from_block}&to={to_block}"
            response = requests.get(url)
            response.raise_for_status()
            data = response.json()

            return [TransactionData.from_dict(tx) for tx in data["transactions"]]
        except Exception as e:
            raise Exception(f"Failed to fetch history: {str(e)}")

    def blocks(self, limit: int) -> List[BlockHeader]:
        """Get latest blocks from this microchain"""
        try:
            url = f"{self._base_url}/microchain/{self.id}/blocks?limit={limit}"
            response = requests.get(url)
            response.raise_for_status()
            data = response.json()

            return [BlockHeader.from_dict(block) for block in data["blocks"]]
        except Exception as e:
            raise Exception(f"Failed to fetch blocks: {str(e)}")


class MicrochainBuilder:
    """Builder for creating microchains"""

    def __init__(self, name: str, owner: str):
        self._config = MicrochainConfig(
            name=name,
            owner=owner,
            consensus={"type": ConsensusType.SINGLE_VALIDATOR.value},
            anchor_frequency=AnchorFrequency.every_n_blocks(100),
            max_txs_per_block=1000,
            block_time_secs=5,
        )
        self._node_url: Optional[str] = None

    def node(self, url: str) -> "MicrochainBuilder":
        """Set node URL"""
        self._node_url = url
        return self

    def consensus(
        self, consensus_type: ConsensusType, validator_count: Optional[int] = None
    ) -> "MicrochainBuilder":
        """Set consensus type"""
        self._config.consensus = {"type": consensus_type.value}
        if validator_count:
            self._config.consensus["validator_count"] = validator_count
        return self

    def anchor_frequency(self, frequency: AnchorFrequency) -> "MicrochainBuilder":
        """Set anchor frequency"""
        self._config.anchor_frequency = frequency
        return self

    def block_time(self, seconds: int) -> "MicrochainBuilder":
        """Set block time"""
        self._config.block_time_secs = seconds
        return self

    def build(self) -> Microchain:
        """Build and create the microchain"""
        if not self._node_url:
            raise InvalidConfigError("Node URL not specified")

        return Microchain.create(self._config, self._node_url)
