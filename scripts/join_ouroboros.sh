#!/bin/bash
# Ouroboros Network - Node Setup (Linux/macOS)
# Supports x86_64, ARM64 (Apple Silicon M1/M2/M3)

set -e

NODE_DIR="$HOME/.ouroboros"
DATA_DIR="$NODE_DIR/data"

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
            x86_64|amd64)    BINARY_NAME="ouro_dag-linux-x64" ;;
            aarch64|arm64)   BINARY_NAME="ouro_dag-linux-arm64" ;;
            *)
                echo "Unsupported architecture: $ARCH"
                echo "Supported: x86_64, aarch64 (ARM64)"
                echo ""
                echo "Manual install for x86_64:"
                echo "  mkdir -p ~/.ouroboros/data"
                echo "  curl -L https://github.com/ouroboros-network/ouroboros/releases/download/v0.4.1/ouro_dag-linux-x64 -o ~/.ouroboros/ouro-bin"
                echo "  chmod +x ~/.ouroboros/ouro-bin"
                echo "  ~/.ouroboros/ouro-bin start"
                echo ""
                echo "Manual install for ARM64:"
                echo "  mkdir -p ~/.ouroboros/data"
                echo "  curl -L https://github.com/ouroboros-network/ouroboros/releases/download/v0.4.1/ouro_dag-linux-arm64 -o ~/.ouroboros/ouro-bin"
                echo "  chmod +x ~/.ouroboros/ouro-bin"
                echo "  ~/.ouroboros/ouro-bin start"
                exit 1
                ;;
        esac
        ;;
    Darwin)
        case "$ARCH" in
            x86_64|amd64)          BINARY_NAME="ouro_dag-macos-x64" ;;
            arm64|arm64e|aarch64)  BINARY_NAME="ouro_dag-macos-arm64" ;;
            *)
                echo "Unsupported architecture: $ARCH"
                echo "Supported: x86_64, arm64 (Apple Silicon M1/M2/M3)"
                echo ""
                echo "Manual install for Apple Silicon:"
                echo "  mkdir -p ~/.ouroboros/data"
                echo "  curl -L https://github.com/ouroboros-network/ouroboros/releases/download/v0.4.1/ouro_dag-macos-arm64 -o ~/.ouroboros/ouro-bin"
                echo "  chmod +x ~/.ouroboros/ouro-bin"
                echo "  ~/.ouroboros/ouro-bin start"
                echo ""
                echo "Manual install for Intel Mac:"
                echo "  mkdir -p ~/.ouroboros/data"
                echo "  curl -L https://github.com/ouroboros-network/ouroboros/releases/download/v0.4.1/ouro_dag-macos-x64 -o ~/.ouroboros/ouro-bin"
                echo "  chmod +x ~/.ouroboros/ouro-bin"
                echo "  ~/.ouroboros/ouro-bin start"
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

echo "Detected: $OS $ARCH"
echo ""

# Create directories
mkdir -p "$NODE_DIR" "$DATA_DIR"

# Step 1: Download binary
echo "[1/4] Downloading Ouroboros node..."
DOWNLOAD_URL="https://github.com/ouroboros-network/ouroboros/releases/download/v0.4.1/$BINARY_NAME"

download_success=false

# Try curl first
if curl -fsSL "$DOWNLOAD_URL" -o "$NODE_DIR/ouro-bin" 2>/dev/null; then
    if [ -s "$NODE_DIR/ouro-bin" ] && [ $(stat -f%z "$NODE_DIR/ouro-bin" 2>/dev/null || stat -c%s "$NODE_DIR/ouro-bin" 2>/dev/null) -gt 1000000 ]; then
        chmod +x "$NODE_DIR/ouro-bin"
        echo "      Download successful"
        download_success=true
    fi
fi

# Try wget as fallback
if [ "$download_success" = false ] && command -v wget &> /dev/null; then
    echo "      Trying wget..."
    if wget -q "$DOWNLOAD_URL" -O "$NODE_DIR/ouro-bin" 2>/dev/null; then
        if [ -s "$NODE_DIR/ouro-bin" ] && [ $(stat -f%z "$NODE_DIR/ouro-bin" 2>/dev/null || stat -c%s "$NODE_DIR/ouro-bin" 2>/dev/null) -gt 1000000 ]; then
            chmod +x "$NODE_DIR/ouro-bin"
            echo "      Download successful"
            download_success=true
        fi
    fi
