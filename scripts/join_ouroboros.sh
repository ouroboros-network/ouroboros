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
            x86_64)  BINARY_NAME="ouro_dag-linux-x64" ;;
            aarch64) BINARY_NAME="ouro_dag-linux-arm64" ;;
            *)
                echo "Unsupported architecture: $ARCH"
                echo "Supported: x86_64, aarch64 (ARM64)"
                exit 1
                ;;
        esac
        ;;
    Darwin)
        case "$ARCH" in
            x86_64)  BINARY_NAME="ouro_dag-macos-x64" ;;
            arm64)   BINARY_NAME="ouro_dag-macos-arm64" ;;
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

echo "Detected: $OS $ARCH"
echo ""

# Create directories
mkdir -p "$NODE_DIR" "$DATA_DIR"

# Download binary
echo "[1/4] Downloading Ouroboros node..."
DOWNLOAD_URL="https://github.com/ouroboros-network/ouroboros/releases/latest/download/$BINARY_NAME"

if curl -sL "$DOWNLOAD_URL" -o "$NODE_DIR/ouro" 2>/dev/null; then
    chmod +x "$NODE_DIR/ouro"
    echo "      Binary downloaded successfully"
else
    echo "      Download failed - building from source..."
    echo ""

    # Check for Rust
    if ! command -v cargo &> /dev/null; then
        echo "Rust not found. Installing via rustup..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        source "$HOME/.cargo/env"
    fi

    # Check for Git
    if ! command -v git &> /dev/null; then
        echo "Git not found. Please install git first."
        if [ "$OS" = "Darwin" ]; then
            echo "  brew install git"
        else
            echo "  sudo apt install git  (Debian/Ubuntu)"
            echo "  sudo dnf install git  (Fedora)"
        fi
        exit 1
    fi

    echo "Building from source (this may take 15-30 minutes)..."
    cd /tmp
    rm -rf ouroboros
    git clone https://github.com/ouroboros-network/ouroboros.git
    cd ouroboros/ouro_dag
    cargo build --release --bin ouro_dag
    cp target/release/ouro_dag "$NODE_DIR/ouro"
    chmod +x "$NODE_DIR/ouro"
    cd "$NODE_DIR"
fi

# Configure node
echo "[2/4] Configuring node..."
SEED_NODE="${OUROBOROS_SEED:-136.112.101.176:9001}"

# Generate BFT secret seed if not exists
if [ -f "$NODE_DIR/.env" ] && grep -q "BFT_SECRET_SEED" "$NODE_DIR/.env"; then
    echo "      Using existing configuration"
    source "$NODE_DIR/.env"
else
    BFT_SECRET_SEED=$(openssl rand -hex 32 2>/dev/null || head -c 64 /dev/urandom | xxd -p | tr -d '\n' | head -c 64)
    NODE_ID="node-$(openssl rand -hex 4 2>/dev/null || head -c 8 /dev/urandom | xxd -p | tr -d '\n')"
    echo "      Generated new node identity: $NODE_ID"
fi

cat > "$NODE_DIR/.env" <<EOF
DATABASE_PATH=$DATA_DIR
API_ADDRESS=0.0.0.0:8000
API_ADDR=0.0.0.0:8000
P2P_ADDRESS=0.0.0.0:9001
LISTEN_ADDR=0.0.0.0:9000
PEER_ADDRS=$SEED_NODE
BFT_SECRET_SEED=${BFT_SECRET_SEED}
NODE_ID=${NODE_ID:-node-$(openssl rand -hex 4 2>/dev/null || echo "default")}
RUST_LOG=info
EOF

echo "      Data directory: $DATA_DIR"
echo "      Seed node: $SEED_NODE"

# Setup auto-start (platform-specific)
echo "[3/4] Setting up auto-start..."

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
ExecStart=$NODE_DIR/ouro start
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
EOF

    sudo systemctl daemon-reload
    sudo systemctl enable ouroboros.service
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
        <string>$NODE_DIR/ouro</string>
        <string>join</string>
        <string>--peer</string>
        <string>$SEED_NODE</string>
        <string>--storage</string>
        <string>rocksdb</string>
        <string>--rocksdb-path</string>
        <string>$DATA_DIR</string>
    </array>
    <key>EnvironmentVariables</key>
    <dict>
        <key>DATABASE_PATH</key>
        <string>$DATA_DIR</string>
        <key>API_ADDRESS</key>
        <string>0.0.0.0:8001</string>
        <key>P2P_ADDRESS</key>
        <string>0.0.0.0:9001</string>
    </dict>
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
else
    echo "      Manual start required (no systemd/launchd)"
fi

# Add ouro to PATH
echo "[4/4] Setting up CLI..."
# Try multiple locations for PATH
if [ -d "/usr/local/bin" ] && [ -w "/usr/local/bin" ]; then
    ln -sf "$NODE_DIR/ouro" "/usr/local/bin/ouro" 2>/dev/null || true
elif command -v sudo &> /dev/null; then
    sudo ln -sf "$NODE_DIR/ouro" "/usr/local/bin/ouro" 2>/dev/null || true
fi
if [ -d "$HOME/.local/bin" ]; then
    mkdir -p "$HOME/.local/bin"
    ln -sf "$NODE_DIR/ouro" "$HOME/.local/bin/ouro" 2>/dev/null || true
fi
# Add to shell profile if not already there
if ! grep -q "\.ouroboros" "$HOME/.bashrc" 2>/dev/null; then
    echo 'export PATH="$HOME/.ouroboros:$PATH"' >> "$HOME/.bashrc"
fi
if ! grep -q "\.ouroboros" "$HOME/.zshrc" 2>/dev/null; then
    echo 'export PATH="$HOME/.ouroboros:$PATH"' >> "$HOME/.zshrc" 2>/dev/null || true
fi
export PATH="$NODE_DIR:$PATH"

# Create helper script
cat > "$NODE_DIR/start.sh" <<EOF
#!/bin/bash
set -a
source $NODE_DIR/.env
set +a
$NODE_DIR/ouro start
EOF
chmod +x "$NODE_DIR/start.sh"

# Start node
echo ""
echo "Starting Ouroboros node..."

if [ "$OS" = "Linux" ] && command -v systemctl &> /dev/null; then
    sudo systemctl start ouroboros
    sleep 3
elif [ "$OS" = "Darwin" ]; then
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
    echo "API: http://localhost:8000"
    echo "Data: $DATA_DIR"
    echo ""
    echo "CLI Commands:"
    echo "  $NODE_DIR/ouro status    - Dashboard"
    echo "  $NODE_DIR/ouro peers     - Connected peers"
    echo "  $NODE_DIR/ouro diagnose  - Run diagnostics"
    echo ""
    if [ "$OS" = "Linux" ]; then
        echo "Service Commands:"
        echo "  sudo systemctl status ouroboros"
        echo "  sudo systemctl stop ouroboros"
        echo "  sudo systemctl restart ouroboros"
    elif [ "$OS" = "Darwin" ]; then
        echo "Service Commands:"
        echo "  launchctl list | grep ouroboros"
        echo "  launchctl unload ~/Library/LaunchAgents/network.ouroboros.node.plist"
    fi
    echo ""
    echo "Logs: tail -f $NODE_DIR/node.log"
    echo ""
    echo "You're now part of the Ouroboros network!"
    echo "=========================================="
else
    echo "Warning: Node may still be starting..."
    echo "Check status: $NODE_DIR/ouro status"
    echo "Check logs: tail -f $NODE_DIR/node.log"
fi
echo ""
