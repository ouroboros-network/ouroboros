#!/bin/bash
set -e

# Log everything
exec > >(tee -a /var/log/startup-script.log)
exec 2>&1

echo "=========================================="
echo "Ouroboros Node Startup Script"
echo "Starting at: $(date)"
echo "=========================================="

# Update system
echo "Updating system packages..."
apt-get update
DEBIAN_FRONTEND=noninteractive apt-get install -y \
    curl \
    ca-certificates \
    python3 \
    python3-venv

# Mount data disk
echo "Setting up data disk..."
DEVICE_NAME="/dev/disk/by-id/google-ouro-rocksdb-data"
MOUNT_POINT="/mnt/blockchain-data"

mkdir -p $MOUNT_POINT

if [ -e "$DEVICE_NAME" ]; then
    # Format if needed
    if ! blkid $DEVICE_NAME; then
        echo "Formatting disk..."
        mkfs.ext4 -F $DEVICE_NAME
    fi

    # Mount
    echo "Mounting disk..."
    mount -o discard,defaults $DEVICE_NAME $MOUNT_POINT

    # Add to fstab for auto-mount on reboot
    if ! grep -q "$DEVICE_NAME" /etc/fstab; then
        echo "$DEVICE_NAME $MOUNT_POINT ext4 discard,defaults,nofail 0 2" >> /etc/fstab
    fi
fi

# Download prebuilt binary from GitHub releases
echo "Downloading Ouroboros node binary..."
REPO="ouroboros-network/ouroboros"
ARCH=$(uname -m)
case "$ARCH" in
    x86_64)       ASSET="ouro-linux-x64" ;;
    aarch64|arm64) ASSET="ouro-linux-arm64" ;;
    *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

INSTALL_DIR="/usr/local/bin"
curl -fsSL "https://github.com/$REPO/releases/latest/download/$ASSET" -o "$INSTALL_DIR/ouro"
chmod +x "$INSTALL_DIR/ouro"

VERSION=$("$INSTALL_DIR/ouro" --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' || echo "unknown")
echo "Installed ouro v$VERSION"

# Download Python tier files
echo "Downloading Python tier files..."
CONFIG_DIR="/root/.ouroboros"
PY_DIR="$CONFIG_DIR/ouro_py"
RAW_BASE="https://raw.githubusercontent.com/$REPO/main"
mkdir -p "$PY_DIR/ouro_medium" "$PY_DIR/ouro_light"
curl -sL -o "$PY_DIR/requirements.txt" "$RAW_BASE/ouro_py/requirements.txt" 2>/dev/null || true
curl -sL -o "$PY_DIR/ouro_medium/main.py" "$RAW_BASE/ouro_py/ouro_medium/main.py" 2>/dev/null || true
curl -sL -o "$PY_DIR/ouro_light/main.py" "$RAW_BASE/ouro_py/ouro_light/main.py" 2>/dev/null || true

# Initialize node config
echo "Configuring node..."
mkdir -p "$CONFIG_DIR"
if [ ! -f "$CONFIG_DIR/config.json" ]; then
    "$INSTALL_DIR/ouro" register-node > /dev/null 2>&1 || true
fi

# Create systemd service
echo "Creating systemd service..."
cat > /etc/systemd/system/ouroboros.service << 'EOF'
[Unit]
Description=Ouroboros Blockchain Node
After=network.target

[Service]
Type=simple
User=root
Environment="DATABASE_PATH=/mnt/blockchain-data/rocksdb"
Environment="RUST_LOG=info"
Environment="RUST_BACKTRACE=1"
ExecStart=/usr/local/bin/ouro start
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
EOF

# Start service
echo "Starting Ouroboros service..."
systemctl daemon-reload
systemctl enable ouroboros
systemctl start ouroboros

echo "=========================================="
echo "Startup complete at: $(date)"
echo "=========================================="
echo "Service status:"
systemctl status ouroboros --no-pager

echo ""
echo "To view logs:"
echo "  journalctl -u ouroboros -f"
