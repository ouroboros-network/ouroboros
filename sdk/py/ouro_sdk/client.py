"""Ouroboros node HTTP client — sync and async variants."""

import json
import os
import urllib.error
import urllib.request
from typing import Any, Dict, Optional


class OuroClient:
    """
    Synchronous Ouroboros node client. Uses stdlib urllib — no dependencies required.

    Example::

        from ouro_sdk import OuroClient

        client = OuroClient("http://localhost:8000", api_key="ouro_abc123")
        print(client.health())
        print(client.balance("ouro1myaddress"))
        client.submit_transaction({
            "sender": "ouro1abc",
            "recipient": "ouro1xyz",
            "amount": 1_000_000,
        })
    """

    def __init__(
        self,
        api_url: str = "http://localhost:8000",
        api_key: Optional[str] = None,
    ):
        self.api_url = api_url.rstrip("/")
        self.api_key = api_key or os.getenv("OURO_API_KEY")

    def _headers(self) -> Dict[str, str]:
        h = {"Content-Type": "application/json", "Accept": "application/json"}
        if self.api_key:
            h["Authorization"] = f"Bearer {self.api_key}"
        return h

    def _get(self, path: str) -> Any:
        req = urllib.request.Request(
            f"{self.api_url}{path}", headers=self._headers()
        )
        try:
            with urllib.request.urlopen(req, timeout=10) as resp:
                return json.loads(resp.read())
        except urllib.error.HTTPError as e:
            body = e.read().decode("utf-8", errors="replace")
            raise RuntimeError(f"GET {path} failed: HTTP {e.code} {body}") from e

    def _post(self, path: str, body: dict) -> Any:
        data = json.dumps(body).encode()
        req = urllib.request.Request(
            f"{self.api_url}{path}",
            data=data,
            headers=self._headers(),
            method="POST",
        )
        try:
            with urllib.request.urlopen(req, timeout=10) as resp:
                return json.loads(resp.read())
        except urllib.error.HTTPError as e:
            body = e.read().decode("utf-8", errors="replace")
            raise RuntimeError(f"POST {path} failed: HTTP {e.code} {body}") from e

    # ─── Public endpoints ──────────────────────────────────────────────

    def health(self) -> dict:
        """GET /health — Node liveness."""
        return self._get("/health")

    def identity(self) -> dict:
        """GET /identity — Node ID, role, uptime, version."""
        return self._get("/identity")

    def consensus(self) -> dict:
        """GET /consensus — Current view, leader, QC, last block."""
        return self._get("/consensus")

    def peers(self) -> dict:
        """GET /peers — Connected peer list."""
        return self._get("/peers")

    def balance(self, address: str) -> dict:
        """GET /ouro/balance/:address — OURO coin balance."""
        return self._get(f"/ouro/balance/{address}")

    def nonce(self, address: str) -> dict:
        """GET /ouro/nonce/:address — Account nonce."""
        return self._get(f"/ouro/nonce/{address}")

    # ─── Protected endpoints ───────────────────────────────────────────

    def metrics(self) -> dict:
        """GET /metrics/json — TPS, block height, sync %, mempool, etc."""
        return self._get("/metrics/json")

    def resources(self) -> dict:
        """GET /resources — CPU, memory, disk usage."""
        return self._get("/resources")

    def mempool(self) -> dict:
        """GET /mempool — Current mempool."""
        return self._get("/mempool")

    def network_stats(self) -> dict:
        """GET /network/stats — Network statistics."""
        return self._get("/network/stats")

    def get_transaction(self, tx_id: str) -> dict:
        """GET /tx/:id — Get transaction by UUID or hash."""
        return self._get(f"/tx/{tx_id}")

    def submit_transaction(self, tx: dict) -> dict:
        """
        POST /tx/submit — Submit a transaction.

        Args:
            tx: dict with keys: sender, recipient, amount (nanoouro),
                optionally: signature (hex), nonce, idempotency_key
        """
        return self._post("/tx/submit", tx)

    def transfer(self, from_addr: str, to_addr: str, amount: int) -> dict:
        """
        POST /ouro/transfer — Transfer OURO between addresses.

        Args:
            from_addr: sender address
            to_addr: recipient address
            amount: amount in nanoouro (1 OURO = 1,000,000,000)
        """
        return self._post("/ouro/transfer", {"from": from_addr, "to": to_addr, "amount": amount})

    # ─── Convenience ──────────────────────────────────────────────────

    def status(self) -> dict:
        """Combined snapshot: health + identity + consensus + metrics. Never raises."""
        result: dict = {"online": False}
        try:
            result["health"] = self.health()
            result["online"] = True
            result["identity"] = self.identity()
            result["consensus"] = self.consensus()
            result["metrics"] = self.metrics()
        except Exception as e:
            result["error"] = str(e)
        return result


