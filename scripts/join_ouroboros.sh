#!/bin/bash
# Ouroboros Network - Node Setup (Linux/macOS)
# Supports x86_64, ARM64 (Apple Silicon M1/M2/M3)

set -e

NODE_DIR="$HOME/.ouroboros"
DATA_DIR="$NODE_DIR/data"
REPO="ouroboros-network/ouroboros"
SEEDS="136.112.101.176:9000,34.57.121.217:9000"

echo ""
echo "=========================================="
echo "  Ouroboros Network - Quick Join"
echo "=========================================="
echo ""

# Detect OS and architecture
OS=$(uname -s)
ARCH=$(uname -m)

case "$OS" in
    Linux)
        case "$ARCH" in
            x86_64|amd64)    BINARY_NAME="ouro-linux-x64" ;;
            aarch64|arm64)   BINARY_NAME="ouro-linux-arm64" ;;
            *)
                echo "Unsupported architecture: $ARCH"
                echo "Supported: x86_64, aarch64 (ARM64)"
                exit 1
                ;;
        esac
        ;;
    Darwin)
        case "$ARCH" in
            x86_64|amd64)          BINARY_NAME="ouro-macos-x64" ;;
            arm64|arm64e|aarch64)  BINARY_NAME="ouro-macos-arm64" ;;
            *)
                echo "Unsupported architecture: $ARCH"
                echo "Supported: x86_64, arm64 (Apple Silicon)"
                exit 1
                ;;
        esac
        ;;
    *)
        echo "Unsupported OS: $OS"
        echo "Supported: Linux, macOS"
        exit 1
        ;;
esac

echo "[1/5] Detected: $OS $ARCH ($BINARY_NAME)"