fi

if [ "$download_success" = false ]; then
    echo ""
    echo "ERROR: Download failed."
    echo ""
    echo "Please download manually from:"
    echo "  $DOWNLOAD_URL"
    echo ""
    echo "Save it to: $NODE_DIR/ouro-bin"
    echo "Then run: chmod +x $NODE_DIR/ouro-bin && $NODE_DIR/ouro-bin join"
    exit 1
fi

echo ""

# Step 2: Stop existing node if running
echo "[2/4] Checking for existing node..."
if pgrep -f "ouro-bin" > /dev/null 2>&1; then
    echo "      Stopping existing node..."
    pkill -f "ouro-bin" 2>/dev/null || true
    sleep 2
fi

# Remove stale lock file if exists
if [ -f "$DATA_DIR/LOCK" ]; then
    rm -f "$DATA_DIR/LOCK" 2>/dev/null || true
fi

# Step 3: Configure node
echo "[3/4] Configuring node..."

# Use consistent ports: API=8000, P2P=9000
SEED_NODE="${OUROBOROS_SEED:-136.112.101.176:9000}"

# Check if existing config has required keys
NEEDS_NEW_CONFIG=true
if [ -f "$NODE_DIR/.env" ]; then
    if grep -q "API_KEYS=" "$NODE_DIR/.env" && grep -q "BFT_SECRET_SEED=" "$NODE_DIR/.env"; then
        echo "      Using existing configuration"
        NEEDS_NEW_CONFIG=false
    else
        echo "      Upgrading configuration (adding required keys)..."
    fi
fi

if [ "$NEEDS_NEW_CONFIG" = true ]; then
    # Generate secrets
    BFT_SECRET_SEED=$(openssl rand -hex 32 2>/dev/null || head -c 64 /dev/urandom | xxd -p | tr -d '\n' | head -c 64)
    NODE_ID="node-$(openssl rand -hex 4 2>/dev/null || head -c 8 /dev/urandom | xxd -p | tr -d '\n')"
    API_KEY=$(openssl rand -hex 16 2>/dev/null || head -c 32 /dev/urandom | xxd -p | tr -d '\n')

    echo "      Generated new node identity: $NODE_ID"

    # Save to .env file - USE CONSISTENT PORTS: API=8000, P2P=9000
    cat > "$NODE_DIR/.env" <<EOF
# Ouroboros Node Configuration
DATABASE_PATH=$DATA_DIR
API_ADDR=0.0.0.0:8000
LISTEN_ADDR=0.0.0.0:9000
PEER_ADDRS=$SEED_NODE
NODE_ID=$NODE_ID
BFT_SECRET_SEED=$BFT_SECRET_SEED
API_KEYS=$API_KEY
RUST_LOG=info
EOF
fi

echo "      API Port: 8000"
echo "      P2P Port: 9000"
echo "      Seed node: $SEED_NODE"

# Step 4: Setup scripts and services
echo "[4/4] Setting up scripts..."

# Create wrapper script that loads environment (main 'ouro' command)
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

# Create start script
cat > "$NODE_DIR/start.sh" <<EOF
#!/bin/bash
set -a
source $NODE_DIR/.env
set +a
exec $NODE_DIR/ouro-bin join
EOF
chmod +x "$NODE_DIR/start.sh"

# Create stop script
cat > "$NODE_DIR/stop.sh" <<'EOF'
#!/bin/bash
echo "Stopping Ouroboros node..."
if pkill -f "ouro-bin"; then
    echo "Node stopped successfully."
else
    echo "No running node found."
fi
EOF
chmod +x "$NODE_DIR/stop.sh"

# Create symlinks
if [ -d "/usr/local/bin" ] && [ -w "/usr/local/bin" ]; then
    ln -sf "$NODE_DIR/ouro" "/usr/local/bin/ouro" 2>/dev/null || true
elif command -v sudo &> /dev/null; then
    sudo ln -sf "$NODE_DIR/ouro" "/usr/local/bin/ouro" 2>/dev/null || true
