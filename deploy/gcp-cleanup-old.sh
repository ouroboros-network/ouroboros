#!/bin/bash
# Remove Old PostgreSQL-based Deployment
set -e

PROJECT_ID="ultimate-flame-407206"
ZONE="us-central1-a"
OLD_INSTANCE="ouro-node-1"

echo "=========================================="
echo "Cleaning Up Old Deployment"
echo "=========================================="
echo "This will remove: $OLD_INSTANCE"
echo ""

# Confirm
read -p "Are you sure you want to delete the old instance? (yes/no): " -r
echo
if [[ ! $REPLY =~ ^yes$ ]]; then
    echo "Cancelled."
    exit 1
fi

# Set project
gcloud config set project $PROJECT_ID

# Stop and delete instance
echo "Stopping instance..."
gcloud compute instances stop $OLD_INSTANCE --zone=$ZONE || true

echo "Deleting instance..."
gcloud compute instances delete $OLD_INSTANCE --zone=$ZONE --quiet

echo ""
echo "Old deployment removed successfully!"
echo ""
echo "Note: This script does NOT delete:"
echo "  - Firewall rules (may be reused)"
echo "  - Persistent disks (delete manually if needed)"
echo "  - PostgreSQL databases (if any)"
echo ""
echo "To list remaining resources:"
echo "  gcloud compute instances list"
echo "  gcloud compute disks list"
echo ""