class AsyncOuroClient:
    """
    Async Ouroboros node client. Requires ``httpx`` (``pip install ouro-sdk[async]``).

    Example::

        from ouro_sdk import AsyncOuroClient

        async def main():
            client = AsyncOuroClient("http://localhost:8000", api_key="ouro_abc123")
            health = await client.health()
            balance = await client.balance("ouro1myaddress")
    """

    def __init__(
        self,
        api_url: str = "http://localhost:8000",
        api_key: Optional[str] = None,
    ):
        try:
            import httpx  # noqa: F401
        except ImportError as e:
            raise ImportError(
                "AsyncOuroClient requires httpx. Install it with: "
                "pip install ouro-sdk[async]"
            ) from e
        self.api_url = api_url.rstrip("/")
        self.api_key = api_key or os.getenv("OURO_API_KEY")

    def _headers(self) -> Dict[str, str]:
        h = {"Content-Type": "application/json", "Accept": "application/json"}
        if self.api_key:
            h["Authorization"] = f"Bearer {self.api_key}"
        return h

    async def _get(self, path: str) -> Any:
        import httpx
        async with httpx.AsyncClient() as client:
            resp = await client.get(
                f"{self.api_url}{path}", headers=self._headers(), timeout=10
            )
            resp.raise_for_status()
            return resp.json()

    async def _post(self, path: str, body: dict) -> Any:
        import httpx
        async with httpx.AsyncClient() as client:
            resp = await client.post(
                f"{self.api_url}{path}",
                json=body,
                headers=self._headers(),
                timeout=10,
            )
            resp.raise_for_status()
            return resp.json()

    async def health(self) -> dict:        return await self._get("/health")
    async def identity(self) -> dict:      return await self._get("/identity")
    async def consensus(self) -> dict:     return await self._get("/consensus")
    async def peers(self) -> dict:         return await self._get("/peers")
    async def metrics(self) -> dict:       return await self._get("/metrics/json")
    async def resources(self) -> dict:     return await self._get("/resources")
    async def mempool(self) -> dict:       return await self._get("/mempool")
    async def network_stats(self) -> dict: return await self._get("/network/stats")

    async def balance(self, address: str) -> dict:
        return await self._get(f"/ouro/balance/{address}")

    async def nonce(self, address: str) -> dict:
        return await self._get(f"/ouro/nonce/{address}")

    async def get_transaction(self, tx_id: str) -> dict:
        return await self._get(f"/tx/{tx_id}")

    async def submit_transaction(self, tx: dict) -> dict:
        return await self._post("/tx/submit", tx)

    async def transfer(self, from_addr: str, to_addr: str, amount: int) -> dict:
        return await self._post(
            "/ouro/transfer", {"from": from_addr, "to": to_addr, "amount": amount}
        )

    async def status(self) -> dict:
        import asyncio
        results = await asyncio.gather(
            self.health(), self.identity(), self.consensus(), self.metrics(),
            return_exceptions=True,
        )
        health, identity, consensus, metrics = results
        return {
            "online":    not isinstance(health, Exception),
            "identity":  None if isinstance(identity, Exception)  else identity,
            "consensus": None if isinstance(consensus, Exception)  else consensus,
            "metrics":   None if isinstance(metrics, Exception)   else metrics,
        }