fi
mkdir -p "$HOME/.local/bin"
ln -sf "$NODE_DIR/ouro" "$HOME/.local/bin/ouro" 2>/dev/null || true

# Add to shell profile if not already there
for profile in "$HOME/.bashrc" "$HOME/.zshrc" "$HOME/.profile"; do
    if [ -f "$profile" ] && ! grep -q "\.ouroboros" "$profile" 2>/dev/null; then
        echo 'export PATH="$HOME/.ouroboros:$HOME/.local/bin:$PATH"' >> "$profile"
    fi
done
export PATH="$NODE_DIR:$HOME/.local/bin:$PATH"

# Setup auto-start (platform-specific)
if [ "$OS" = "Linux" ] && command -v systemctl &> /dev/null; then
    # Linux with systemd
    sudo tee /etc/systemd/system/ouroboros.service > /dev/null <<EOF
[Unit]
Description=Ouroboros Node
After=network.target

[Service]
Type=simple
User=$USER
WorkingDirectory=$NODE_DIR
EnvironmentFile=$NODE_DIR/.env
ExecStart=$NODE_DIR/ouro-bin join
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
EOF

    sudo systemctl daemon-reload
    sudo systemctl enable ouroboros.service 2>/dev/null || true
    echo "      Systemd service configured"

elif [ "$OS" = "Darwin" ]; then
    # macOS with launchd
    mkdir -p ~/Library/LaunchAgents
    cat > ~/Library/LaunchAgents/network.ouroboros.node.plist <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>network.ouroboros.node</string>
    <key>ProgramArguments</key>
    <array>
        <string>$NODE_DIR/start.sh</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>$NODE_DIR/node.log</string>
    <key>StandardErrorPath</key>
    <string>$NODE_DIR/node_error.log</string>
</dict>
</plist>
EOF
    echo "      LaunchAgent configured"
fi

echo ""
echo "Starting Ouroboros node..."

# Start node
if [ "$OS" = "Linux" ] && command -v systemctl &> /dev/null; then
    sudo systemctl start ouroboros 2>/dev/null || true
    sleep 3
elif [ "$OS" = "Darwin" ]; then
    launchctl unload ~/Library/LaunchAgents/network.ouroboros.node.plist 2>/dev/null || true
    launchctl load ~/Library/LaunchAgents/network.ouroboros.node.plist 2>/dev/null || true
    sleep 3
else
    # Manual start in background
    nohup "$NODE_DIR/start.sh" > "$NODE_DIR/node.log" 2>&1 &
    sleep 3
fi

# Check status
echo ""
if curl -sf http://localhost:8000/health >/dev/null 2>&1; then
    echo "=========================================="
    echo "  Node started successfully!"
    echo "=========================================="
    echo ""
    echo "  Seed node: $SEED_NODE"
    echo "  API: http://localhost:8000"
    echo "  Data: $DATA_DIR"
    echo ""
    echo "Commands:"
    echo "  ouro status     - View node dashboard"
    echo "  ouro peers      - List connected peers"
    echo "  ouro diagnose   - Run diagnostics"
    echo ""
    echo "Management:"
    if [ "$OS" = "Linux" ] && command -v systemctl &> /dev/null; then
        echo "  sudo systemctl status ouroboros  - Check status"
        echo "  sudo systemctl stop ouroboros    - Stop node"
        echo "  sudo systemctl restart ouroboros - Restart node"
    elif [ "$OS" = "Darwin" ]; then
        echo "  $NODE_DIR/stop.sh   - Stop node"
        echo "  $NODE_DIR/start.sh  - Start node"
    else
        echo "  $NODE_DIR/stop.sh   - Stop node"
        echo "  $NODE_DIR/start.sh  - Start node"
    fi
    echo ""
    echo "Logs:"
    echo "  tail -f $NODE_DIR/node.log"
    echo ""
    echo "You're now part of the Ouroboros network!"
    echo "=========================================="
else
    echo "Warning: Node may still be starting..."
    echo ""
    echo "Check status: ouro status"
    echo "Check logs: tail -f $NODE_DIR/node.log"
    echo ""
    if [ -f "$NODE_DIR/node.log" ]; then
        echo "=== Recent Log ==="
        tail -20 "$NODE_DIR/node.log" 2>/dev/null || true
    fi
fi
echo ""
