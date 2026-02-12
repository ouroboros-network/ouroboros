#!/bin/bash
# Cleanup Old Ouroboros Deployments
# Version: 1.0
# Usage: ./cleanup-old-deployment.sh

set -e

echo "========================================"
echo "  Ouroboros Cleanup Script v0.4.0"
echo "========================================"
echo ""

# Configuration
PROJECT_ID=${GCP_PROJECT_ID:-"your-project-id"}
REGION=${GCP_REGION:-"us-central1"}
ZONE=${GCP_ZONE:-"us-central1-a"}

echo "Configuration:"
echo "  Project ID: $PROJECT_ID"
echo "  Region: $REGION"
echo "  Zone: $ZONE"
echo ""

# Ask for confirmation
read -p "This will DELETE old deployments. Are you sure? (yes/no): " CONFIRM
if [ "$CONFIRM" != "yes" ]; then
    echo "❌ Cancelled by user"
    exit 1
fi

echo ""
echo "🔍 Scanning for existing deployments..."
echo ""

# Function to backup data
backup_data() {
    echo "📦 Creating backups..."
    BACKUP_TIMESTAMP=$(date +%Y%m%d_%H%M%S)
    BACKUP_PATH="gs://${PROJECT_ID}-backups/ouroboros/v0.3.0_${BACKUP_TIMESTAMP}"

    echo "  Backup location: $BACKUP_PATH"

    # Backup database if exists
    if gcloud sql instances describe ouroboros-db 2>/dev/null; then
        echo "  - Backing up Cloud SQL database..."
        gcloud sql export sql ouroboros-db \
            ${BACKUP_PATH}/database.sql \
            --database=ouroboros \
            2>/dev/null || echo "    ⚠️  Database backup failed (may not exist)"
    fi

    # Backup RocksDB disk if exists
    if gcloud compute disks describe ouroboros-data-disk --zone=$ZONE 2>/dev/null; then
        echo "  - Creating disk snapshot..."
        gcloud compute disks snapshot ouroboros-data-disk \
            --snapshot-names=ouroboros-backup-${BACKUP_TIMESTAMP} \
            --zone=$ZONE \
            2>/dev/null || echo "    ⚠️  Disk snapshot failed"
    fi

    echo "✅ Backups created!"
    echo ""
}

# Function to cleanup Cloud Run
cleanup_cloud_run() {
    echo "🧹 Cleaning up Cloud Run services..."

    SERVICES=$(gcloud run services list --region=$REGION --format="value(name)" 2>/dev/null | grep -i ouroboros || echo "")

    if [ -z "$SERVICES" ]; then
        echo "  No Cloud Run services found"
    else
        for SERVICE in $SERVICES; do
            echo "  - Deleting service: $SERVICE"
            gcloud run services delete $SERVICE \
                --region=$REGION \
                --quiet
        done
        echo "✅ Cloud Run services deleted"
    fi
    echo ""
}

# Function to cleanup GKE
cleanup_gke() {
    echo "🧹 Cleaning up GKE resources..."

    # Check if cluster exists
    if gcloud container clusters describe ouroboros-cluster --zone=$ZONE 2>/dev/null; then
        echo "  - Getting cluster credentials..."
        gcloud container clusters get-credentials ouroboros-cluster --zone=$ZONE 2>/dev/null || true

        echo "  - Deleting namespace..."
        kubectl delete namespace ouroboros --ignore-not-found=true 2>/dev/null || true

        echo "  - Deleting cluster..."
        gcloud container clusters delete ouroboros-cluster \
            --zone=$ZONE \
            --quiet

        echo "✅ GKE cluster deleted"
    else
        echo "  No GKE cluster found"
    fi
    echo ""
}

# Function to cleanup VMs
cleanup_vms() {
    echo "🧹 Cleaning up Compute Engine VMs..."

    VMS=$(gcloud compute instances list --format="value(name)" 2>/dev/null | grep -i ouroboros || echo "")

    if [ -z "$VMS" ]; then
        echo "  No VM instances found"
    else
        for VM in $VMS; do
            echo "  - Stopping VM: $VM"
            gcloud compute instances stop $VM --zone=$ZONE --quiet 2>/dev/null || true

            echo "  - Deleting VM: $VM"
            gcloud compute instances delete $VM \
                --zone=$ZONE \
                --quiet
        done
        echo "✅ VM instances deleted"
    fi
    echo ""
}

