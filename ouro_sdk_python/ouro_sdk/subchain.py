"""Subchain interface for building high-scale business applications"""

from typing import List, Optional
from dataclasses import dataclass
from enum import Enum
import requests
from .client import OuroClient
from .transaction import Transaction, TransactionBuilder
from .errors import InvalidConfigError, TransactionFailedError


# Minimum deposit required to create a subchain (5,000 OURO)
MIN_SUBCHAIN_DEPOSIT = 500_000_000_000

# Rent rate per block (0.0001 OURO)
RENT_RATE_PER_BLOCK = 10_000


class SubchainState(Enum):
    """Subchain state"""
    ACTIVE = "active"
    GRACE_PERIOD = "grace_period"
    TERMINATED = "terminated"


@dataclass
class ValidatorConfig:
    """Validator configuration for subchain"""
    pubkey: str
    stake: int
    endpoint: Optional[str] = None

    def to_dict(self) -> dict:
        result = {"pubkey": self.pubkey, "stake": self.stake}
        if self.endpoint:
            result["endpoint"] = self.endpoint
        return result


@dataclass
class SubchainConfig:
    """Subchain configuration"""
    name: str
    owner: str
    deposit: int = MIN_SUBCHAIN_DEPOSIT
    anchor_frequency: int = 100
    rpc_endpoint: Optional[str] = None
    validators: List[ValidatorConfig] = None

    def __post_init__(self):
        if self.validators is None:
            self.validators = []

    def with_deposit(self, deposit: int) -> "SubchainConfig":
        """Set deposit amount"""
        self.deposit = deposit
        return self

    def with_anchor_frequency(self, frequency: int) -> "SubchainConfig":
        """Set anchor frequency"""
        self.anchor_frequency = frequency
        return self

    def with_rpc_endpoint(self, endpoint: str) -> "SubchainConfig":
        """Set RPC endpoint"""
        self.rpc_endpoint = endpoint
        return self

    def with_validator(self, pubkey: str, stake: int, endpoint: Optional[str] = None) -> "SubchainConfig":
        """Add a validator"""
        self.validators.append(ValidatorConfig(pubkey, stake, endpoint))
        return self

    def validate(self) -> None:
        """Validate configuration"""
        if not self.name or len(self.name) > 64:
            raise InvalidConfigError("Name must be 1-64 characters")
        if self.deposit < MIN_SUBCHAIN_DEPOSIT:
            raise InvalidConfigError(
                f"Deposit must be at least {MIN_SUBCHAIN_DEPOSIT // 100_000_000} OURO"
            )


@dataclass
class SubchainStatus:
    """Subchain status information"""
    id: str
    name: str
    owner: str
    state: SubchainState
    deposit_balance: int
    blocks_remaining: int
    block_height: int
    tx_count: int
    last_anchor_height: Optional[int]
    validator_count: int

    @classmethod
    def from_dict(cls, data: dict) -> "SubchainStatus":
        return cls(
            id=data["id"],
            name=data["name"],
            owner=data["owner"],
            state=SubchainState(data["state"]),
            deposit_balance=data["deposit_balance"],
            blocks_remaining=data["blocks_remaining"],
            block_height=data["block_height"],
            tx_count=data["tx_count"],
            last_anchor_height=data.get("last_anchor_height"),
            validator_count=data["validator_count"],
        )


