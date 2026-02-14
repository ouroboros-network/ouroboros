# Ouroboros Network

A high-performance decentralized blockchain with tiered node architecture, post-quantum security, zero-knowledge proofs, and on-chain governance.

**Join the network in under 5 minutes | Choose your role: Heavy, Medium, or Light | Earn OURO coins**

---

## Quick Start

### Linux / macOS

```bash
curl -fsSL https://raw.githubusercontent.com/ouroboros-network/ouroboros/main/install.sh | bash
```

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/ouroboros-network/ouroboros/main/scripts/join_ouroboros.ps1 | iex
```

### Windows (Command Prompt)

```cmd
powershell -ExecutionPolicy Bypass -c "irm https://raw.githubusercontent.com/ouroboros-network/ouroboros/main/scripts/join_ouroboros.ps1 | iex"
```

### Manual Download

Download the latest binary from [Releases](https://github.com/ouroboros-network/ouroboros/releases/latest):

| Platform | Binary |
|----------|--------|
| Linux x64 | `ouro-linux-x64` |
| macOS x64 (Intel) | `ouro-macos-x64` |
| macOS ARM64 (Apple Silicon) | `ouro-macos-arm64` |
| Windows x64 | `ouro-windows-x64.exe` |

```bash
chmod +x ouro-*        # Linux/macOS only
./ouro-linux-x64 start # Replace with your platform binary
```

---

## Architecture

Ouroboros uses a **three-tier node architecture** that separates responsibilities for scalability and accessibility:

```
                    HEAVY NODES (Rust)
                 Global Settlement Layer
          BFT Consensus | Full DAG | Finality
               /                    \
        MEDIUM NODES (Python)    MEDIUM NODES (Python)
        Subchain Aggregators     Shadow Consensus Hubs
        Batch Ordering           Cross-tier Relay
           /        \                /        \
    LIGHT NODES   LIGHT NODES  LIGHT NODES  LIGHT NODES
    App Runners   Watchdogs    Microchains  Fraud Detectors
```

### Node Tiers

| Tier | Language | Role | Reward | Hardware |
|------|----------|------|--------|----------|
| **Heavy** | Rust | BFT consensus, global finality, fraud adjudication | 1.0x multiplier | 8+ cores, 16GB RAM, 1TB SSD |
| **Medium** | Python | Subchain aggregation, batch ordering, shadow consensus | 0.5x + aggregation fees | 4+ cores, 8GB RAM, 500GB SSD |
| **Light** | Python | App microchains, anchor verification, fraud bounties | 0.1x + fraud bounties | Any modern device |

```bash
# See detailed role information
ouro roles

# Start with a specific role
ouro start --role heavy    # Full validator (default)
ouro start --role medium   # Subchain aggregator
ouro start --role light    # App node / watchdog
```

---

## Running Your Node

```bash
# Start a new node (defaults to Heavy role)
ouro start

# Join an existing network
ouro join --peer 136.112.101.176:9000

# Start in headless mode
ouro start --headless

# Register your node identity
ouro register-node --node-id <your-node-id>
```

### Quick Reference

```bash
ouro status          # Live node dashboard
ouro status --once   # Print status once and exit
ouro peers           # List connected peers
ouro consensus       # Show consensus status
ouro diagnose        # Run diagnostic checks
ouro roles           # Show tier details
ouro benchmark       # Benchmark your hardware
ouro --help          # See all commands
```

---

## Key Features

### Post-Quantum Security

Ed25519 + Dilithium5 hybrid signatures protect against quantum computing threats.

```bash
ENABLE_PQ_CRYPTO=true ouro start
```

Migration path: Ed25519-only -> Hybrid (current) -> Dilithium-only (future)

### Zero-Knowledge Proofs

Groth16 on BN254 curve for private transactions with adaptive difficulty:

```bash
ouro benchmark               # Find optimal ZK difficulty
ouro start --min-difficulty medium --max-difficulty extra_large
```

| Difficulty | Proof Speed | Reward Multiplier |
|------------|-------------|-------------------|
| small | Default | 1x |
| medium | < 5 seconds | 2x |
| large | < 2 seconds | 4x |
| extra_large | < 500ms | 8x |

### ZK State Proofs

Light nodes sync without replaying the full chain. Heavy nodes serve cryptographic state proofs at `/state_proof` that commit to the global state via SHA-256 hash chains.

### Subchain Market

Medium nodes advertise their capacity and Light nodes discover aggregators automatically:

```bash
# Medium nodes auto-advertise to Heavy nodes
# Light nodes auto-discover aggregators

# API endpoints (on Heavy nodes)
curl http://localhost:8000/subchain/discover?type=gaming
```

### On-Chain Governance

Token holders submit and vote on proposals for protocol changes, treasury spending, and upgrades:

- Proposal threshold: 1,000 OURO staked
- Voting period: 7 days
- Quorum: 33% of circulating supply
- Approval: >50% of non-abstain votes

### Fraud Detection

Built-in intrusion detection with auto-escalation:
- Rate limit violation tracking
- Authentication failure monitoring
- Automatic IP blocking after threshold
- Webhook alerts for security events

### Native Contracts

Built-in contracts execute at native speed:
- Token transfers (21,000 gas)
- Staking / unstaking (50,000 gas)
- Cross-chain bridge lock/mint (100,000 gas)
- Governance vote/propose

---

## Account & Transactions

```bash
# Generate a new keypair
ouro account new

