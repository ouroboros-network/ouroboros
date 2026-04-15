"""HTTP client for interacting with Ouroboros network"""

import requests
from typing import List, Dict, Any
from .types import (
    Balance,
    TxStatus,
    MicrochainState,
    MicrochainConfig,
    TransactionData,
    Resources,
    BlockHeader,
)
from .errors import (
    NetworkError,
    TransactionFailedError,
    AnchorFailedError,
    SdkError,
)


class OuroClient:
    """Main client for interacting with Ouroboros network"""

    def __init__(self, node_url: str, api_key: str = None):
        self.base_url = node_url.rstrip("/")
        self.api_key = api_key
        self.session = requests.Session()
        headers = {"Content-Type": "application/json"}
        if self.api_key:
            headers["Authorization"] = f"Bearer {self.api_key}"
        self.session.headers.update(headers)

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
            # API might return 404 for new addresses on microchains
            if response.status_code == 404:
                return 0
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

    def get_resources(self) -> Resources:
        """Get node resource usage (CPU, RAM, Disk, Network)"""
        try:
            url = f"{self.base_url}/resources"
            response = self.session.get(url)
            response.raise_for_status()
            return Resources.from_dict(response.json())
        except requests.RequestException as e:
            raise NetworkError(str(e))

    def get_transaction_history(self, address: str, limit: int = 10) -> List[TransactionData]:
        """Get transaction history for an address"""
        try:
            url = f"{self.base_url}/ouro/transactions/{address}"
            response = self.session.get(url, params={"limit": limit})
            response.raise_for_status()
            data = response.json()
            return [TransactionData.from_dict(tx) for tx in data["transactions"]]
        except requests.RequestException as e:
            raise NetworkError(str(e))

    def get_blocks(self, limit: int = 10) -> List[BlockHeader]:
        """Get recent blocks"""
        try:
            url = f"{self.base_url}/blocks"
            response = self.session.get(url, params={"limit": limit})
            response.raise_for_status()
            data = response.json()
            return [BlockHeader.from_dict(b) for b in data["blocks"]]
        except requests.RequestException as e:
            raise NetworkError(str(e))
