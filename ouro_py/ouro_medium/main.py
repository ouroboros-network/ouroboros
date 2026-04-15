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
import os
import json
import uuid
import time
import hashlib
import socket
import logging
import aiohttp
from aiohttp import web

# Fix Windows asyncio: ProactorEventLoop raises ConnectionResetError [WinError 10054]
# on socket cleanup. SelectorEventLoop handles this gracefully.
if sys.platform == "win32":
    asyncio.set_event_loop_policy(asyncio.WindowsSelectorEventLoopPolicy())
from functools import wraps

logging.basicConfig(level=logging.INFO, format="[MediumNode] %(message)s")
log = logging.getLogger("medium")


# ─── Auth Middleware ────────────────────────────────────────────────

def load_api_keys():
    """Load API keys from environment (shared with Rust node config)."""
    keys_str = os.getenv("API_KEYS", "")
    if not keys_str:
        return set()
    return {k.strip() for k in keys_str.split(",") if k.strip()}


# Public routes that don't require auth
PUBLIC_ROUTES = {"/health", "/identity"}


@web.middleware
async def auth_middleware(request, handler):
    """Bearer token authentication matching the Rust node's auth system."""
    if request.path in PUBLIC_ROUTES:
        return await handler(request)

    api_keys = request.app.get("api_keys", set())
    if not api_keys:
        # SECURITY: Fail closed if no keys are configured in production
        # Only allow /health and /identity for monitoring
        log.warning(f"Unauthorized access attempt to {request.path} - No API keys configured")
        return web.json_response(
            {"error": "Node is secured. API keys must be configured to access this endpoint."},
            status=403,
        )

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


# ─── Medium Node ────────────────────────────────────────────────────

