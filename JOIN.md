# Join the Ouroboros Network

Welcome! This guide will help you join the Ouroboros blockchain network as a node operator.

## Choose Your Role

Ouroboros uses a three-tier architecture. Pick the role that matches your hardware and goals:

| Tier | Role | Reward | Requirements |
|------|------|--------|--------------|
| **Heavy** | BFT consensus, global finality | 1.0x multiplier | 8+ cores, 16GB RAM, 1TB SSD |
| **Medium** | Subchain aggregation, batch ordering | 0.5x + fees | 4+ cores, 8GB RAM, 500GB SSD, Python 3.10+ |
| **Light** | App nodes, fraud detection | 0.1x + bounties | Any modern device, Python 3.10+ |

> **Default**: If you don't specify a role, you'll run as a **Heavy** node.

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

```bash
chmod +x ouro-*          # Linux/macOS only
./ouro-linux-x64 start   # Replace with your platform binary
```

## After Installation

### Start Your Node

```bash
# Start as Heavy node (default)
ouro start

# Start as Medium node (subchain aggregator)
ouro start --role medium

# Start as Light node (app runner / watchdog)
ouro start --role light

# Join with a specific peer
ouro join --peer 136.112.101.176:9000
```

### Verify It's Running

```bash
# Check health
curl http://localhost:8000/health
# Should return: {"status":"ok","version":"1.4.1",...}

# Live dashboard
ouro status

# Check peers
ouro peers
```

### Choose Your Difficulty (Heavy Nodes)

Heavy nodes can select ZK proof difficulty for higher rewards:

```bash
# Benchmark your hardware first
ouro benchmark

# Start with custom difficulty range
ouro start --min-difficulty medium --max-difficulty extra_large
```

| Difficulty | Reward Multiplier |
|------------|-------------------|
| small | 1x (default) |
| medium | 2x |
| large | 4x |
| extra_large | 8x |

## Configuration

Your node config is stored at `~/.ouroboros/config.json`. Key settings:

| Setting | Default | Description |
|---------|---------|-------------|
| API port | `8000` (Heavy), `8001` (Medium), `8002` (Light) | REST API |
| P2P port | `9000` | Peer-to-peer networking |
| Role | `heavy` | Node tier |

### Environment Variables

You can override config with environment variables:

```bash
NODE_ROLE=medium        # Override node role
API_ADDR=0.0.0.0:8001  # Custom API address
API_KEYS=your_key       # API authentication key
ENABLE_PQ_CRYPTO=true   # Enable post-quantum signatures
```

## Seed Nodes

Your node will automatically connect to these bootstrap peers:

- `136.112.101.176:9000` (Primary seed)
- `34.57.121.217:9000` (Secondary seed)

Public API: `http://34.57.121.217:8000`

## Firewall Configuration

Open these ports if you want other nodes to connect to you (recommended):

- **9000** - P2P networking (required)
- **8000** - API (optional, for public API hosting)

**Linux (ufw):**
```bash
sudo ufw allow 9000/tcp
```

**Windows Firewall:**
```powershell
New-NetFirewallRule -DisplayName "Ouroboros P2P" -Direction Inbound -LocalPort 9000 -Protocol TCP -Action Allow
```

## Monitoring Your Node

```bash
# Live dashboard with peer count, consensus, rewards
ouro status

# One-shot status check
ouro status --once

# Run diagnostics
ouro diagnose

# Check consensus state
ouro consensus
```

### API Monitoring

```bash
# Health check
curl http://localhost:8000/health

# Detailed health with subsystem status
curl http://localhost:8000/health/detailed

# Performance metrics (Prometheus format)
curl http://localhost:8000/metrics

# Resource usage
curl http://localhost:8000/resources
```

## Earning Rewards

| Action | Heavy | Medium | Light |
|--------|-------|--------|-------|
| Block proposals | 20 OURO | 10 OURO | 2 OURO |
| Block validation | 3 OURO | 1.5 OURO | 0.3 OURO |
| Network uptime | 1 OURO/day | 0.5 OURO/day | 0.1 OURO/day |
| Fraud detection | - | - | Bounty |

```bash
# Check your balance
ouro account balance

# Claim pending rewards
curl -H "Authorization: Bearer YOUR_API_KEY" http://localhost:8000/rewards/claim -X POST
```

## Troubleshooting

### Node won't start

1. Check the error log in your terminal output
2. Verify ports aren't already in use: `netstat -an | grep 9000`
3. Try running diagnostics: `ouro diagnose`

### Can't connect to seed nodes

1. Verify internet connectivity: `ping 136.112.101.176`
2. Check firewall isn't blocking outbound port 9000
3. The node will run standalone and retry connections automatically

### Config migration error

If upgrading from an older version and seeing `missing field` errors, download the latest release which handles config migration automatically.

### High CPU usage

Normal during initial sync and ZK proof generation. CPU usage stabilizes after sync completes. Use `ouro benchmark` to find optimal difficulty for your hardware.

## Security Notes

1. **Keep your private keys safe** - stored in `~/.ouroboros/`
2. **Never share your `BFT_SECRET_SEED`** - it's your node's identity
3. **Keep your node updated** - run the install command again to upgrade
4. **Enable post-quantum mode** for maximum security: `ENABLE_PQ_CRYPTO=true`
5. **API keys** are required for all state-changing operations

## Getting Help

- **GitHub Issues:** https://github.com/ouroboros-network/ouroboros/issues
- **All commands:** `ouro --help`
- **Role details:** `ouro roles`

---

**Welcome to the Ouroboros Network!** Your participation strengthens decentralization and earns you OURO coins.
