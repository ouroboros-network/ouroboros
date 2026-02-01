#!/bin/bash
# Ouroboros Oracle Testnet Deployment Script
# v0.3.0 - January 2026

set -e

echo "==================================="
echo "Ouroboros Oracle Testnet Deployment"
echo "==================================="

# Configuration
TESTNET_CONFIG="config/testnet_oracle.json"
DATA_DIR="./testnet_data"
LOG_DIR="./testnet_logs"

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

print_status() {
    echo -e "${GREEN}[+]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[!]${NC} $1"
}

print_error() {
    echo -e "${RED}[x]${NC} $1"
}

# Check prerequisites
check_prerequisites() {
    print_status "Checking prerequisites..."

    # Check Rust
    if ! command -v cargo &> /dev/null; then
        print_error "Rust/Cargo not found. Install from https://rustup.rs/"
        exit 1
    fi

    # Check RocksDB dependencies
    if [[ "$OSTYPE" == "linux-gnu"* ]]; then
        if ! dpkg -l | grep -q librocksdb-dev; then
            print_warning "librocksdb-dev not found. Installing..."
            sudo apt-get update && sudo apt-get install -y librocksdb-dev libclang-dev
        fi
    fi

    print_status "Prerequisites OK"
}

# Build the oracle node
build_oracle() {
    print_status "Building oracle node in release mode..."
    cargo build --release
    print_status "Build complete"
}

# Initialize data directories
init_directories() {
    print_status "Initializing data directories..."

    mkdir -p "$DATA_DIR"
    mkdir -p "$LOG_DIR"
    mkdir -p "$DATA_DIR/rocksdb"
    mkdir -p "$DATA_DIR/keys"

    print_status "Directories created"
}

# Generate testnet keys
generate_keys() {
    print_status "Generating testnet keys..."

    # Generate node keypair
    if [ ! -f "$DATA_DIR/keys/node_keypair" ]; then
        openssl rand -hex 32 > "$DATA_DIR/keys/bft_secret_seed"
        print_status "Generated BFT secret seed"
    else
        print_warning "Keys already exist, skipping generation"
    fi
}

# Create testnet environment file
create_env_file() {
    print_status "Creating testnet .env file..."

    cat > .env.testnet << EOF
# Ouroboros Testnet Configuration
# Generated: $(date)

# Network
CHAIN_ID=ouroboros-testnet-1
NODE_ID=oracle-testnet-$(openssl rand -hex 4)

# Storage
ROCKSDB_PATH=$DATA_DIR/rocksdb
STORAGE_MODE=rocks

# Network Ports
API_ADDR=0.0.0.0:8001
LISTEN_ADDR=0.0.0.0:9001
BFT_PORT=9091
BFT_PEERS=

# Oracle Configuration
ORACLE_API_PORT=8081
ORACLE_UPDATE_INTERVAL_MS=5000
ORACLE_MIN_SOURCES=3

# Security (relaxed for testnet)
RATE_LIMIT_ENABLED=true
RATE_LIMIT_MAX_REQUESTS=100
RATE_LIMIT_WINDOW_SECS=60

# Logging
RUST_LOG=info,ouro_dag=debug

# Testnet specific
TEST_MODE=true
INSECURE_MODE=true

# Free API Keys (for testing)
NASA_API_KEY=DEMO_KEY
EOF

    print_status "Testnet .env file created"
}

# Initialize genesis state
init_genesis() {
    print_status "Initializing genesis state..."

    # Run genesis initialization
    cargo run --release --bin ouro_node -- init-genesis \
        --chain-id ouroboros-testnet-1 \
        --config "$TESTNET_CONFIG" \
        || print_warning "Genesis init not available via CLI, will initialize on first run"

    print_status "Genesis state ready"
}

# Start the oracle node
start_oracle() {
    print_status "Starting oracle node..."

    # Load environment
    export $(cat .env.testnet | grep -v '^#' | xargs)

    # Start in background
    nohup cargo run --release > "$LOG_DIR/oracle_$(date +%Y%m%d_%H%M%S).log" 2>&1 &

    echo $! > "$DATA_DIR/oracle.pid"
    print_status "Oracle node started with PID $(cat $DATA_DIR/oracle.pid)"
}

# Health check
health_check() {
    print_status "Performing health check..."

    sleep 5  # Wait for node to start

    if curl -s http://localhost:8001/health > /dev/null; then
        print_status "Health check PASSED"
        curl -s http://localhost:8001/health | jq .
    else
        print_error "Health check FAILED - node may still be starting"
    fi
}

# Display connection info
show_info() {
    echo ""
    echo "==================================="
    echo "Testnet Oracle Deployment Complete"
    echo "==================================="
    echo ""
    echo "Endpoints:"
    echo "  - Node API:    http://localhost:8001"
    echo "  - Oracle API:  http://localhost:8081"
    echo "  - P2P Port:    9001"
    echo "  - BFT Port:    9091"
    echo ""
    echo "Available Oracle Feeds:"
    echo "  - Crypto: BTC/USD, ETH/USD, OURO/USD (via CoinGecko, Binance, Coinbase)"
    echo "  - Weather: Open-Meteo (free, no API key)"
    echo "  - Stocks: Yahoo Finance (AAPL, GOOGL, MSFT, TSLA)"
    echo "  - News: Hacker News, Reddit"
    echo "  - Random: Random.org (true randomness)"
    echo "  - NASA: APOD (DEMO_KEY)"
    echo ""
    echo "Test Commands:"
    echo "  curl http://localhost:8001/health"
    echo "  curl http://localhost:8001/oracle/feeds"
    echo "  curl http://localhost:8001/oracle/price/BTC"
    echo ""
    echo "Logs: $LOG_DIR/"
    echo "Data: $DATA_DIR/"
    echo ""
}

# Main deployment flow
main() {
    case "${1:-deploy}" in
        "check")
            check_prerequisites
            ;;
        "build")
            check_prerequisites
            build_oracle
            ;;
        "init")
            init_directories
            generate_keys
            create_env_file
            ;;
        "start")
            start_oracle
            health_check
            ;;
        "deploy")
            check_prerequisites
            build_oracle
            init_directories
            generate_keys
            create_env_file
            start_oracle
            health_check
            show_info
            ;;
        "info")
            show_info
            ;;
        "stop")
            if [ -f "$DATA_DIR/oracle.pid" ]; then
                kill $(cat "$DATA_DIR/oracle.pid") 2>/dev/null || true
                rm "$DATA_DIR/oracle.pid"
                print_status "Oracle node stopped"
            else
                print_warning "No running oracle found"
            fi
            ;;
        *)
            echo "Usage: $0 {check|build|init|start|deploy|info|stop}"
            exit 1
            ;;
    esac
}

main "$@"
