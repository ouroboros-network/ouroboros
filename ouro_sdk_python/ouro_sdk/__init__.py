"""
Ouroboros SDK for Python

Build decentralized applications on the Ouroboros blockchain platform.

- Microchains: For dApps, games, NFTs, user wallets
- Subchains: For high-scale business infrastructure (payments, oracles, bridges)
"""

from .client import OuroClient
from .microchain import Microchain, MicrochainBuilder
from .subchain import (
    Subchain,
    SubchainBuilder,
    SubchainConfig,
    SubchainStatus,
    SubchainState,
    ValidatorConfig,
    MIN_SUBCHAIN_DEPOSIT,
    RENT_RATE_PER_BLOCK,
)
from .transaction import Transaction, TransactionBuilder
from .types import (
    ConsensusType,
    TxStatus,
    MicrochainConfig,
    MicrochainState,
    Balance,
    BlockHeader,
    TransactionData,
    AnchorFrequency,
)
from .errors import (
    SdkError,
    NetworkError,
    TransactionFailedError,
    MicrochainNotFoundError,
    InsufficientBalanceError,
    InvalidSignatureError,
    AnchorFailedError,
    InvalidConfigError,
)

__version__ = "0.4.0"
__all__ = [
    # Core classes
    "OuroClient",
    "Microchain",
    "MicrochainBuilder",
    "Transaction",
    "TransactionBuilder",
    # Subchain classes
    "Subchain",
    "SubchainBuilder",
    "SubchainConfig",
    "SubchainStatus",
    "SubchainState",
    "ValidatorConfig",
    "MIN_SUBCHAIN_DEPOSIT",
    "RENT_RATE_PER_BLOCK",
    # Types
    "ConsensusType",
    "TxStatus",
    "MicrochainConfig",
    "MicrochainState",
    "Balance",
    "BlockHeader",
    "TransactionData",
    "AnchorFrequency",
    # Errors
    "SdkError",
    "NetworkError",
    "TransactionFailedError",
    "MicrochainNotFoundError",
    "InsufficientBalanceError",
    "InvalidSignatureError",
    "AnchorFailedError",
    "InvalidConfigError",
]
