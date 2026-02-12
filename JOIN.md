# Join the Ouroboros Network

Welcome! This guide will help you join the Ouroboros blockchain network as a node operator.

## What You Need

- A computer with internet connection
- **Linux/Mac:** 2GB RAM minimum, 4GB recommended
- **Windows:** 4GB RAM minimum, 8GB recommended
- 20GB free disk space
- Basic terminal/command line knowledge

## Quick Start

### Linux / Mac

```bash
curl -sSL https://raw.githubusercontent.com/ouroboros-network/ouroboros/main/scripts/join_ouroboros.sh | bash
```

Or download and run manually:

```bash
wget https://raw.githubusercontent.com/ouroboros-network/ouroboros/main/scripts/join_ouroboros.sh
chmod +x join_ouroboros.sh
./join_ouroboros.sh
```

### Windows

1. **Download PowerShell script:**
   - Go to: https://raw.githubusercontent.com/ouroboros-network/ouroboros/main/scripts/join_ouroboros.ps1
   - Right-click → Save As → `join_ouroboros.ps1`

2. **Run as Administrator:**
   - Right-click PowerShell → "Run as Administrator"
   - Navigate to download folder
   - Run: `.\join_ouroboros.ps1`

## Manual Setup (All Platforms)

If you prefer to set up manually:

### 1. Install Dependencies

**Ubuntu/Debian:**
```bash
sudo apt update
sudo apt install -y build-essential pkg-config libssl-dev postgresql postgresql-contrib curl git
```

**macOS:**
```bash
brew install postgresql rust git
```

**Windows:**
- Install [Rust](https://rustup.rs/)
- Install [PostgreSQL](https://www.postgresql.org/download/windows/)
- Install [Git](https://git-scm.com/download/win)

### 2. Install Rust

**Linux/Mac:**
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source $HOME/.cargo/env
```

**Windows:** Download from https://rustup.rs/

### 3. Clone Repository

```bash
git clone https://github.com/ouroboros-network/ouroboros.git
cd ouroboros/ouro_dag
```

### 4. Setup Database

**Linux/Mac:**
```bash
sudo systemctl start postgresql
sudo -u postgres psql -c "CREATE USER ouro WITH PASSWORD 'ouro_pass';"
sudo -u postgres psql -c "CREATE DATABASE ouro_db OWNER ouro;"
```

**Windows:**
```powershell
psql -U postgres -c "CREATE USER ouro WITH PASSWORD 'ouro_pass';"
psql -U postgres -c "CREATE DATABASE ouro_db OWNER ouro;"
```

### 5. Configure Node

Create a `.env` file in the `ouro_dag` directory:

```bash
NODE_ID=node-YOUR_UNIQUE_ID
DATABASE_URL=postgres://ouro:ouro_pass@localhost:5432/ouro_db
ROCKSDB_PATH=/path/to/your/data/rocksdb
API_ADDR=0.0.0.0:8000
LISTEN_ADDR=0.0.0.0:9001
BFT_PORT=9091
BFT_PEERS=34.173.167.150:9091
BFT_SECRET_SEED=YOUR_RANDOM_32_BYTE_HEX
RUST_LOG=info
ENABLE_UPNP=false
ENABLE_TOR=false
API_KEYS=your_api_key
```

Generate secrets:
```bash
# Node ID
echo "node-$(openssl rand -hex 4)"

# BFT Secret
openssl rand -hex 32
```

### 6. Build the Node

```bash
cargo build --release -j 2
```

This takes 15-30 minutes depending on your hardware.

### 7. Run Migrations

```bash
export DATABASE_URL=postgres://ouro:ouro_pass@localhost:5432/ouro_db
./target/release/migrate
```

### 8. Start Your Node

**Linux/Mac:**
```bash
nohup ./target/release/ouro_dag start > ~/ouro_node.log 2>&1 &
```

**Windows:**
```powershell
Start-Process .\target\release\ouro_dag.exe -ArgumentList "start"
```

### 9. Verify It's Running

```bash
# Check health
curl http://localhost:8000/health

# Should return: {"status":"ok"}
```

## Seed Nodes

Connect to these bootstrap nodes:

- `34.173.167.150:9091` (Primary seed node)

## Monitoring Your Node

### Check Logs

**Linux/Mac:**
```bash
tail -f ~/ouro_node.log
```

**Windows:**
```powershell
Get-Content $env:USERPROFILE\ouro_node.log -Tail 50 -Wait
```

### Check if Running

**Linux/Mac:**
```bash
ps aux | grep ouro_dag
```

**Windows:**
```powershell
Get-Process ouro_dag
```

### Check Connectivity

```bash
# Check API
curl http://localhost:8000/health

# Check if ports are listening
netstat -tulpn | grep -E '8001|9001|9091'
```

## Firewall Configuration

If you want other nodes to connect to you (recommended), open these ports:

- **9001** - P2P networking
- **9091** - BFT consensus
- **8001** - API (optional, only if hosting public API)

**Linux (ufw):**
```bash
sudo ufw allow 9001/tcp
sudo ufw allow 9091/tcp
```

**Linux (iptables):**
```bash
sudo iptables -A INPUT -p tcp --dport 9001 -j ACCEPT
sudo iptables -A INPUT -p tcp --dport 9091 -j ACCEPT
```

**Windows Firewall:**
```powershell
New-NetFirewallRule -DisplayName "Ouroboros P2P" -Direction Inbound -LocalPort 9001 -Protocol TCP -Action Allow
New-NetFirewallRule -DisplayName "Ouroboros BFT" -Direction Inbound -LocalPort 9091 -Protocol TCP -Action Allow
```

## Troubleshooting

### Node won't start

1. Check PostgreSQL is running:
   ```bash
   sudo systemctl status postgresql
   ```

2. Check database connection:
   ```bash
   psql postgres://ouro:ouro_pass@localhost:5432/ouro_db -c "SELECT 1"
   ```

3. Check logs for errors:
   ```bash
   tail -50 ~/ouro_node.log
   ```

### Can't connect to seed node

1. Verify internet connectivity:
   ```bash
   ping 34.173.167.150
   ```

2. Check if seed node port is reachable:
   ```bash
   nc -zv 34.173.167.150 9091
   ```

3. Check firewall isn't blocking outbound connections

### High CPU usage

This is normal during initial sync. The node will stabilize after syncing completes.

### Out of disk space

The blockchain grows over time. Ensure you have at least 20GB free space initially.

## Getting Help

- **GitHub Issues:** https://github.com/ouroboros-network/ouroboros/issues
- **Check seed node status:** `curl http://34.173.167.150:8000/health`

## Contributing

Once your node is running, you're part of the network! You can:

- Run a validator node
- Help relay transactions
- Contribute to the codebase
- Report bugs and suggest improvements

## Network Information

- **Blockchain:** Ouroboros
- **Consensus:** BFT (Byzantine Fault Tolerant) using HotStuff
- **Cryptography:** Post-quantum ready (Dilithium, Kyber)
- **Target TPS:** 20,000 - 50,000 transactions/second
- **Block Time:** ~2 seconds

## Security Notes

1. **Keep your BFT_SECRET_SEED private** - It's like your node's private key
2. **Use strong passwords** for PostgreSQL
3. **Keep your system updated** with security patches
4. **Monitor your node** regularly
5. **Backup your data** in `ROCKSDB_PATH` directory

---

**Welcome to the Ouroboros Network!**

Your participation helps make this network more decentralized and resilient.
