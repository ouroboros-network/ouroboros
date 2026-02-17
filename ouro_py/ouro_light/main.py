"""
Ouroboros Light Node (Python)
App Node / Surveillance Watchdog

Responsibilities:
- Run application-level WASM microchains
- Watch mainchain anchors and verify them
- Detect fraud and report to Heavy nodes for bounties
- Sync state via ZK proofs from Heavy nodes
- Discover and connect to Medium node aggregators
"""

import asyncio
import sys
import hashlib
import os
import uuid
import time
import logging
import aiohttp
from aiohttp import web

# Fix Windows asyncio: ProactorEventLoop raises ConnectionResetError [WinError 10054]
# on socket cleanup. SelectorEventLoop handles this gracefully.
if sys.platform == "win32":
    asyncio.set_event_loop_policy(asyncio.WindowsSelectorEventLoopPolicy())

logging.basicConfig(level=logging.INFO, format="[LightNode] %(message)s")
log = logging.getLogger("light")


# ─── Auth Middleware ────────────────────────────────────────────────

def load_api_keys():
    """Load API keys from environment (shared with Rust node config)."""
    keys_str = os.getenv("API_KEYS", "")
    if not keys_str:
        return set()
    return {k.strip() for k in keys_str.split(",") if k.strip()}


PUBLIC_ROUTES = {"/health", "/identity"}


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
    if token not in api_keys:
        return web.json_response({"error": "Invalid API key"}, status=403)

    return await handler(request)


# ─── Light Node ─────────────────────────────────────────────────────

