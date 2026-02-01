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
    build-essential \
    cmake \
    libsnappy-dev \
    zlib1g-dev \
    libbz2-dev \
    libgflags-dev \
    liblz4-dev \
    libzstd-dev \
    libssl-dev \
    pkg-config \
    curl \
    ca-certificates \
    git

# Install Rust
echo "Installing Rust..."
if [ ! -d "/root/.cargo" ]; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
fi
source /root/.cargo/env

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

# Clone repository
echo "Cloning Ouroboros repository..."
cd /opt
if [ ! -d "ouroboros" ]; then
    git clone https://github.com/ouroboros-network/ouroboros.git
else
    cd ouroboros
    git pull origin main
    cd ..
fi

cd ouroboros/ouro_dag

# Build release binary
echo "Building Ouroboros node (this will take 20-30 minutes)..."
cargo build --release --bin ouro_dag

# Create systemd service
echo "Creating systemd service..."
cat > /etc/systemd/system/ouroboros.service << 'EOF'
[Unit]
Description=Ouroboros Blockchain Node
After=network.target

[Service]
Type=simple
User=root
WorkingDirectory=/opt/ouroboros/ouro_dag
Environment="ROCKSDB_PATH=/mnt/blockchain-data/rocksdb"
Environment="RUST_LOG=info"
Environment="RUST_BACKTRACE=1"
ExecStart=/opt/ouroboros/ouro_dag/target/release/ouro_dag start
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
