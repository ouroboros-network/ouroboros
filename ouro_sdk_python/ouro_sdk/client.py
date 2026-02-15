"""HTTP client for interacting with Ouroboros network"""

from typing import List, Dict, Any
import requests
from .types import (
    Balance,
    TxStatus,
    MicrochainState,
    MicrochainConfig,
    TransactionData,
)
from .errors import (
    NetworkError,
    TransactionFailedError,
    AnchorFailedError,
    SdkError,
)


class OuroClient:
    """Main client for interacting with Ouroboros network"""

    def __init__(self, node_url: str):
        self.base_url = node_url.rstrip("/")
        self.session = requests.Session()
        self.session.headers.update({"Content-Type": "application/json"})

    def get_balance(self, address: str) -> Balance:
        """Get mainchain balance for address"""
        try:
            url = f"{self.base_url}/balance/{address}"
            response = self.session.get(url)
            response.raise_for_status()
            data = response.json()

            return Balance(
                address=address, balance=data["balance"], pending=data.get("pending", 0)
            )
        except requests.RequestException as e:
            raise NetworkError(str(e))

    def get_microchain_balance(self, microchain_id: str, address: str) -> int:
        """Get microchain balance"""
        try:
            url = f"{self.base_url}/microchain/{microchain_id}/balance/{address}"
            response = self.session.get(url)
            response.raise_for_status()
            data = response.json()

            return data["balance"]
        except requests.RequestException as e:
            raise NetworkError(str(e))

    def submit_transaction(self, tx: TransactionData) -> str:
        """Submit transaction to mainchain"""
        try:
            url = f"{self.base_url}/tx/submit"
            response = self.session.post(url, json=tx.to_dict())
            response.raise_for_status()
            data = response.json()

            if data.get("success"):
                return data["tx_id"]
            else:
                raise TransactionFailedError(data.get("message", "Unknown error"))
        except TransactionFailedError:
            raise
        except requests.RequestException as e:
            raise NetworkError(str(e))

    def get_transaction_status(self, tx_id: str) -> TxStatus:
        """Get transaction status"""
        try:
            url = f"{self.base_url}/tx/{tx_id}"
            response = self.session.get(url)
            response.raise_for_status()
            data = response.json()

            return TxStatus(data["status"])
        except requests.RequestException as e:
            raise NetworkError(str(e))

    def create_microchain(self, config: MicrochainConfig) -> str:
        """Create a new microchain"""
        try:
            url = f"{self.base_url}/microchain/create"
            response = self.session.post(url, json=config.to_dict())
            response.raise_for_status()
            data = response.json()

            if data.get("success"):
                return data["microchain_id"]
            else:
                raise SdkError(data.get("message", "Failed to create microchain"))
        except SdkError:
            raise
        except requests.RequestException as e:
            raise NetworkError(str(e))

    def get_microchain_state(self, microchain_id: str) -> MicrochainState:
        """Get microchain state"""
        try:
            url = f"{self.base_url}/microchain/{microchain_id}/state"
            response = self.session.get(url)
            response.raise_for_status()
            data = response.json()

            return MicrochainState.from_dict(data)
        except requests.RequestException as e:
            raise NetworkError(str(e))

    def list_microchains(self) -> List[MicrochainState]:
        """List all microchains"""
        try:
            url = f"{self.base_url}/microchains"
            response = self.session.get(url)
            response.raise_for_status()
            data = response.json()

            return [MicrochainState.from_dict(mc) for mc in data["microchains"]]
        except requests.RequestException as e:
            raise NetworkError(str(e))

    def anchor_microchain(self, microchain_id: str) -> str:
        """Trigger manual anchor for a microchain"""
        try:
            url = f"{self.base_url}/microchain/{microchain_id}/anchor"
            response = self.session.post(url)
            response.raise_for_status()
            data = response.json()

            if data.get("success"):
                return data["anchor_id"]
            else:
                raise AnchorFailedError(data.get("message", "Unknown error"))
        except AnchorFailedError:
            raise
        except requests.RequestException as e:
            raise NetworkError(str(e))

    def health_check(self) -> bool:
        """Check node health"""
        try:
            url = f"{self.base_url}/health"
            response = self.session.get(url)
            return response.status_code >= 200 and response.status_code < 300
        except requests.RequestException:
            return False