class MediumNode:
    def __init__(self, node_id: str, api_port: int = 8001):
        self.node_id = node_id
        self.api_port = api_port
        self.role = "medium"
        self.start_time = time.time()
        self.heavy_online = False
        self.shadow_mode = False
        self.heavy_addr = os.getenv("HEAVY_ADDR", "http://localhost:8000")

        # Real metrics tracking
        self.tx_count = 0
        self.batches_submitted = 0
        self.last_heavy_heartbeat = 0
        self.mempool = []

    async def get_identity(self, request):
        uptime = int(time.time() - self.start_time)
        return web.json_response({
            "node_id": self.node_id,
            "role": self.role,
            "public_name": f"Aggregator-{self.node_id[:8]}",
            "total_uptime_secs": uptime,
            "difficulty": "medium",
            "version": "0.2.0-py",
            "heavy_online": self.heavy_online,
            "shadow_mode": self.shadow_mode,
        })

    async def health_check(self, request):
        return web.json_response({
            "status": "ok",
            "node_name": f"Aggregator-{self.node_id[:8]}",
            "uptime_secs": int(time.time() - self.start_time),
        })

    async def get_metrics(self, request):
        uptime = max(1, int(time.time() - self.start_time))
        return web.json_response({
            "tx_total": self.tx_count,
            "tps_avg": round(self.tx_count / uptime, 2),
            "mempool_count": len(self.mempool),
            "batches_submitted": self.batches_submitted,
            "heavy_online": self.heavy_online,
            "shadow_mode": self.shadow_mode,
        })

    async def submit_tx(self, request):
        """Accept a transaction from a Light node for batching."""
        try:
            tx = await request.json()
        except Exception:
            return web.json_response({"error": "Invalid JSON"}, status=400)

        required = ["sender", "recipient", "amount"]
        for field in required:
            if field not in tx:
                return web.json_response(
                    {"error": f"Missing required field: {field}"}, status=400
                )

        tx["id"] = str(uuid.uuid4())
        tx["received_at"] = time.time()
        self.mempool.append(tx)
        self.tx_count += 1

        log.info(f"TX received: {tx['sender'][:8]}→{tx['recipient'][:8]} amt={tx['amount']}")
        return web.json_response({"status": "accepted", "tx_id": tx["id"]})

    async def get_mempool(self, request):
        return web.json_response({
            "count": len(self.mempool),
            "transactions": self.mempool[:50]  # Limit output
        })

    async def get_balance(self, request):
        """Fetch balance for a specific address. Proxies to Heavy node if available."""
        address = request.match_info.get("address", "")
        if not address:
            return web.json_response({"error": "Missing address"}, status=400)

        # Proxy to heavy node if online
        if self.heavy_online:
            try:
                # Heavy node uses /account/balance/<address> based on README/code
                # We need to find the exact endpoint. Let's try /account/balance/{address}
                url = f"{self.heavy_addr}/account/balance/{address}"
                async with aiohttp.ClientSession() as session:
                    async with session.get(url, timeout=5) as resp:
                        if resp.status == 200:
                            data = await resp.json()
                            return web.json_response(data)
                        else:
                            log.warning(f"Heavy node balance check failed: HTTP {resp.status}")
            except Exception as e:
                log.warning(f"Heavy node balance check error: {e}")

        # Fallback for demo/shadow mode: return 0 or a placeholder
        # In a real system, we'd query local subchain state here
        return web.json_response({
            "address": address,
            "balance": 0,
            "shadow_mode": self.shadow_mode
        })


    async def shutdown(self, request):
        """Graceful shutdown endpoint."""
        log.info("Shutdown requested via API")
        asyncio.get_event_loop().call_later(1, lambda: os._exit(0))
        return web.json_response({"status": "shutting_down"})

    # ─── Background Tasks ──────────────────────────────────────────

    async def monitor_heavy_nodes(self):
        """Heartbeat check against the Heavy node."""
        while True:
            await asyncio.sleep(30)
            try:
                async with aiohttp.ClientSession() as session:
                    async with session.get(
                        f"{self.heavy_addr}/health", timeout=aiohttp.ClientTimeout(total=5)
                    ) as resp:
                        if resp.status == 200:
                            self.heavy_online = True
                            self.last_heavy_heartbeat = time.time()
                            if self.shadow_mode:
                                log.info("Heavy node back online, exiting shadow mode")
                                self.shadow_mode = False
                        else:
                            self._handle_heavy_offline()
            except Exception:
                self._handle_heavy_offline()

    def _handle_heavy_offline(self):
        if self.heavy_online:
            log.warning("Heavy node unreachable — entering shadow mode")
        self.heavy_online = False
        self.shadow_mode = True

    async def _detect_public_addr(self) -> str:
        """Detect our public-facing IP for advertising to peers.

        Priority:
        1. PUBLIC_ADDR env var (most reliable — set this in production)
        2. External IP via ipify.org (works through NAT)
        3. Outbound socket local address (LAN IP — fallback only)
        4. Hostname resolution (last resort)
        """
        if os.getenv("PUBLIC_ADDR"):
            return os.getenv("PUBLIC_ADDR")

        # Try to get the actual public internet IP via an external service
        for url in ["https://api.ipify.org", "https://api4.my-ip.io/ip"]:
            try:
                async with aiohttp.ClientSession() as session:
                    async with session.get(
                        url, timeout=aiohttp.ClientTimeout(total=4)
                    ) as resp:
                        if resp.status == 200:
                            ip = (await resp.text()).strip()
                            if ip and len(ip) < 40:  # sanity check
                                log.info(f"Detected public IP: {ip}")
                                return ip
            except Exception:
                continue

        # Fallback: outbound socket (gives LAN IP behind NAT)
        try:
            with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as s:
                s.connect(("8.8.8.8", 80))
                return s.getsockname()[0]
        except Exception:
            pass
        try:
            return socket.gethostbyname(socket.gethostname())
        except Exception:
            return "127.0.0.1"

    async def advertise_to_heavy(self):
        """Periodically registers this aggregator in the Subchain Market."""
        public_ip = await self._detect_public_addr()
        log.info(f"Advertising with address: {public_ip}:{self.api_port}")

        while True:
            await asyncio.sleep(60)
            if not self.heavy_online:
                continue

            url = f"{self.heavy_addr}/subchain/advertise"
            api_keys = list(load_api_keys())
            headers = {}
            if api_keys:
                headers["Authorization"] = f"Bearer {api_keys[0]}"

            payload = {
                "subchain_id": f"subchain-{self.node_id[:8]}",
                "aggregator_node_id": self.node_id,
                "aggregator_addr": f"{public_ip}:{self.api_port}",  # Real IP, not localhost
                "app_type": os.getenv("APP_TYPE", "general"),
                "capacity_percent": max(0, 100 - len(self.mempool)),
                "last_seen": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                "reputation_score": 1.0,
            }
            try:
                async with aiohttp.ClientSession() as session:
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
        while True:
            await asyncio.sleep(10)
            if not self.mempool or not self.heavy_online:
                continue

            batch = self.mempool[:100]
            self.mempool = self.mempool[100:]

            # Compute batch root: SHA256 of concatenated tx IDs
            combined = "".join(tx.get("id", str(i)) for i, tx in enumerate(batch))
            batch_hash = hashlib.sha256(combined.encode()).digest()
            # serde_json Vec<u8> expects a JSON array of integers
            batch_root = list(batch_hash)

            url = f"{self.heavy_addr}/subchain/batch_anchor"
            api_keys = list(load_api_keys())
            headers = {"Content-Type": "application/json"}
            if api_keys:
                headers["Authorization"] = f"Bearer {api_keys[0]}"

            payload = {
                "batch_root": batch_root,
                "aggregator": self.node_id,
                "leaf_count": len(batch),
                "serialized_leaves_ref": None,
            }

            try:
                async with aiohttp.ClientSession() as session:
                    async with session.post(
                        url, json=payload, headers=headers,
                        timeout=aiohttp.ClientTimeout(total=10),
                    ) as resp:
                        if resp.status == 200:
                            log.info(
                                f"Anchored batch of {len(batch)} txs "
                                f"(root: {batch_hash.hex()[:12]}...)"
                            )
                            self.batches_submitted += 1
                        else:
                            log.warning(f"Batch anchor HTTP {resp.status} — re-queuing {len(batch)} txs")
                            self.mempool = batch + self.mempool
            except Exception as e:
                log.warning(f"Batch anchor error: {e} — re-queuing {len(batch)} txs")
                self.mempool = batch + self.mempool

    # ─── Server ─────────────────────────────────────────────────────

    async def run(self):
        log.info(f"--- Ouroboros Medium Node (Python) ---")
        log.info(f"ID: {self.node_id} | Port: {self.api_port}")

        app = web.Application(middlewares=[auth_middleware])
        app["api_keys"] = load_api_keys()

        if app["api_keys"]:
            log.info(f"Auth enabled ({len(app['api_keys'])} API key(s) loaded)")
        else:
            log.warning("No API_KEYS set — running in open access mode (dev only)")

        # Public routes
        app.router.add_get("/identity", self.get_identity)
        app.router.add_get("/health", self.health_check)
        app.router.add_get("/ouro/balance/{address}", self.get_balance)

        # Protected routes
        app.router.add_get("/metrics/json", self.get_metrics)
        app.router.add_post("/tx/submit", self.submit_tx)
        app.router.add_get("/mempool", self.get_mempool)
        app.router.add_post("/shutdown", self.shutdown)

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

    async def submit_heartbeat_loop(self):
        """Periodically submit heartbeats to the Heavy node to claim rewards."""
        from cryptography.hazmat.primitives.asymmetric import ed25519
        import binascii

        # Load signing key from environment
        key_hex = os.getenv("NODE_KEYPAIR_HEX")
        if not key_hex:
            log.warning("NODE_KEYPAIR_HEX not set - Heartbeats will be unsigned and REJECTED")
            return

        try:
            # Ed25519 private keys are 32 bytes. NODE_KEYPAIR_HEX might be 64 bytes (seed + pubkey)
            key_bytes = binascii.unhexlify(key_hex)
            if len(key_bytes) >= 32:
                signing_key = ed25519.Ed25519PrivateKey.from_private_bytes(key_bytes[:32])
                verifying_key = signing_key.public_key()
                pubkey_hex = binascii.hexlify(verifying_key.public_bytes_raw()).decode()
            else:
                log.error(f"Invalid NODE_KEYPAIR_HEX length: {len(key_bytes)}")
                return
        except Exception as e:
            log.error(f"Failed to load signing key: {e}")
            return

        wallet_address = os.getenv("NODE_WALLET_ADDRESS", "ouro1_placeholder")

        while True:
            await asyncio.sleep(60)
            if not self.heavy_online:
                continue

            # Heartbeat message format: "heartbeat:<node_id>:<wallet_address>:<timestamp>"
            # Added timestamp to prevent replay attacks as per Phase 3 of the plan
            timestamp = int(time.time())
            message = f"heartbeat:{self.node_id}:{wallet_address}:{timestamp}"
            
            try:
                signature = signing_key.sign(message.encode())
                sig_hex = binascii.hexlify(signature).decode()

                payload = {
                    "node_id": self.node_id,
                    "wallet_address": wallet_address,
                    "role": self.role,
                    "public_key": pubkey_hex,
                    "signature": sig_hex,
                    "timestamp": timestamp,
                }

                url = f"{self.heavy_addr}/rewards/heartbeat"
                api_keys = list(load_api_keys())
                headers = {"Content-Type": "application/json"}
                if api_keys:
                    headers["Authorization"] = f"Bearer {api_keys[0]}"

                async with aiohttp.ClientSession() as session:
                    async with session.post(url, json=payload, headers=headers, timeout=10) as resp:
                        if resp.status == 200:
                            log.info(f"Heartbeat submitted successfully (ts: {timestamp})")
                        else:
                            log.warning(f"Heartbeat submission failed: HTTP {resp.status}")
            except Exception as e:
                log.warning(f"Heartbeat submission error: {e}")


if __name__ == "__main__":
    node_id = os.getenv("NODE_ID", str(uuid.uuid4()))
    port = int(os.getenv("API_PORT", "8001"))
    node = MediumNode(node_id, port)
    asyncio.run(node.run())
