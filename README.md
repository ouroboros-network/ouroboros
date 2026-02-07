# Ouroboros Network

A high-performance decentralized blockchain network with adaptive difficulty, post-quantum security, and transparent rewards.

**Join the network in under 5 minutes | No database setup required | Earn OURO coins for validation**

---

## Quick Start

### Installation

#### Linux / macOS

```bash
bash <(curl -fsSL https://raw.githubusercontent.com/ouroboros-network/ouroboros/main/scripts/join_ouroboros.sh)
```

#### Windows (PowerShell or Command Prompt)

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://raw.githubusercontent.com/ouroboros-network/ouroboros/main/scripts/join_ouroboros.ps1 | iex"
```

#### Manual Download

Download the latest binary from [Releases](https://github.com/ouroboros-network/ouroboros/releases/latest) and run:

```bash
ouro start
```

### Running Your Node

```bash
# Start a new node
ouro start

# Join an existing network with a specific peer
ouro join --peer 136.112.101.176:9000

# Start in headless mode (no interactive dashboard)
ouro start --headless

# Register your node identity (saved to ~/.ouroboros/config.json)
ouro register-node --node-id <your-node-id>
```

### Quick Reference

```bash
ouro status          # View live node dashboard
ouro peers           # List connected peers
ouro consensus       # Show consensus status
ouro diagnose        # Run diagnostic checks
ouro --help          # See all available commands
```

---

## Adaptive Task Difficulty

Ouroboros features an adaptive difficulty system that automatically adjusts task difficulty based on your node's performance. This ensures optimal resource utilization while preventing system overload.

### How It Works

- Starts at **small** difficulty
- Auto-promotes based on proof generation speed
- Rewards scale with difficulty tier

### Hardware Benchmark

Run the built-in benchmark to instantly find the optimal difficulty for your hardware:

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

# Case-insensitive
ouro start --max-difficulty LARGE
```

### Difficulty Guidelines

| Difficulty | Use Case | Reward Multiplier |
|------------|----------|-------------------|
| small | Default, background tasks | 1x |
| medium | Standard desktop/laptop | 2x |
| large | High-performance systems | 4x |
| extra_large | Dedicated proving machines | 8x |

> **Tip:** Use the default adaptive system (no flags needed). The system will automatically find the optimal difficulty for your hardware. Only override if you're fine-tuning performance.

---

## Account & Transaction CLI

Ouroboros includes built-in account and transaction management:

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

## Midgard Wallet

For advanced wallet features, use the included Midgard wallet:

```bash
cd ouroboros/midgard_wallet
cargo build --release

# Check node status
cargo run --release -- --node-url http://localhost:8000 status

# Check your balance
cargo run --release -- --node-url http://localhost:8000 balance

# Send OURO
cargo run --release -- --node-url http://localhost:8000 send <recipient> <amount>
```

**Current Public Node:** `http://34.57.121.217:8000` (GCP Full Node)

> If you're running a local node, use `http://localhost:8000` instead.

---

## Post-Quantum Security

Ouroboros supports **Ed25519 + Dilithium5** hybrid signatures for quantum-resistant consensus. When enabled, block proposals and votes are signed with both classical and post-quantum keys.

```bash
# Enable post-quantum cryptography
ENABLE_PQ_CRYPTO=true ouro start
```

The migration follows a phased approach:
- **Phase 1:** Accept either Ed25519 or Hybrid signatures (current)
- **Phase 2:** Require Hybrid signatures for all consensus messages
- **Phase 3:** Dilithium-only (post-transition)

---

## Earn Rewards

Validators earn OURO coins scaled by their difficulty tier:

| Action | Base Reward | With 8x Multiplier |
|--------|-------------|---------------------|
| Block proposals | 20 OURO/block | 160 OURO/block |
| Block validation | 3 OURO/validation | 24 OURO/validation |
| Network uptime | 1.5 OURO/hour | 12 OURO/hour |

Check any validator's metrics:

```bash
curl http://localhost:8000/metrics/VALIDATOR_ADDRESS
curl http://localhost:8000/metrics/leaderboard
curl http://localhost:8000/rewards/YOUR_ADDRESS
```

---

## Docker

```bash
git clone https://github.com/ouroboros-network/ouroboros.git
cd ouroboros
docker-compose up -d
docker-compose logs     # Check logs
docker-compose down     # Shutdown
```

See [DOCKER.md](DOCKER.md) for full Docker documentation.

---

## What is Ouroboros?

Ouroboros is a hybrid Byzantine Fault Tolerant (BFT) blockchain that combines:

- **Lightweight nodes**: Run on any device with RocksDB embedded storage
- **Full validators**: High-performance nodes with full blockchain history
- **Adaptive difficulty**: Automatic performance tuning with reward multipliers
- **Post-quantum security**: Dilithium5 + Ed25519 hybrid signatures
- **Transparent rewards**: All validator contributions are publicly tracked
- **Decentralized P2P**: No central authority, fully distributed network

### Secure Consensus

HotStuff BFT consensus with:

- Post-quantum cryptography (Dilithium5 + Kyber1024)
- Adaptive difficulty and reputation-weighted rewards
- Leader rotation for fairness
- Byzantine fault tolerance (up to 1/3 malicious nodes)
- Liveness timer for dead leader detection

---

## System Requirements

### Lightweight Node (Community)

| Requirement | Minimum |
|-------------|---------|
| CPU | 1 core |
| RAM | 512MB |
| Storage | 1GB |
| OS | Linux, macOS, or Windows |

### Full Validator (Server)

| Requirement | Minimum |
|-------------|---------|
| CPU | 2+ cores |
| RAM | 2GB+ |
| Storage | 10GB+ |
| OS | Linux (Ubuntu 20.04+ recommended) |

> No external database required - RocksDB is embedded in the node binary.

---

## Monitoring Your Node

### Live Dashboard

```bash
ouro status              # Live-updating dashboard
ouro status --once       # Print once and exit
```

The dashboard shows: node status, difficulty tier, TPS, connected peers, consensus view, and uptime.

### Logs

**Linux:**
```bash
tail -f ~/.ouroboros/node.log
```

**Windows:**
```powershell
Get-Content $env:USERPROFILE\.ouroboros\node.log -Tail 50 -Wait
```

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

| Resource | Link |
|----------|------|
| Seed Node | `136.112.101.176:9000` |
| Public API | `http://34.57.121.217:8000` |
| API Documentation | [API_DOCUMENTATION.md](API_DOCUMENTATION.md) |

---

## Configuration

Node configuration is stored in `~/.ouroboros/config.json` and includes:

- Node identity (ID, public name)
- Adaptive difficulty settings (current tier, min/max overrides)
- Update preferences

Environment variables can be set in `~/.ouroboros/.env`. See [.env.example](ouro_dag/.env.example) for all options.

---

## Support

- **Issues**: [GitHub Issues](https://github.com/ouroboros-network/ouroboros/issues)
- **Network Stats**: Check `/metrics/leaderboard` endpoint
- **Updates**: Watch this repo for new releases

---

## Security

### Reporting Vulnerabilities

Please report security issues to the repository maintainer privately.

### Network Safety

- Never share your validator private keys
- Keep your node software updated
- Use firewall rules to protect your node
- Enable post-quantum mode for maximum security

---

## License

MIT License - see LICENSE file for details.

---

**Ready to join?** Run the quick start command for your platform above.