class Subchain:
    """
    Subchain interface for building high-scale business applications

    Subchains are designed for:
    - Financial infrastructure (money transfer, payments)
    - High-throughput services (oracles, bridges)
    - Enterprise applications with dedicated resources

    Requirements:
    - Minimum deposit: 5,000 OURO
    - Rent: 0.01 OURO per block

    Example:
        >>> subchain = Subchain.connect("hermes-subchain", "http://localhost:8001")
        >>> status = subchain.status()
        >>> print(f"Blocks remaining: {status.blocks_remaining}")
    """

    def __init__(
        self, subchain_id: str, client: OuroClient, base_url: str, nonce: int = 0
    ):
        self.id = subchain_id
        self._client = client
        self._base_url = base_url
        self._nonce = nonce

    @classmethod
    def connect(cls, subchain_id: str, node_url: str) -> "Subchain":
        """
        Connect to an existing subchain

        Args:
            subchain_id: The subchain ID to connect to
            node_url: Node URL to connect through

        Returns:
            Connected Subchain instance
        """
        client = OuroClient(node_url)

        # Verify subchain exists
        response = requests.get(f"{node_url}/subchain/{subchain_id}/status")
        response.raise_for_status()
        data = response.json()

        if not data.get("success", True):
            raise Exception(f"Subchain not found: {subchain_id}")

        return cls(subchain_id, client, node_url, 0)

    @classmethod
    def register(cls, config: SubchainConfig, node_url: str) -> "Subchain":
        """
        Register a new subchain

        Args:
            config: Subchain configuration
            node_url: Node URL to register through

        Returns:
            Registered Subchain instance
        """
        config.validate()

        client = OuroClient(node_url)

        # Register subchain
        response = requests.post(f"{node_url}/subchain/register", json={
            "name": config.name,
            "owner": config.owner,
            "deposit": config.deposit,
            "anchor_frequency": config.anchor_frequency,
            "rpc_endpoint": config.rpc_endpoint,
            "validators": [v.to_dict() for v in config.validators],
        })
        response.raise_for_status()
        data = response.json()

        if not data.get("success"):
            raise Exception(data.get("message", "Failed to register subchain"))

        return cls(data["subchain_id"], client, node_url, 0)

    def status(self) -> SubchainStatus:
        """Get subchain status"""
        response = requests.get(f"{self._base_url}/subchain/{self.id}/status")
        response.raise_for_status()
        return SubchainStatus.from_dict(response.json())

    def deposit_balance(self) -> int:
        """Get current deposit balance"""
        return self.status().deposit_balance

    def blocks_remaining(self) -> int:
        """Get estimated blocks remaining before rent runs out"""
        return self.status().blocks_remaining

    def top_up_rent(self, amount: int) -> str:
        """
        Top up rent deposit

        Args:
            amount: Amount to add to deposit

        Returns:
            Transaction ID
        """
        response = requests.post(
            f"{self._base_url}/subchain/{self.id}/topup",
            json={"amount": amount}
        )
        response.raise_for_status()
        data = response.json()

        if not data.get("success"):
            raise Exception(data.get("message", "Failed to top up rent"))

        return data["tx_id"]

    def balance(self, address: str) -> int:
        """
        Get balance for an address on this subchain

        Args:
            address: Address to check

        Returns:
            Balance amount
        """
        response = requests.get(
            f"{self._base_url}/subchain/{self.id}/balance/{address}"
        )
        response.raise_for_status()
        return response.json()["balance"]

    def submit_tx(self, tx: Transaction) -> str:
        """
        Submit a transaction to this subchain

        Args:
            tx: Transaction to submit

        Returns:
            Transaction ID
        """
        try:
            url = f"{self._base_url}/subchain/{self.id}/tx"
            response = requests.post(url, json=tx.to_dict())
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
        """Create a transaction builder for this subchain"""
        builder = TransactionBuilder()
        builder.set_nonce(self._nonce)
        return builder

    def transfer(self, from_addr: str, to: str, amount: int) -> str:
        """
        Transfer tokens on this subchain

        Args:
            from_addr: Sender address
            to: Recipient address
            amount: Amount to transfer

        Returns:
            Transaction ID
        """
        tx = Transaction(from_addr, to, amount)
        tx.nonce = self._nonce
        return self.submit_tx(tx)

    def anchor(self) -> str:
        """
        Anchor this subchain to mainchain

        Returns:
            Transaction ID
        """
        response = requests.post(f"{self._base_url}/subchain/{self.id}/anchor")
        response.raise_for_status()
        data = response.json()

        if not data.get("success"):
            raise Exception(data.get("message", "Failed to anchor"))

        return data["tx_id"]

    def tx_history(self, from_block: int, to_block: int) -> List[dict]:
        """
        Get transaction history

        Args:
            from_block: Start block
            to_block: End block

        Returns:
            List of transactions
        """
        url = f"{self._base_url}/subchain/{self.id}/txs?from={from_block}&to={to_block}"
        response = requests.get(url)
        response.raise_for_status()
        return response.json()["transactions"]

    def add_validator(self, validator: ValidatorConfig) -> str:
        """
        Add a validator to the subchain

        Args:
            validator: Validator configuration

        Returns:
            Transaction ID
        """
        response = requests.post(
            f"{self._base_url}/subchain/{self.id}/validators",
            json=validator.to_dict()
        )
        response.raise_for_status()
        data = response.json()

        if not data.get("success"):
            raise Exception(data.get("message", "Failed to add validator"))

        return data["tx_id"]

    def remove_validator(self, pubkey: str) -> str:
        """
        Remove a validator from the subchain

        Args:
            pubkey: Validator public key to remove

        Returns:
            Transaction ID
        """
        response = requests.delete(
            f"{self._base_url}/subchain/{self.id}/validators/{pubkey}"
        )
        response.raise_for_status()
        data = response.json()

        if not data.get("success"):
            raise Exception(data.get("message", "Failed to remove validator"))

        return data["tx_id"]

    def validators(self) -> List[ValidatorConfig]:
        """
        Get list of validators

        Returns:
            List of validator configurations
        """
        response = requests.get(
            f"{self._base_url}/subchain/{self.id}/validators"
        )
        response.raise_for_status()
        data = response.json()
        return [
            ValidatorConfig(
                pubkey=v["pubkey"],
                stake=v["stake"],
                endpoint=v.get("endpoint")
            )
            for v in data["validators"]
        ]

    def withdraw_deposit(self) -> str:
        """
        Withdraw deposit (only after termination)

        Returns:
            Transaction ID
        """
        response = requests.post(
            f"{self._base_url}/subchain/{self.id}/withdraw"
        )
        response.raise_for_status()
        data = response.json()

        if not data.get("success"):
            raise Exception(data.get("message", "Failed to withdraw deposit"))

        return data["tx_id"]


