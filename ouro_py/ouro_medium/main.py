"""
Ouroboros Medium Node (Python)
Subchain Aggregator / Shadow Hub

Responsibilities:
- Batch microchain transactions into subchain blocks
- Advertise capacity to Heavy nodes via Subchain Market
- Monitor Heavy node liveness (shadow consensus readiness)
- Serve local API for Light nodes to submit transactions
"""

import asyncio
import sys
import hashlib
import os
import uuid
import time
import logging
import json
import binascii
import aiohttp
from aiohttp import web

# Fix Windows asyncio: ProactorEventLoop raises ConnectionResetError [WinError 10054]
# on socket cleanup. SelectorEventLoop handles this gracefully.
if sys.platform == "win32":
    asyncio.set_event_loop_policy(asyncio.WindowsSelectorEventLoopPolicy())

logging.basicConfig(level=logging.INFO, format="[MediumNode] %(message)s")
log = logging.getLogger("medium")


# â”€â”€â”€ Auth Middleware â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

def load_api_keys():
    """Load API keys from environment (shared with Rust node config)."""
    keys_str = os.getenv("API_KEYS", "")
    if not keys_str:
        return set()
    return {k.strip() for k in keys_str.split(",") if k.strip()}


PUBLIC_ROUTES = {"/health", "/identity", "/akasha/sensory"}


@web.middleware
async def auth_middleware(request, handler):
    """Bearer token authentication matching the Rust node's auth system."""
    if request.path in PUBLIC_ROUTES:
        return await handler(request)

    api_keys = request.app.get("api_keys", set())
    if not api_keys:
        return await handler(request)

    auth_header = request.headers.get("Authorization", "")
    if not auth_header.startswith("Bearer "):
        return web.json_response(
            {"error": "Missing or invalid Authorization header. Use: Bearer <api_key>"},
            status=401,
        )

    token = auth_header[7:].strip()

    # SECURITY: Use constant-time comparison to prevent timing attacks (Phase 4)
    import secrets
    authorized = any(secrets.compare_digest(token, key) for key in api_keys)

    if not authorized:
        return web.json_response({"error": "Invalid API key"}, status=403)

    return await handler(request)


# â”€â”€â”€ Medium Node â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