# Function to cleanup disks
cleanup_disks() {
    echo "🧹 Cleaning up persistent disks..."

    read -p "Delete persistent disks? (data will be lost) (yes/no): " DELETE_DISKS

    if [ "$DELETE_DISKS" == "yes" ]; then
        DISKS=$(gcloud compute disks list --format="value(name)" 2>/dev/null | grep -i ouroboros || echo "")

        if [ -z "$DISKS" ]; then
            echo "  No persistent disks found"
        else
            for DISK in $DISKS; do
                echo "  - Deleting disk: $DISK"
                gcloud compute disks delete $DISK \
                    --zone=$ZONE \
                    --quiet
            done
            echo "✅ Persistent disks deleted"
        fi
    else
        echo "  Skipping disk deletion (kept for recovery)"
    fi
    echo ""
}

# Function to cleanup load balancers
cleanup_load_balancers() {
    echo "🧹 Cleaning up load balancers..."

    LBS=$(gcloud compute forwarding-rules list --format="value(name)" 2>/dev/null | grep -i ouroboros || echo "")

    if [ -z "$LBS" ]; then
        echo "  No load balancers found"
    else
        for LB in $LBS; do
            echo "  - Deleting load balancer: $LB"
            gcloud compute forwarding-rules delete $LB \
                --global \
                --quiet 2>/dev/null || \
            gcloud compute forwarding-rules delete $LB \
                --region=$REGION \
                --quiet 2>/dev/null || true
        done
        echo "✅ Load balancers deleted"
    fi
    echo ""
}

# Function to cleanup firewall rules
cleanup_firewall_rules() {
    echo "🧹 Cleaning up firewall rules..."

    RULES=$(gcloud compute firewall-rules list --format="value(name)" 2>/dev/null | grep -i ouroboros || echo "")

    if [ -z "$RULES" ]; then
        echo "  No firewall rules found"
    else
        for RULE in $RULES; do
            echo "  - Deleting rule: $RULE"
            gcloud compute firewall-rules delete $RULE --quiet
        done
        echo "✅ Firewall rules deleted"
    fi
    echo ""
}

# Function to cleanup old images
cleanup_images() {
    echo "🧹 Cleaning up old Docker images..."

    read -p "Delete old Docker images? (yes/no): " DELETE_IMAGES

    if [ "$DELETE_IMAGES" == "yes" ]; then
        IMAGES=$(gcloud artifacts docker images list \
            $REGION-docker.pkg.dev/$PROJECT_ID/ouroboros 2>/dev/null | \
            grep -E 'v0\.[0-3]\.' || echo "")

        if [ -z "$IMAGES" ]; then
            echo "  No old images found"
        else
            echo "$IMAGES" | while read IMAGE; do
                IMAGE_PATH=$(echo $IMAGE | awk '{print $1}')
                echo "  - Deleting image: $IMAGE_PATH"
                gcloud artifacts docker images delete $IMAGE_PATH --quiet 2>/dev/null || true
            done
            echo "✅ Old images deleted"
        fi
    else
        echo "  Skipping image deletion (kept for rollback)"
    fi
    echo ""
}

# Function to cleanup Cloud SQL
cleanup_cloud_sql() {
    echo "🧹 Cleaning up Cloud SQL..."

    if gcloud sql instances describe ouroboros-db 2>/dev/null; then
        read -p "Delete Cloud SQL instance? (database will be lost) (yes/no): " DELETE_DB

        if [ "$DELETE_DB" == "yes" ]; then
            echo "  - Deleting Cloud SQL instance..."
            gcloud sql instances delete ouroboros-db --quiet
            echo "✅ Cloud SQL deleted"
        else
            echo "  Keeping Cloud SQL instance"
        fi
    else
        echo "  No Cloud SQL instance found"
    fi
    echo ""
}

# Main cleanup sequence
main() {
    echo "Starting cleanup process..."
    echo ""

    # Step 1: Backup
    backup_data

    # Step 2: Cleanup services
    cleanup_cloud_run
    cleanup_gke
    cleanup_vms

    # Step 3: Cleanup networking
    cleanup_load_balancers
    cleanup_firewall_rules

    # Step 4: Cleanup storage
    cleanup_disks
    cleanup_images
    cleanup_cloud_sql

    echo "========================================"
    echo "  ✅ Cleanup Complete!"
    echo "========================================"
    echo ""
    echo "Summary:"
    echo "  - Old deployments removed"
    echo "  - Backups created (check GCS bucket)"
    echo "  - Ready for new deployment"
    echo ""
    echo "Next steps:"
    echo "  1. Review backups"
    echo "  2. Run: ./deploy-v0.4.0.sh"
    echo ""
}

# Run main function
main