# Check balance
ouro account balance
ouro account balance <address>

# Send OURO
ouro tx send --to <recipient-address> --amount 10.5
```

---

## Earn Rewards

Rewards scale with your node tier and difficulty:

| Action | Base Reward | Heavy | Medium | Light |
|--------|-------------|-------|--------|-------|
| Block proposals | 20 OURO/block | 20 | 10 | 2 |
| Block validation | 3 OURO/validation | 3 | 1.5 | 0.3 |
| Network uptime | 1 OURO/day | 1 | 0.5 | 0.1 |
| Fraud detection | Bounty | - | - | Variable |

```bash
curl http://localhost:8000/rewards/heartbeat   # Submit heartbeat
curl http://localhost:8000/rewards/claim       # Claim pending rewards
```

---

## API Reference

### Public Endpoints (no auth required)

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Health check |
| `/health/detailed` | GET | Detailed health with subsystems |
| `/identity` | GET | Node identity, role, and config |
| `/metrics/json` | GET | Performance metrics (JSON) |
| `/metrics` | GET | Prometheus format metrics |
| `/resources` | GET | CPU, memory, disk usage |
| `/peers` | GET | Connected peer list |
| `/state_proof` | GET | ZK state proof for light sync |
| `/network/stats` | GET | Network I/O statistics |

### Protected Endpoints (require `Authorization: Bearer <api_key>`)

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/tx/submit` | POST | Submit a transaction |
| `/mempool` | GET | View pending transactions |
| `/tx/:id` | GET | Look up transaction by ID |
| `/rewards/heartbeat` | POST | Submit uptime heartbeat |
| `/rewards/claim` | POST | Claim pending rewards |
| `/shutdown` | POST | Graceful node shutdown |
| `/validators/stakes` | GET | Current validator stakes |
| `/slashing/events` | GET | Recent slashing events |
| `/subchain/advertise` | POST | Advertise subchain (Medium) |
| `/subchain/discover` | GET | Discover aggregators (Light) |

---

## SDKs & Tools

### Midgard Wallet

CLI wallet for managing OURO coins. See [`midgard_wallet/`](midgard_wallet/) for details.

```bash
cd midgard_wallet && cargo build --release
./midgard new-wallet
./midgard balance
./midgard send --to <address> --amount 10
```

### Rust SDK

```toml
[dependencies]
ouro-sdk = { path = "ouro_sdk" }
```

See [`ouro_sdk/`](ouro_sdk/) for the Rust SDK documentation.

### JavaScript/TypeScript SDK

```bash
npm install ouro-sdk
```

See [`ouro_sdk_js/`](ouro_sdk_js/) for the JS/TS SDK documentation.

### Python SDK

```bash
pip install ouro-sdk
```

See [`ouro_sdk_python/`](ouro_sdk_python/) for the Python SDK documentation.

---

## System Requirements

### Heavy Node (Full Validator)

| Requirement | Minimum | Recommended |
|-------------|---------|-------------|
| CPU | 4 cores | 8+ cores |
| RAM | 8 GB | 16 GB+ |
| Storage | 100 GB SSD | 1 TB NVMe |
| Network | 100 Mbps | 1 Gbps |
| OS | Linux, macOS, Windows | Ubuntu 22.04+ |

### Medium Node (Aggregator)

| Requirement | Minimum |
|-------------|---------|
| CPU | 2+ cores |
| RAM | 4 GB |
| Storage | 50 GB SSD |
| Network | Stable connection |
| Requires | Python 3.10+ |

### Light Node (App/Watchdog)

| Requirement | Minimum |
|-------------|---------|
| CPU | 1 core |
| RAM | 512 MB |
| Storage | 1 GB |
| Requires | Python 3.10+ |

> No external database required. RocksDB is embedded in the Heavy node binary.

---

## Network Information

| Resource | Value |
|----------|-------|
| Seed Nodes | `136.112.101.176:9000`, `34.57.121.217:9000` |
| Public API | `http://34.57.121.217:8000` |
| Default API Port | `8000` (Heavy), `8001` (Medium), `8002` (Light) |
| Default P2P Port | `9000` |
| Releases | [GitHub Releases](https://github.com/ouroboros-network/ouroboros/releases) |

---

## Project Structure

```
ouroboros/
  ouro_dag/          # Core node (Rust) - Heavy node binary
  ouro_py/           # Python tier implementations
    ouro_medium/     # Medium node (subchain aggregator)
    ouro_light/      # Light node (app runner / watchdog)
  midgard_wallet/    # CLI wallet for OURO coins
  ouro_sdk/          # Rust SDK
  ouro_sdk_js/       # JavaScript/TypeScript SDK
  ouro_sdk_python/   # Python SDK
  scripts/           # Install and join scripts
  deploy/            # Deployment configurations
```

---

## Security

- Never share your validator private keys or `BFT_SECRET_SEED`
- Keep your node software updated
- Use firewall rules to protect your node's P2P port
- Enable post-quantum mode for maximum security
- API keys are required for all state-changing operations

Report security vulnerabilities privately to the repository maintainer.

---

## License

See [LICENSE](LICENSE) for details.

---

**Ready to join?** Run the quick start command for your platform above, or check `ouro roles` to pick your tier.
