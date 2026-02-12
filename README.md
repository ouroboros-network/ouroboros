# Ouroboros Network

A high-performance decentralized blockchain network with adaptive difficulty, post-quantum security, and transparent rewards.

**Join the network in under 5 minutes | No database setup required | Earn OURO coins for validation**

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

Download the latest binary for your platform from [Releases](https://github.com/ouroboros-network/ouroboros/releases/latest):

| Platform | Binary |
|----------|--------|
| Linux x64 | `ouro-linux-x64` |
| macOS x64 (Intel) | `ouro-macos-x64` |
| macOS ARM64 (Apple Silicon) | `ouro-macos-arm64` |
| Windows x64 | `ouro-windows-x64.exe` |

Then run:

```bash
chmod +x ouro-*        # Linux/macOS only
./ouro-linux-x64 start # Replace with your platform binary
```

---

## Running Your Node

```bash
# Start a new node
ouro start

# Join an existing network with a specific peer
ouro join --peer 136.112.101.176:9000

# Start in headless mode (no interactive dashboard)
ouro start --headless

# Register your node identity
ouro register-node --node-id <your-node-id>
```

### Quick Reference

```bash
ouro status          # View live node dashboard
ouro status --once   # Print status once and exit
ouro peers           # List connected peers
ouro consensus       # Show consensus status
ouro diagnose        # Run diagnostic checks
ouro --help          # See all available commands
```

---

## Adaptive Task Difficulty

Ouroboros features an adaptive difficulty system that automatically adjusts based on your node's performance, ensuring optimal resource utilization.

### How It Works

- Starts at **small** difficulty
- Auto-promotes based on proof generation speed
- Rewards scale with difficulty tier

### Hardware Benchmark

Run the built-in benchmark to find the optimal difficulty for your hardware:

```bash
ouro benchmark
ouro benchmark --cycles 20    # More cycles for accuracy
```

### Difficulty Override

```bash
# Lower difficulty for resource-constrained systems
ouro start --max-difficulty small

# Higher difficulty for powerful hardware
ouro start --min-difficulty medium --max-difficulty extra_large
```

### Difficulty Tiers

| Difficulty | Use Case | Reward Multiplier |
|------------|----------|-------------------|
| small | Default, background tasks | 1x |
| medium | Standard desktop/laptop | 2x |
| large | High-performance systems | 4x |
| extra_large | Dedicated proving machines | 8x |

---

## Account & Transactions

```bash
# Generate a new keypair
ouro account new

# Check your balance
ouro account balance
ouro account balance <address>

# Send OURO to another address
ouro tx send --to <recipient-address> --amount 10.5
```

---

## Post-Quantum Security

Ouroboros supports **Ed25519 + Dilithium5** hybrid signatures for quantum-resistant consensus. When enabled, block proposals and votes are signed with both classical and post-quantum keys.

```bash
# Enable post-quantum cryptography
ENABLE_PQ_CRYPTO=true ouro start
```

Migration phases:
- **Phase 1:** Accept Ed25519 or Hybrid signatures (current)
- **Phase 2:** Require Hybrid signatures for all consensus
- **Phase 3:** Dilithium-only (post-transition)

---

## Earn Rewards

Validators earn OURO coins for contributing to the network:

| Action | Base Reward |
|--------|-------------|
| Block proposals | 20 OURO/block |
| Block validation | 3 OURO/validation |
| Network uptime | 1.5 OURO/hour |

Check validator metrics:

```bash
curl http://localhost:8000/metrics/leaderboard
curl http://localhost:8000/rewards/YOUR_ADDRESS
```

---

## What is Ouroboros?

Ouroboros is a hybrid Byzantine Fault Tolerant (BFT) blockchain that combines:

- **Lightweight nodes** — Run on any device with RocksDB embedded storage
- **Adaptive difficulty** — Automatic performance tuning with reward multipliers
- **Post-quantum security** — Dilithium5 + Ed25519 hybrid signatures
- **HotStuff BFT consensus** — Leader rotation, liveness timer, Byzantine fault tolerance
- **Transparent rewards** — All validator contributions publicly tracked
- **Decentralized P2P** — No central authority, fully distributed network

---

## System Requirements

### Lightweight Node

| Requirement | Minimum |
|-------------|---------|
| CPU | 1 core |
| RAM | 512 MB |
| Storage | 1 GB |
| OS | Linux, macOS, or Windows |

### Full Validator

| Requirement | Minimum |
|-------------|---------|
| CPU | 2+ cores |
| RAM | 2 GB+ |
| Storage | 10 GB+ |
| OS | Linux (Ubuntu 20.04+ recommended) |

> No external database required — RocksDB is embedded in the node binary.

---

## Monitoring Your Node

### Live Dashboard

```bash
ouro status              # Live-updating dashboard
ouro status --once       # Print once and exit
```

The dashboard shows: node status, difficulty tier, TPS, connected peers, consensus view, and uptime.

### API Endpoints

```bash
curl http://localhost:8000/health            # Health check
curl http://localhost:8000/health/detailed   # Detailed health
curl http://localhost:8000/identity          # Node identity & difficulty
curl http://localhost:8000/metrics/json      # Metrics (JSON)
curl http://localhost:8000/resources         # System resource usage
```

---

## Network Information

| Resource | Value |
|----------|-------|
| Seed Node | `136.112.101.176:9000` |
| Public API | `http://34.57.121.217:8000` |
| Releases | [GitHub Releases](https://github.com/ouroboros-network/ouroboros/releases) |

---

## Security

- Never share your validator private keys
- Keep your node software updated
- Use firewall rules to protect your node
- Enable post-quantum mode for maximum security

Report security vulnerabilities to the repository maintainer privately.

---

## Support

- **Issues**: [GitHub Issues](https://github.com/ouroboros-network/ouroboros/issues)
- **Network Stats**: Check `/metrics/leaderboard` endpoint
- **Updates**: Watch this repo for new releases

---

**Ready to join?** Run the quick start command for your platform above.