# Check for existing installation
EXISTING_VERSION=""
if [ -f "$NODE_DIR/ouro-bin" ]; then
    EXISTING_VERSION=$("$NODE_DIR/ouro-bin" --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' || echo "")
    if [ -n "$EXISTING_VERSION" ]; then
        echo "      Found existing: v$EXISTING_VERSION"
    fi
fi

# Get latest release version
LATEST_VERSION=""
LATEST_URL=""
if command -v curl >/dev/null 2>&1; then
    LATEST_TAG=$(curl -sI "https://github.com/$REPO/releases/latest" 2>/dev/null | grep -i "^location:" | grep -oE 'v[0-9]+\.[0-9]+\.[0-9]+' || echo "")
    if [ -n "$LATEST_TAG" ]; then
        LATEST_VERSION="${LATEST_TAG#v}"
        echo "      Latest release: v$LATEST_VERSION"
    fi
fi

NEEDS_DOWNLOAD=true
if [ -n "$EXISTING_VERSION" ] && [ "$EXISTING_VERSION" = "$LATEST_VERSION" ]; then
    echo "      Already up to date!"
    NEEDS_DOWNLOAD=false
elif [ -n "$EXISTING_VERSION" ] && [ -n "$LATEST_VERSION" ]; then
    echo "      Upgrading v$EXISTING_VERSION -> v$LATEST_VERSION"
fi

# Create directories
mkdir -p "$NODE_DIR" "$DATA_DIR"

# Step 2: Stop existing node if running
echo "[2/5] Checking for existing node..."
if pgrep -f "ouro-bin" > /dev/null 2>&1; then
    echo "      Stopping existing node..."
    # Try graceful shutdown first
    if [ -f "$NODE_DIR/.env" ]; then
        API_KEY=$(grep "^API_KEYS=" "$NODE_DIR/.env" 2>/dev/null | cut -d= -f2 | cut -d, -f1)
        if [ -n "$API_KEY" ]; then
            curl -sf -X POST -H "Authorization: Bearer $API_KEY" http://localhost:8000/shutdown 2>/dev/null || true
            sleep 3
        fi
    fi
    # Force kill if still running
    if pgrep -f "ouro-bin" > /dev/null 2>&1; then
        pkill -f "ouro-bin" 2>/dev/null || true
        sleep 2
    fi
    echo "      Stopped."
else
    echo "      No running node found."
fi

# Remove stale lock file
[ -f "$DATA_DIR/LOCK" ] && rm -f "$DATA_DIR/LOCK" 2>/dev/null || true

# Step 3: Download binary
echo "[3/5] Downloading Ouroboros node..."

if [ "$NEEDS_DOWNLOAD" = true ]; then
    DOWNLOAD_URL="https://github.com/$REPO/releases/latest/download/$BINARY_NAME"

    download_success=false

    if curl -fsSL "$DOWNLOAD_URL" -o "$NODE_DIR/ouro-bin" 2>/dev/null; then
        FILE_SIZE=$(stat -f%z "$NODE_DIR/ouro-bin" 2>/dev/null || stat -c%s "$NODE_DIR/ouro-bin" 2>/dev/null || echo 0)
        if [ "$FILE_SIZE" -gt 1000000 ]; then
            chmod +x "$NODE_DIR/ouro-bin"
            SIZE_MB=$(echo "scale=1; $FILE_SIZE / 1048576" | bc 2>/dev/null || echo "?")
            echo "      Downloaded successfully (${SIZE_MB} MB)"
            download_success=true
        fi
    fi

    if [ "$download_success" = false ] && command -v wget &> /dev/null; then
        echo "      Trying wget..."
        if wget -q "$DOWNLOAD_URL" -O "$NODE_DIR/ouro-bin" 2>/dev/null; then
            chmod +x "$NODE_DIR/ouro-bin"
            echo "      Download successful (wget)"
            download_success=true
        fi
    fi

    if [ "$download_success" = false ]; then
        echo ""
        echo "ERROR: Download failed."
        echo "Download manually: https://github.com/$REPO/releases/latest"
        echo "Save as: $NODE_DIR/ouro-bin"
        exit 1
    fi

    # Download Python tier files
    echo "      Downloading Python tier files..."
    RAW_BASE="https://raw.githubusercontent.com/$REPO/main"
    PY_DIR="$NODE_DIR/ouro_py"
    mkdir -p "$PY_DIR/ouro_medium" "$PY_DIR/ouro_light"
    curl -sL -o "$PY_DIR/requirements.txt" "$RAW_BASE/ouro_py/requirements.txt" 2>/dev/null || true
    curl -sL -o "$PY_DIR/ouro_medium/main.py" "$RAW_BASE/ouro_py/ouro_medium/main.py" 2>/dev/null || true
    curl -sL -o "$PY_DIR/ouro_light/main.py" "$RAW_BASE/ouro_py/ouro_light/main.py" 2>/dev/null || true
    echo "      Python tier files installed."
else
    echo "      Skipping download (already up to date)."
fi

echo ""

# Step 4: Configure node
echo "[4/5] Configuring node..."

SEED_NODE="${OUROBOROS_SEED:-$SEEDS}"

NEEDS_NEW_CONFIG=true
if [ -f "$NODE_DIR/.env" ]; then
    if grep -q "API_KEYS=" "$NODE_DIR/.env" && grep -q "BFT_SECRET_SEED=" "$NODE_DIR/.env"; then
        echo "      Using existing configuration"
        NEEDS_NEW_CONFIG=false
    else
        echo "      Upgrading configuration..."
    fi
fi

if [ "$NEEDS_NEW_CONFIG" = true ]; then
    BFT_SECRET_SEED=$(openssl rand -hex 32 2>/dev/null || head -c 64 /dev/urandom | xxd -p | tr -d '\n' | head -c 64)
    NODE_ID="ouro_$(openssl rand -hex 8 2>/dev/null || head -c 16 /dev/urandom | xxd -p | tr -d '\n')"
    API_KEY="ouro_$(openssl rand -hex 16 2>/dev/null || head -c 32 /dev/urandom | xxd -p | tr -d '\n')"

    echo "      Generated node identity: $NODE_ID"

    cat > "$NODE_DIR/.env" <<EOF
# Ouroboros Node Configuration
ROCKSDB_PATH=$DATA_DIR
API_ADDR=0.0.0.0:8000
LISTEN_ADDR=0.0.0.0:9000
PEER_ADDRS=$SEED_NODE
NODE_ID=$NODE_ID
BFT_SECRET_SEED=$BFT_SECRET_SEED
API_KEYS=$API_KEY
RUST_LOG=info
STORAGE_MODE=rocksdb
EOF
fi

echo "      API: http://localhost:8000"
echo "      P2P: 0.0.0.0:9000"

# Step 5: Setup scripts and start
echo "[5/5] Starting node..."

# Create wrapper script
cat > "$NODE_DIR/ouro" <<'WRAPPER'
#!/bin/bash
OURO_DIR="$HOME/.ouroboros"
if [ -f "$OURO_DIR/.env" ]; then
    set -a
    source "$OURO_DIR/.env"
    set +a
fi
exec "$OURO_DIR/ouro-bin" "$@"
WRAPPER
chmod +x "$NODE_DIR/ouro"

# Create start/stop scripts
cat > "$NODE_DIR/start.sh" <<EOF
#!/bin/bash
set -a
source $NODE_DIR/.env
set +a
exec $NODE_DIR/ouro-bin start
EOF
chmod +x "$NODE_DIR/start.sh"

cat > "$NODE_DIR/stop.sh" <<'EOF'
#!/bin/bash
echo "Stopping Ouroboros node..."
if pkill -f "ouro-bin"; then
    echo "Node stopped."
else
    echo "No running node found."
fi
EOF
chmod +x "$NODE_DIR/stop.sh"

# Add to PATH
if [ -d "/usr/local/bin" ] && [ -w "/usr/local/bin" ]; then
    ln -sf "$NODE_DIR/ouro" "/usr/local/bin/ouro" 2>/dev/null || true
elif command -v sudo &> /dev/null; then
    sudo ln -sf "$NODE_DIR/ouro" "/usr/local/bin/ouro" 2>/dev/null || true
fi
mkdir -p "$HOME/.local/bin"
ln -sf "$NODE_DIR/ouro" "$HOME/.local/bin/ouro" 2>/dev/null || true

for profile in "$HOME/.bashrc" "$HOME/.zshrc" "$HOME/.profile"; do
    if [ -f "$profile" ] && ! grep -q "\.ouroboros" "$profile" 2>/dev/null; then
        echo 'export PATH="$HOME/.ouroboros:$HOME/.local/bin:$PATH"' >> "$profile"
    fi
done
export PATH="$NODE_DIR:$HOME/.local/bin:$PATH"

# Start node
nohup "$NODE_DIR/start.sh" > "$NODE_DIR/node.log" 2>&1 &
NODE_PID=$!
echo "      Node started (PID: $NODE_PID)"

sleep 5

# Check status
echo ""
if curl -sf http://localhost:8000/health >/dev/null 2>&1; then
    VERSION=$("$NODE_DIR/ouro-bin" --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' || echo "?")
    echo "=========================================="
    echo "  Node started successfully! v$VERSION"
    echo "=========================================="
    echo ""
    echo "  API:  http://localhost:8000"
    echo "  Data: $DATA_DIR"
    echo ""
    echo "Commands:"
    echo "  ouro status     - View node dashboard"
    echo "  ouro peers      - List connected peers"
    echo "  ouro roles      - Show tier details"
    echo "  ouro diagnose   - Run diagnostics"
    echo ""
    echo "Start with a specific role:"
    echo "  ouro start --role heavy     # Full validator (default)"
    echo "  ouro start --role medium    # Subchain aggregator"
    echo "  ouro start --role light     # App node / watchdog"
    echo ""
    echo "Management:"
    echo "  $NODE_DIR/stop.sh   - Stop node"
    echo "  $NODE_DIR/start.sh  - Start node"
    echo ""
    echo "You're now part of the Ouroboros network!"
    echo "=========================================="
else
    echo "Warning: Node may still be starting..."
    echo ""
    echo "Check status: ouro status"
    echo "Check logs:   tail -f $NODE_DIR/node.log"
    echo ""
    if [ -f "$NODE_DIR/node.log" ]; then
        echo "=== Recent Log ==="
        tail -20 "$NODE_DIR/node.log" 2>/dev/null || true
    fi
fi
echo ""
