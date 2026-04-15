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

# Create unprivileged user
echo "Creating unprivileged user 'ouroboros'..."
useradd -m -s /bin/bash ouroboros || true

# ... (Mount disk logic)
chown ouroboros:ouroboros $MOUNT_POINT

# Download and verify binary
echo "Downloading Ouroboros node binary..."
# ... (ASSET selection logic)

EXPECTED_SHA256="0f52b069d261399434e320d3f2a89324a1f68748303e919864070e30965d1d6a" # Example for v1.5.2-linux-x64
curl -fsSL "https://github.com/$REPO/releases/latest/download/$ASSET" -o "$INSTALL_DIR/ouro"
echo "$EXPECTED_SHA256 $INSTALL_DIR/ouro" | sha256sum -c - || {
    echo "ERROR: Checksum verification failed! Binary may be compromised."
    exit 1
}
chmod +x "$INSTALL_DIR/ouro"

# ... (Python files download logic)
chown -R ouroboros:ouroboros "$CONFIG_DIR"

# Create systemd service
echo "Creating systemd service..."
cat > /etc/systemd/system/ouroboros.service << 'EOF'
[Unit]
Description=Ouroboros Blockchain Node
After=network.target

[Service]
Type=simple
User=ouroboros
Group=ouroboros
WorkingDirectory=/home/ouroboros
Environment="DATABASE_PATH=/mnt/blockchain-data/rocksdb"
Environment="RUST_LOG=info"
ExecStart=/usr/local/bin/ouro start
Restart=always
RestartSec=10
# Security hardening
NoNewPrivileges=true
ProtectSystem=full
ProtectHome=read-only

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