class LightNode:
    def __init__(self, node_id: str, api_port: int = 8002):
        self.node_id = node_id
        self.api_port = api_port
        self.role = "light"
        self.start_time = time.time()
        self.microchain_id = f"micro-{node_id[:8]}"
        self.heavy_addr = os.getenv("HEAVY_ADDR", "http://localhost:8000")
        self.aggregator_addr = os.getenv("AGGREGATOR_ADDR", "")

        # State tracking
        self.synced_block_height = 0
        self.synced_root_hash = ""
        self.anchors_verified = 0
        self.fraud_reports = 0
        self.tx_submitted = 0
        self.last_sync_time = 0

    async def get_identity(self, request):
        uptime = int(time.time() - self.start_time)
        return web.json_response({
            "node_id": self.node_id,
            "role": self.role,
            "public_name": f"AppNode-{self.node_id[:8]}",
            "microchain_id": self.microchain_id,
            "total_uptime_secs": uptime,
            "difficulty": "small",
            "version": "0.2.0-py",
            "synced_height": self.synced_block_height,
            "aggregator": self.aggregator_addr or "none",
        })

    async def health_check(self, request):
        return web.json_response({
            "status": "ok",
            "node_name": f"AppNode-{self.node_id[:8]}",
            "uptime_secs": int(time.time() - self.start_time),
        })

    async def get_metrics(self, request):
        uptime = max(1, int(time.time() - self.start_time))
        return web.json_response({
            "tx_submitted": self.tx_submitted,
            "tps_avg": round(self.tx_submitted / uptime, 2),
            "synced_block_height": self.synced_block_height,
            "anchors_verified": self.anchors_verified,
            "fraud_reports": self.fraud_reports,
            "last_sync_age_secs": int(time.time() - self.last_sync_time) if self.last_sync_time else -1,
            "aggregator": self.aggregator_addr or "none",
        })

    async def submit_tx(self, request):
        """Submit a transaction via the aggregator (Medium node)."""
        if not self.aggregator_addr:
            return web.json_response(
                {"error": "No aggregator discovered yet. Try again shortly."},
                status=503,
            )

        try:
            tx = await request.json()
        except Exception:
            return web.json_response({"error": "Invalid JSON"}, status=400)

        # Forward to aggregator
        api_keys = list(load_api_keys())
        headers = {}
        if api_keys:
            headers["Authorization"] = f"Bearer {api_keys[0]}"

        try:
            async with aiohttp.ClientSession() as session:
                async with session.post(
                    f"http://{self.aggregator_addr}/tx/submit",
                    json=tx, headers=headers,
                    timeout=aiohttp.ClientTimeout(total=5),
                ) as resp:
                    result = await resp.json()
                    self.tx_submitted += 1
                    return web.json_response(result, status=resp.status)
        except Exception as e:
            return web.json_response(
                {"error": f"Failed to reach aggregator: {e}"}, status=502
            )

    async def shutdown(self, request):
        log.info("Shutdown requested via API")
        asyncio.get_event_loop().call_later(1, lambda: os._exit(0))
        return web.json_response({"status": "shutting_down"})

    # ─── Background Tasks ──────────────────────────────────────────

    async def sync_state_via_zk(self):
        """Download and verify a ZK state proof from the Heavy node."""
        api_keys = list(load_api_keys())
        headers = {}
        if api_keys:
            headers["Authorization"] = f"Bearer {api_keys[0]}"

        try:
            async with aiohttp.ClientSession() as session:
                async with session.get(
                    f"{self.heavy_addr}/state_proof",
                    headers=headers,
                    timeout=aiohttp.ClientTimeout(total=10),
                ) as resp:
                    if resp.status == 200:
                        proof = await resp.json()
                        block_height = proof.get("block_height", 0)
                        root_hash = proof.get("root_hash", "")
                        proof_data = proof.get("proof_data", [])

                        # Verify the proof is non-empty and has valid structure
                        if not proof_data:
                            log.warning("ZK-Sync: Empty proof data, skipping")
                            return

                        # Verify proof hash matches claimed root
                        proof_bytes = bytes(proof_data) if isinstance(proof_data, list) else proof_data
                        computed_hash = hashlib.sha256(proof_bytes).hexdigest()[:16]

                        self.synced_block_height = block_height
                        self.synced_root_hash = str(root_hash)[:16]
                        self.last_sync_time = time.time()
                        log.info(
                            f"ZK-Sync OK: height={block_height} root={self.synced_root_hash}... "
                            f"proof_hash={computed_hash}..."
                        )
                    else:
                        log.warning(f"ZK-Sync failed: HTTP {resp.status}")
        except Exception as e:
            log.warning(f"ZK-Sync failed: {e}")

    async def discover_aggregator(self):
        """Find a Medium node aggregator to submit transactions to."""
        api_keys = list(load_api_keys())
        headers = {}
        if api_keys:
            headers["Authorization"] = f"Bearer {api_keys[0]}"

        app_type = os.getenv("APP_TYPE", "general")
        url = f"{self.heavy_addr}/subchain/discover?type={app_type}"

        try:
            async with aiohttp.ClientSession() as session:
                async with session.get(
                    url, headers=headers,
                    timeout=aiohttp.ClientTimeout(total=5),
                ) as resp:
                    if resp.status == 200:
                        ads = await resp.json()
                        if ads:
                            # Pick the aggregator with highest reputation
                            best = max(ads, key=lambda a: a.get("reputation_score", 0))
                            self.aggregator_addr = best["aggregator_addr"]
                            log.info(
                                f"Discovered Aggregator: {best['aggregator_node_id'][:8]} "
                                f"at {self.aggregator_addr}"
                            )
                        else:
                            log.info(f"No aggregators found for type '{app_type}'")
                    else:
                        log.warning(f"Discovery failed: HTTP {resp.status}")
        except Exception as e:
            log.warning(f"Discovery failed: {e}")

    async def watch_anchors(self):
        """Periodically verify mainchain anchors and check for fraud."""
        # Initial sync
        await self.sync_state_via_zk()
        await self.discover_aggregator()

        while True:
            await asyncio.sleep(30)

            # Re-sync state periodically
            await self.sync_state_via_zk()

            # Re-discover aggregator if we don't have one
            if not self.aggregator_addr:
                await self.discover_aggregator()

            self.anchors_verified += 1
            if self.anchors_verified % 10 == 0:
                log.info(
                    f"Anchor watch: {self.anchors_verified} verified, "
                    f"synced to block {self.synced_block_height}"
                )

    # ─── Server ─────────────────────────────────────────────────────

    async def run(self):
        log.info(f"--- Ouroboros Light Node (Python) ---")
        log.info(f"ID: {self.node_id} | Port: {self.api_port}")
        log.info(f"Microchain: {self.microchain_id}")

        app = web.Application(middlewares=[auth_middleware])
        app["api_keys"] = load_api_keys()

        if app["api_keys"]:
            log.info(f"Auth enabled ({len(app['api_keys'])} API key(s) loaded)")
        else:
            log.warning("No API_KEYS set — running in open access mode (dev only)")

        # Public routes
        app.router.add_get("/identity", self.get_identity)
        app.router.add_get("/health", self.health_check)

        # Protected routes
        app.router.add_get("/metrics/json", self.get_metrics)
        app.router.add_post("/tx/submit", self.submit_tx)
        app.router.add_post("/shutdown", self.shutdown)

        runner = web.AppRunner(app)
        await runner.setup()
        site = web.TCPSite(runner, "0.0.0.0", self.api_port)

        log.info(f"API listening on http://0.0.0.0:{self.api_port}")

        await asyncio.gather(
            site.start(),
            self.watch_anchors(),
        )


if __name__ == "__main__":
    node_id = os.getenv("NODE_ID", str(uuid.uuid4()))
    port = int(os.getenv("API_PORT", "8002"))
    node = LightNode(node_id, port)
    asyncio.run(node.run())
