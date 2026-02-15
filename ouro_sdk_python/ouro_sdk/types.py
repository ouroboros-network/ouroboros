"""Type definitions for the Ouroboros SDK"""

from typing import Dict, Any, Optional, List, Union
from enum import Enum
from dataclasses import dataclass


class ConsensusType(str, Enum):
    """Consensus type for microchain"""

    SINGLE_VALIDATOR = "single_validator"
    BFT = "bft"


class TxStatus(str, Enum):
    """Transaction status"""

    PENDING = "pending"
    CONFIRMED = "confirmed"
    FAILED = "failed"
    ANCHORED = "anchored"


@dataclass
class AnchorFrequency:
    """Anchor frequency configuration"""

    type: str  # 'blocks', 'seconds', or 'manual'
    count: Optional[int] = None

    @staticmethod
    def every_n_blocks(count: int) -> "AnchorFrequency":
        """Anchor every N blocks"""
        return AnchorFrequency(type="blocks", count=count)

    @staticmethod
    def every_n_seconds(count: int) -> "AnchorFrequency":
        """Anchor every N seconds"""
        return AnchorFrequency(type="seconds", count=count)

    @staticmethod
    def manual() -> "AnchorFrequency":
        """Manual anchoring only"""
        return AnchorFrequency(type="manual")

    def to_dict(self) -> Dict[str, Any]:
        """Convert to dictionary"""
        if self.type == "manual":
            return {"type": "manual"}
        return {"type": self.type, "count": self.count}


@dataclass
class MicrochainConfig:
    """Configuration for creating a microchain"""

    name: str
    owner: str
    consensus: Optional[Dict[str, Any]] = None
    anchor_frequency: Optional[AnchorFrequency] = None
    max_txs_per_block: int = 1000
    block_time_secs: int = 5

    def to_dict(self) -> Dict[str, Any]:
        """Convert to dictionary for API submission"""
        config = {
            "name": self.name,
            "owner": self.owner,
            "max_txs_per_block": self.max_txs_per_block,
            "block_time_secs": self.block_time_secs,
        }
        if self.consensus:
            config["consensus"] = self.consensus
        if self.anchor_frequency:
            config["anchor_frequency"] = self.anchor_frequency.to_dict()
        return config


@dataclass
class MicrochainState:
    """Microchain state information"""

    id: str
    name: str
    owner: str
    block_height: int
    tx_count: int
    created_at: str
    last_anchor_height: Optional[int] = None

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "MicrochainState":
        """Create from dictionary"""
        return MicrochainState(
            id=data["id"],
            name=data["name"],
            owner=data["owner"],
            block_height=data["blockHeight"],
            tx_count=data["txCount"],
            created_at=data["createdAt"],
            last_anchor_height=data.get("lastAnchorHeight"),
        )


@dataclass
class Balance:
    """Balance information"""

    address: str
    balance: int
    pending: int


@dataclass
class BlockHeader:
    """Block header"""

    height: int
    hash: str
    previous_hash: str
    timestamp: str
    tx_count: int

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "BlockHeader":
        """Create from dictionary"""
        return BlockHeader(
            height=data["height"],
            hash=data["hash"],
            previous_hash=data["previousHash"],
            timestamp=data["timestamp"],
            tx_count=data["txCount"],
        )


@dataclass
class TransactionData:
    """Transaction data"""

    id: str
    from_addr: str
    to: str
    amount: int
    nonce: int
    signature: str
    data: Optional[Dict[str, Any]] = None
    timestamp: Optional[str] = None

    def to_dict(self) -> Dict[str, Any]:
        """Convert to dictionary for API submission"""
        tx_dict = {
            "id": self.id,
            "from": self.from_addr,
            "to": self.to,
            "amount": self.amount,
            "nonce": self.nonce,
            "signature": self.signature,
        }
        if self.data:
            tx_dict["data"] = self.data
        if self.timestamp:
            tx_dict["timestamp"] = self.timestamp
        return tx_dict

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "TransactionData":
        """Create from dictionary"""
        return TransactionData(
            id=data["id"],
            from_addr=data["from"],
            to=data["to"],
            amount=data["amount"],
            nonce=data["nonce"],
            signature=data["signature"],
            data=data.get("data"),
            timestamp=data.get("timestamp"),
        )
