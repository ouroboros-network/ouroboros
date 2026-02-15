"""Transaction building and signing"""

import uuid
from datetime import datetime
from typing import Optional, Dict, Any
import nacl.signing
import nacl.encoding
from .types import TransactionData
from .errors import InvalidConfigError, InvalidSignatureError


class Transaction:
    """Transaction class for building and signing transactions"""

    def __init__(self, from_addr: str, to: str, amount: int):
        self.id = str(uuid.uuid4())
        self.from_addr = from_addr
        self.to = to
        self.amount = amount
        self.nonce = 0
        self.signature = ""
        self.data: Optional[Dict[str, Any]] = None
        self.timestamp = datetime.utcnow().isoformat() + "Z"

    def with_nonce(self, nonce: int) -> "Transaction":
        """Set transaction nonce"""
        self.nonce = nonce
        return self

    def with_data(self, data: Dict[str, Any]) -> "Transaction":
        """Add custom data to transaction"""
        self.data = data
        return self

    def sign(self, private_key_hex: str) -> "Transaction":
        """Sign transaction with private key (hex string)"""
        try:
            private_key_bytes = bytes.fromhex(private_key_hex)
            signing_key = nacl.signing.SigningKey(private_key_bytes)

            message = self._get_signing_message()
            signature_bytes = signing_key.sign(message.encode()).signature

            self.signature = signature_bytes.hex()
            return self
        except Exception as e:
            raise InvalidSignatureError()

    def _get_signing_message(self) -> str:
        """Get signing message"""
        return f"{self.id}:{self.from_addr}:{self.to}:{self.amount}:{self.nonce}"

    def to_json(self) -> TransactionData:
        """Convert to TransactionData for API submission"""
        return TransactionData(
            id=self.id,
            from_addr=self.from_addr,
            to=self.to,
            amount=self.amount,
            nonce=self.nonce,
            signature=self.signature,
            data=self.data,
            timestamp=self.timestamp,
        )

    @staticmethod
    def from_json(data: TransactionData) -> "Transaction":
        """Create from TransactionData"""
        tx = Transaction(data.from_addr, data.to, data.amount)
        tx.id = data.id
        tx.nonce = data.nonce
        tx.signature = data.signature
        tx.data = data.data
        tx.timestamp = data.timestamp or tx.timestamp
        return tx


class TransactionBuilder:
    """Builder for creating transactions"""

    def __init__(self):
        self._from: Optional[str] = None
        self._to: Optional[str] = None
        self._amount: Optional[int] = None
        self._nonce: int = 0
        self._data: Optional[Dict[str, Any]] = None

    def set_from(self, from_addr: str) -> "TransactionBuilder":
        """Set sender address"""
        self._from = from_addr
        return self

    def set_to(self, to: str) -> "TransactionBuilder":
        """Set recipient address"""
        self._to = to
        return self

    def set_amount(self, amount: int) -> "TransactionBuilder":
        """Set amount"""
        self._amount = amount
        return self

    def set_nonce(self, nonce: int) -> "TransactionBuilder":
        """Set nonce"""
        self._nonce = nonce
        return self

    def set_data(self, data: Dict[str, Any]) -> "TransactionBuilder":
        """Add custom data"""
        self._data = data
        return self

    def build(self) -> Transaction:
        """Build transaction"""
        if not self._from:
            raise InvalidConfigError("Missing 'from' address")
        if not self._to:
            raise InvalidConfigError("Missing 'to' address")
        if self._amount is None:
            raise InvalidConfigError("Missing amount")

        tx = Transaction(self._from, self._to, self._amount)
        tx.nonce = self._nonce
        if self._data:
            tx.data = self._data

        return tx
