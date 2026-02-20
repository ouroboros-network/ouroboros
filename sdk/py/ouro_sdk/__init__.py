"""
ouro-sdk — Python SDK for the Ouroboros blockchain network.

Quick start (sync):
    from ouro_sdk import OuroClient
    client = OuroClient("http://localhost:8000", api_key="your-key")
    print(client.health())
    print(client.balance("ouro1abc..."))

Quick start (async):
    from ouro_sdk import AsyncOuroClient
    client = AsyncOuroClient("http://localhost:8000", api_key="your-key")
    health = await client.health()
"""

from .client import OuroClient, AsyncOuroClient

__version__ = "1.0.0"
__all__ = ["OuroClient", "AsyncOuroClient"]
