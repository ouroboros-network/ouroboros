#!/bin/bash
# GCP Deployment Script for Ouroboros RocksDB Edition
# SECURITY: This script creates firewall rules with IP restrictions
set -e

PROJECT_ID="ultimate-flame-407206"
REGION="us-central1"
ZONE="us-central1-a"
INSTANCE_NAME="ouro-node-rocksdb"
MACHINE_TYPE="e2-medium"  # Upgraded from e2-small for better performance
IMAGE_FAMILY="debian-12"
IMAGE_PROJECT="debian-cloud"
BOOT_DISK_SIZE="30GB"
DATA_DISK_SIZE="50GB"

# SECURITY: IP Allowlists for firewall rules
# P2P port is open to all (required for peer discovery)
# API port is restricted to specific IPs only
#
# To customize, set these environment variables before running:
#   export ADMIN_IP_RANGES="1.2.3.4/32,5.6.7.8/32"
#   export P2P_IP_RANGES="0.0.0.0/0"  # Open for peer discovery
#
ADMIN_IP_RANGES="${ADMIN_IP_RANGES:-}"
P2P_IP_RANGES="${P2P_IP_RANGES:-0.0.0.0/0}"

echo "=========================================="
echo "Ouroboros RocksDB Deployment to GCP"
echo "=========================================="
echo "Project: $PROJECT_ID"
echo "Region: $REGION"
echo "Instance: $INSTANCE_NAME"
echo ""
echo "SECURITY CONFIGURATION:"
echo "  P2P Port (9000): Open to ${P2P_IP_RANGES}"
if [ -z "$ADMIN_IP_RANGES" ]; then
    echo "  API Port (8000): BLOCKED (no ADMIN_IP_RANGES set)"
    echo ""
    echo "  WARNING: API port will not be accessible!"
    echo "  Set ADMIN_IP_RANGES to enable API access, e.g.:"
    echo "    export ADMIN_IP_RANGES=\"YOUR_IP/32\""
    echo ""
    read -p "Continue without API access? (y/N) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        echo "Aborted. Set ADMIN_IP_RANGES and retry."
        exit 1
    fi
else
    echo "  API Port (8000): Restricted to ${ADMIN_IP_RANGES}"
fi
echo ""

# Set project
gcloud config set project $PROJECT_ID

# Create persistent disk for RocksDB data
echo "Creating persistent disk for blockchain data..."
gcloud compute disks create ${INSTANCE_NAME}-data \
    --size=$DATA_DISK_SIZE \
    --zone=$ZONE \
    --type=pd-standard \
    || echo "Disk already exists, continuing..."

# Create firewall rules with IP restrictions
echo "Setting up firewall rules..."

# P2P port - open to peers for network discovery (required for blockchain)
gcloud compute firewall-rules create ouro-p2p \
    --allow=tcp:9000 \
    --description="Ouroboros P2P port - open for peer discovery" \
    --direction=INGRESS \
    --source-ranges="${P2P_IP_RANGES}" \
    --target-tags=ouro-node \
    || echo "Firewall rule ouro-p2p already exists"

# API port - restricted to admin IPs only
if [ -n "$ADMIN_IP_RANGES" ]; then
    gcloud compute firewall-rules create ouro-api \
        --allow=tcp:8000 \
        --description="Ouroboros API port - restricted to admin IPs" \
        --direction=INGRESS \
        --source-ranges="${ADMIN_IP_RANGES}" \
        --target-tags=ouro-node \
        || echo "Firewall rule ouro-api already exists"
else
    echo "Skipping API firewall rule (no ADMIN_IP_RANGES set)"
fi

# Create startup script
cat > /tmp/startup-script.sh << 'EOF'
#!/bin/bash
set -e

# Update system
apt-get update
apt-get install -y docker.io git curl

# Start Docker
systemctl start docker
systemctl enable docker

# Mount data disk
if ! grep -q "/mnt/disks/data" /etc/fstab; then
    mkdir -p /mnt/disks/data
    DEVICE_NAME=$(ls /dev/disk/by-id/google-${INSTANCE_NAME}-data 2>/dev/null || echo "")
    if [ -n "$DEVICE_NAME" ]; then
        # Format if needed
        if ! blkid $DEVICE_NAME; then
            mkfs.ext4 -F $DEVICE_NAME
        fi
        # Mount
        mount -o discard,defaults $DEVICE_NAME /mnt/disks/data
        echo "$DEVICE_NAME /mnt/disks/data ext4 discard,defaults,nofail 0 2" >> /etc/fstab
    fi
fi

# Clone repository
cd /opt
if [ ! -d "ouroboros" ]; then
    git clone https://github.com/ouroboros-network/ouroboros.git
fi
cd ouroboros
git pull

# Build Docker image
cd ouro_dag
docker build -t ouroboros-node .

# Run container
docker stop ouro-node 2>/dev/null || true
docker rm ouro-node 2>/dev/null || true

docker run -d \
    --name ouro-node \
    --restart unless-stopped \
    -p 8000:8000 \
    -p 9000:9000 \
    -v /mnt/disks/data:/data \
    -e ROCKSDB_PATH=/data/rocksdb \
    -e RUST_LOG=info \
    ouroboros-node

echo "Ouroboros node started successfully!"
docker logs ouro-node
EOF

# Create the instance
echo "Creating compute instance..."
gcloud compute instances create $INSTANCE_NAME \
    --zone=$ZONE \
    --machine-type=$MACHINE_TYPE \
    --image-family=$IMAGE_FAMILY \
    --image-project=$IMAGE_PROJECT \
    --boot-disk-size=$BOOT_DISK_SIZE \
    --boot-disk-type=pd-standard \
    --disk=name=${INSTANCE_NAME}-data,mode=rw \
    --metadata-from-file=startup-script=/tmp/startup-script.sh \
    --tags=ouro-node \
    --scopes=cloud-platform

# Get external IP
EXTERNAL_IP=$(gcloud compute instances describe $INSTANCE_NAME \
    --zone=$ZONE \
    --format='get(networkInterfaces[0].accessConfigs[0].natIP)')

echo ""
echo "=========================================="
echo "Deployment Complete!"
echo "=========================================="
echo "Instance: $INSTANCE_NAME"
echo "External IP: $EXTERNAL_IP"
echo "API: http://$EXTERNAL_IP:8000"
echo "P2P: $EXTERNAL_IP:9000"
echo ""
echo "To check status:"
echo "  gcloud compute instances describe $INSTANCE_NAME --zone=$ZONE"
echo ""
echo "To SSH into instance:"
echo "  gcloud compute ssh $INSTANCE_NAME --zone=$ZONE"
echo ""
echo "To view logs:"
echo "  gcloud compute ssh $INSTANCE_NAME --zone=$ZONE --command='docker logs ouro-node'"
echo ""