class MediumNode:
    def __init__(self, node_id: str, api_port: int = 8001):
        self.node_id = node_id
        self.api_port = api_port
        self.role = "medium"
        self.start_time = time.time()
        self.heavy_addr = os.getenv("HEAVY_ADDR", "http://localhost:8000")
        self.heavy_online = False
        self.mempool = []
        self.batches_submitted = 0

    async def get_identity(self, request):
        uptime = int(time.time() - self.start_time)
        return web.json_response({
            "node_id": self.node_id,
            "role": self.role,
            "public_name": f"Aggregator-{self.node_id[:8]}",
            "total_uptime_secs": uptime,
            "difficulty": "medium",
            "version": "0.3.0-py",
            "mempool_size": len(self.mempool),
            "batches_submitted": self.batches_submitted,
        })

    async def health_check(self, request):
        return web.json_response({
            "status": "ok",
            "node_name": f"Aggregator-{self.node_id[:8]}",
            "uptime_secs": int(time.time() - self.start_time),
            "heavy_node_status": "online" if self.heavy_online else "offline",
        })

    async def get_metrics(self, request):
        return web.json_response({
            "mempool_size": len(self.mempool),
            "batches_submitted": self.batches_submitted,
            "heavy_online": self.heavy_online,
        })

    async def get_balance(self, request):
        address = request.match_info["address"]
        url = f"{self.heavy_addr}/account/balance/{address}"
        async with aiohttp.ClientSession(connector=aiohttp.TCPConnector(ssl=False)) as session:
            async with session.get(url) as resp:
                return web.json_response(await resp.json(), status=resp.status)

    async def submit_tx(self, request):
        """Receive transaction from Light node and queue in local mempool."""
        try:
            tx = await request.json()
        except Exception:
            return web.json_response({"error": "Invalid JSON"}, status=400)

        # Basic structural check
        if not tx.get("sender") or not tx.get("recipient"):
            return web.json_response({"error": "Missing sender or recipient"}, status=400)

        self.mempool.append(tx)
        log.info(f"TX received: {tx['sender']}â†’{tx['recipient']} amt={tx.get('amount', 0)}")
        return web.json_response({"status": "queued", "tx_id": str(uuid.uuid4())})

    # â”€â”€â”€ Background Tasks â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    async def monitor_heavy_nodes(self):
        """Periodically checks if the Heavy node settlement layer is reachable."""
        while True:
            try:
                async with aiohttp.ClientSession(connector=aiohttp.TCPConnector(ssl=False)) as session:
                    async with session.get(f"{self.heavy_addr}/health", timeout=5) as resp:
                        if resp.status == 200:
                            if not self.heavy_online:
                                log.info(f"Settlement layer {self.heavy_addr} is ONLINE")
                            self.heavy_online = True
                        else:
                            self.heavy_online = False
            except Exception:
                if self.heavy_online:
                    log.warning(f"Settlement layer {self.heavy_addr} is OFFLINE")
                self.heavy_online = False
            await asyncio.sleep(5)

    async def _detect_public_addr(self):
        """Helper to find public IP for P2P advertising."""
        env_addr = os.getenv("PUBLIC_ADDR")
        if env_addr:
            return env_addr

        try:
            async with aiohttp.ClientSession() as session:
                async with session.get("https://api.ipify.org", timeout=5) as resp:
                    return await resp.text()
        except Exception:
            return "127.0.0.1"

    async def advertise_to_heavy(self):
        """Periodically registers this aggregator in the Subchain Market."""
        public_ip = await self._detect_public_addr()
        log.info(f"Advertising with address: {public_ip}:{self.api_port}")

        while True:
            await asyncio.sleep(5)
            if not self.heavy_online:
                continue

            url = f"{self.heavy_addr}/subchain/advertise"
            api_keys = list(load_api_keys())
            headers = {"Authorization": f"Bearer {api_keys[0]}"} if api_keys else {}

            payload = {
                "subchain_id": "default_subchain",
                "aggregator_node_id": self.node_id,
                "aggregator_addr": f"{public_ip}:{self.api_port}",
                "app_type": os.getenv("APP_TYPE", "general"),
                "capacity_percent": max(0, 100 - len(self.mempool)),
                "last_seen": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                "reputation_score": 1.0,
            }
            try:
                async with aiohttp.ClientSession(connector=aiohttp.TCPConnector(ssl=False)) as session:
                    async with session.post(
                        url, json=payload, headers=headers,
                        timeout=aiohttp.ClientTimeout(total=5),
                    ) as resp:
                        if resp.status == 200:
                            log.info("Advertised to Subchain Market")
                        else:
                            log.warning(f"Advertise failed: HTTP {resp.status}")
            except Exception as e:
                log.warning(f"Advertise failed: {e}")

    async def batch_flush(self):
        """Periodically flush mempool to Heavy node as a batch anchor."""
        from cryptography.hazmat.primitives.asymmetric import ed25519
        import binascii

        while True:
            await asyncio.sleep(10)
            if not self.mempool or not self.heavy_online:
                continue

            batch = self.mempool[:100]
            self.mempool = self.mempool[100:]

            # Load microchain signing key (for test purposes, the aggregator signs the leaf)
            key_hex = os.getenv("MICROCHAIN_KEYPAIR_HEX")
            signing_key = None
            if key_hex:
                try:
                    signing_key = ed25519.Ed25519PrivateKey.from_private_bytes(binascii.unhexlify(key_hex))
                except Exception as e:
                    log.error(f"Failed to load microchain key: {e}")

            # Wrap transactions into MicroAnchorLeaf structures
            leaves = []
            for tx in batch:
                timestamp = int(time.time())
                microchain_id_str = tx.get("microchain_id", "00000000-0000-0000-0000-000000000000")
                try:
                    # Convert to UUID and get raw 16 bytes
                    microchain_uuid_bytes = uuid.UUID(microchain_id_str).bytes
                except Exception:
                    microchain_uuid_bytes = b"\x00" * 16

                height = 0
                chain_id = "ouroboros-mainnet-1"
                micro_root = hashlib.sha256(json.dumps(tx).encode()).digest()
                
                # Construct payload: chain_id | microchain_id (16 bytes) | height BE (8) | micro_root | timestamp BE (8)
                payload_bytes = (
                    chain_id.encode() +
                    microchain_uuid_bytes +
                    height.to_bytes(8, 'big') +
                    micro_root +
                    timestamp.to_bytes(8, 'big')
                )
                
                sig_hex = "00" * 64
                if signing_key:
                    sig_hex = binascii.hexlify(signing_key.sign(payload_bytes)).decode()

                leaf = {
                    "microchain_id": microchain_id_str,
                    "height": height,
                    "micro_root_hex": binascii.hexlify(micro_root).decode(),
                    "timestamp": timestamp,
                    "sig_micro_hex": sig_hex,
                    "archive_url": None,
                    "tx_data": json.dumps(tx),
                    "chain_id": chain_id
                }
                leaves.append(leaf)

            serialized_leaves = json.dumps(leaves)
            batch_root = hashlib.sha256(serialized_leaves.encode()).hexdigest()

            payload = {
                "batch_root": list(binascii.unhexlify(batch_root)),
                "aggregator": self.node_id,
                "leaf_count": len(leaves),
                "serialized_leaves": serialized_leaves,
            }

            url = f"{self.heavy_addr}/subchain/batch_anchor"
            api_keys = list(load_api_keys())
            headers = {"Authorization": f"Bearer {api_keys[0]}"} if api_keys else {}

            try:
                async with aiohttp.ClientSession(connector=aiohttp.TCPConnector(ssl=False)) as session:
                    async with session.post(
                        url, json=payload, headers=headers,
                        timeout=aiohttp.ClientTimeout(total=10),
                    ) as resp:
                        if resp.status == 200:
                            log.info(
                                f"Batch anchored successfully: root={batch_root[:16]}... "
                                f"({len(batch)} txs)"
                            )
                            self.batches_submitted += 1
                        else:
                            log.warning(f"Batch anchor HTTP {resp.status} â€” re-queuing {len(batch)} txs")
                            self.mempool = batch + self.mempool
            except Exception as e:
                log.warning(f"Batch anchor error: {e} â€” re-queuing {len(batch)} txs")
                self.mempool = batch + self.mempool

    async def submit_heartbeat_loop(self):
        """Periodically submit heartbeats to the Heavy node to claim rewards."""
        while True:
            await asyncio.sleep(5)
            if not self.heavy_online:
                continue

            try:
                # Use a dummy signing key if not set
                from cryptography.hazmat.primitives.asymmetric import ed25519
                import binascii

                key_hex = os.getenv("NODE_KEYPAIR_HEX", "0" * 64)
                key_bytes = binascii.unhexlify(key_hex)
                signing_key = ed25519.Ed25519PrivateKey.from_private_bytes(key_bytes)
                pubkey_hex = binascii.hexlify(signing_key.public_key().public_bytes_raw()).decode()

                wallet_address = os.getenv("NODE_WALLET_ADDRESS", f"wallet-{self.node_id[:8]}")
                timestamp = int(time.time())
                nonce = "0"
                message = f"heartbeat:{self.node_id}:{wallet_address}:{timestamp}:{nonce}"
                signature = signing_key.sign(message.encode())
                sig_hex = binascii.hexlify(signature).decode()

                payload = {
                    "node_id": self.node_id,
                    "wallet_address": wallet_address,
                    "role": self.role,
                    "public_key": pubkey_hex,
                    "signature": sig_hex,
                    "timestamp": timestamp,
                    "nonce": nonce,
                }

                url = f"{self.heavy_addr}/rewards/heartbeat"
                api_keys = list(load_api_keys())
                headers = {"Authorization": f"Bearer {api_keys[0]}"} if api_keys else {}

                async with aiohttp.ClientSession(connector=aiohttp.TCPConnector(ssl=False)) as session:
                    async with session.post(url, json=payload, headers=headers, timeout=5) as resp:
                        if resp.status == 200:
                            log.info(f"Heartbeat submitted successfully (ts: {timestamp})")
                        else:
                            log.warning(f"Heartbeat submission failed: HTTP {resp.status}")
            except Exception as e:
                log.warning(f"Heartbeat submission error: {e}")

    async def run(self):
        log.info(f"--- Ouroboros Medium Node (Python) ---")
        log.info(f"ID: {self.node_id} | Port: {self.api_port}")

        app = web.Application(middlewares=[auth_middleware])
        app["api_keys"] = load_api_keys()

        if app["api_keys"]:
            log.info(f"Auth enabled ({len(app['api_keys'])} API key(s) loaded)")
        else:
            log.warning("No API_KEYS set â€” running in open access mode (dev only)")

        # Public routes
        app.router.add_get("/identity", self.get_identity)
        app.router.add_get("/health", self.health_check)
        app.router.add_get("/akasha/sensory", self.health_check)
        app.router.add_get("/ouro/balance/{address}", self.get_balance)

        # Protected routes
        app.router.add_get("/metrics/json", self.get_metrics)
        app.router.add_post("/tx/submit", self.submit_tx)

        runner = web.AppRunner(app)
        await runner.setup()
        site = web.TCPSite(runner, "0.0.0.0", self.api_port)

        log.info(f"API listening on http://0.0.0.0:{self.api_port}")

        await asyncio.gather(
            site.start(),
            self.monitor_heavy_nodes(),
            self.advertise_to_heavy(),
            self.batch_flush(),
            self.submit_heartbeat_loop(),
        )


if __name__ == "__main__":
    node_id = os.getenv("NODE_ID", str(uuid.uuid4()))
    port = int(os.getenv("API_PORT", "8001"))
    node = MediumNode(node_id, port)
    asyncio.run(node.run())
