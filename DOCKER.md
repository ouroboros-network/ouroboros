# Running Ouroboros Node with Docker

Run an Ouroboros node in a container - no Rust toolchain required.

## Quick Start

### Option 1: Docker Compose (Recommended)

```bash
# Clone the repository
git clone https://github.com/ouroboros-network/ouroboros.git
cd ouroboros

# Start the node
docker-compose up -d

# Check logs
docker-compose logs -f

# Check status
curl http://localhost:8000/health
```

### Option 2: Docker Run

```bash
# Build the image
docker build -t ouroboros-node .

# Run the container
docker run -d \
  --name ouroboros-node \
  -p 8000:8000 \
  -p 9000:9000 \
  -v ouroboros-data:/data \
  -e PEER_ADDRS=136.112.101.176:9000,34.57.121.217:9000 \
  -e RUST_LOG=info \
  ouroboros-node
```

### Option 3: Pre-built Image

```bash
# Pull the latest image
docker pull ghcr.io/ouroboros-network/ouroboros:latest

# Run the node
docker run -d \
  --name ouroboros-node \
  -p 8000:8000 \
  -p 9000:9000 \
  -v ouroboros-data:/data \
  -e PEER_ADDRS=136.112.101.176:9000,34.57.121.217:9000 \
  ghcr.io/ouroboros-network/ouroboros:latest
```

Available tags:
- `latest` - Latest stable release
- `main` - Latest from main branch
- `v0.4.1` - Specific version

## Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `ROCKSDB_PATH` | `/data` | Database storage path |
| `API_ADDR` | `0.0.0.0:8000` | API listen address |
| `LISTEN_ADDR` | `0.0.0.0:9000` | P2P listen address |
| `PEER_ADDRS` | (seed nodes) | Comma-separated peer addresses |
| `RUST_LOG` | `info` | Log level (debug, info, warn, error) |
| `STORAGE_MODE` | `full` | Storage mode (always use `full`) |
| `NODE_WALLET_ADDRESS` | (none) | Your wallet address for rewards |
| `API_KEYS` | (none) | API authentication key |
| `BFT_SECRET_SEED` | (auto) | 64-char hex seed for signing |

### Ports

| Port | Protocol | Description |
|------|----------|-------------|
| 8000 | TCP/HTTP | REST API |
| 9000 | TCP | P2P networking |

### Volumes

| Path | Description |
|------|-------------|
| `/data` | RocksDB database and node state |

## Production Deployment

### With Custom Configuration

Create a `.env` file:

```env
NODE_WALLET_ADDRESS=your_wallet_address_here
API_KEYS=your_secure_api_key_here
BFT_SECRET_SEED=your_64_character_hex_seed_here
RUST_LOG=info
```

Then run:

```bash
docker-compose --env-file .env up -d
```

### With TLS (HTTPS)

Mount your certificates:

```yaml
# In docker-compose.yml, add to volumes:
volumes:
  - ouroboros-data:/data
  - ./certs/cert.pem:/certs/cert.pem:ro
  - ./certs/key.pem:/certs/key.pem:ro

# Add environment variables:
environment:
  - TLS_CERT_PATH=/certs/cert.pem
  - TLS_KEY_PATH=/certs/key.pem
```

### Resource Limits

The default `docker-compose.yml` includes resource limits:
- CPU: 2 cores max, 0.5 cores reserved
- Memory: 2GB max, 512MB reserved

Adjust in the `deploy.resources` section as needed.

## Management Commands

```bash
# View logs
docker-compose logs -f ouroboros-node

# Stop node
docker-compose down

# Restart node
docker-compose restart

# Check node health
curl http://localhost:8000/health

# View node status (if ouro CLI installed)
ouro status

# Enter container shell
docker exec -it ouroboros-node /bin/bash

# Backup data
docker run --rm -v ouroboros-data:/data -v $(pwd):/backup \
  alpine tar czf /backup/ouroboros-backup.tar.gz -C /data .

# Restore data
docker run --rm -v ouroboros-data:/data -v $(pwd):/backup \
  alpine tar xzf /backup/ouroboros-backup.tar.gz -C /data
```

## Monitoring

### Health Check

The container includes a built-in health check:

```bash
docker inspect --format='{{.State.Health.Status}}' ouroboros-node
```

### Prometheus Metrics

Metrics are available at `http://localhost:8000/metrics` (Prometheus format) and `http://localhost:8000/metrics/json` (JSON format).

### Example Prometheus Config

```yaml
scrape_configs:
  - job_name: 'ouroboros'
    static_configs:
      - targets: ['ouroboros-node:8000']
    metrics_path: /metrics
```

## Troubleshooting

### Node won't start

```bash
# Check logs
docker-compose logs ouroboros-node

# Common issues:
# - Port already in use: Change ports in docker-compose.yml
# - Permission denied: Check volume permissions
# - Database locked: Stop any other instances
```

### Can't connect to peers

```bash
# Check if P2P port is accessible
nc -zv localhost 9000

# Check firewall rules
sudo ufw allow 9000/tcp
```

### High memory usage

Adjust resource limits in `docker-compose.yml`:

```yaml
deploy:
  resources:
    limits:
      memory: 1G
```

## Building for Different Architectures

```bash
# Build for linux/amd64
docker build --platform linux/amd64 -t ouroboros-node:amd64 .

# Build for linux/arm64
docker build --platform linux/arm64 -t ouroboros-node:arm64 .

# Multi-arch build (requires buildx)
docker buildx build --platform linux/amd64,linux/arm64 -t ouroboros-node:latest .
```

## Security Notes

- The container runs as non-root user `ouroboros` (UID 1000)
- Sensitive environment variables should be passed via `.env` file or secrets
- Keep your `BFT_SECRET_SEED` and `API_KEYS` private
- Use TLS in production environments
- Regularly update to the latest image

## Support

- Issues: [GitHub Issues](https://github.com/ouroboros-network/ouroboros/issues)
- Documentation: [README.md](README.md)
- API Docs: [API_DOCUMENTATION.md](API_DOCUMENTATION.md)