class SubchainBuilder:
    """
    Builder for creating subchains

    Example:
        >>> subchain = SubchainBuilder("Hermes", "ouro1owner...") \\
        ...     .node("http://localhost:8001") \\
        ...     .deposit(1_000_000_000_000) \\
        ...     .anchor_frequency(50) \\
        ...     .validator("validator1_pubkey", 100_000_000_000) \\
        ...     .build()
    """

    def __init__(self, name: str, owner: str):
        self._config = SubchainConfig(name=name, owner=owner)
        self._node_url: Optional[str] = None

    def node(self, url: str) -> "SubchainBuilder":
        """Set node URL"""
        self._node_url = url
        return self

    def deposit(self, amount: int) -> "SubchainBuilder":
        """Set deposit amount (1 OURO = 100,000,000)"""
        self._config.deposit = amount
        return self

    def anchor_frequency(self, frequency: int) -> "SubchainBuilder":
        """Set anchor frequency in blocks"""
        self._config.anchor_frequency = frequency
        return self

    def rpc_endpoint(self, endpoint: str) -> "SubchainBuilder":
        """Set RPC endpoint"""
        self._config.rpc_endpoint = endpoint
        return self

    def validator(
        self, pubkey: str, stake: int, endpoint: Optional[str] = None
    ) -> "SubchainBuilder":
        """Add a validator"""
        self._config.validators.append(ValidatorConfig(pubkey, stake, endpoint))
        return self

    def build(self) -> Subchain:
        """Build and register the subchain"""
        if not self._node_url:
            raise InvalidConfigError("Node URL not specified")

        return Subchain.register(self._config, self._node_url)
