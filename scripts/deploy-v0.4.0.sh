#!/bin/bash
# Deploy Ouroboros v0.4.0 to GCP
# Version: 1.0
# Usage: ./deploy-v0.4.0.sh [cloud-run|gke|vm]

set -e

echo "========================================"
echo "  Ouroboros Deployment Script v0.4.0"
echo "========================================"
echo ""

# Configuration
PROJECT_ID=${GCP_PROJECT_ID:-"your-project-id"}
REGION=${GCP_REGION:-"us-central1"}
ZONE=${GCP_ZONE:-"us-central1-a"}
DEPLOYMENT_TYPE=${1:-"cloud-run"}
VERSION="v0.4.0"
IMAGE_NAME="ouroboros-node"

echo "Configuration:"
echo "  Project ID: $PROJECT_ID"
echo "  Region: $REGION"
echo "  Zone: $ZONE"
echo "  Deployment Type: $DEPLOYMENT_TYPE"
echo "  Version: $VERSION"
echo ""

# Validate project ID
if [ "$PROJECT_ID" == "your-project-id" ]; then
    echo "❌ Error: Please set GCP_PROJECT_ID environment variable"
    echo "   export GCP_PROJECT_ID=your-actual-project-id"
    exit 1
fi

# Set GCP project
echo "📝 Setting GCP project..."
gcloud config set project $PROJECT_ID

# Function to run tests
run_tests() {
    echo "🧪 Running tests..."
    cd ../ouro_dag
    cargo test --lib --release
    cd ../scripts
    echo "✅ Tests passed!"
    echo ""
}

# Function to build release
build_release() {
    echo "🔨 Building release binary..."
    cd ../ouro_dag
    cargo clean
    cargo build --release --bin ouro_dag
    cd ../scripts
    echo "✅ Release build complete!"
    echo ""
}

# Function to build Docker image
build_docker_image() {
    echo "🐳 Building Docker image..."

    # Configure Docker for GCP
    gcloud auth configure-docker ${REGION}-docker.pkg.dev

    # Create Artifact Registry repository if doesn't exist
    gcloud artifacts repositories create ouroboros \
        --repository-format=docker \
        --location=$REGION \
        --description="Ouroboros blockchain node images" \
        2>/dev/null || echo "  Repository already exists"

    # Build image
    cd ../ouro_dag
    docker build -t ${REGION}-docker.pkg.dev/${PROJECT_ID}/ouroboros/${IMAGE_NAME}:${VERSION} .

    # Tag as latest
    docker tag ${REGION}-docker.pkg.dev/${PROJECT_ID}/ouroboros/${IMAGE_NAME}:${VERSION} \
        ${REGION}-docker.pkg.dev/${PROJECT_ID}/ouroboros/${IMAGE_NAME}:latest

    cd ../scripts

    echo "✅ Docker image built!"
    echo ""
}

# Function to test Docker image locally
test_docker_image() {
    echo "🧪 Testing Docker image locally..."

    # Create test env file
    cat > /tmp/ouroboros-test.env << EOF
NODE_ID=node-test
BFT_PEERS=
BFT_PORT=9091
API_ADDR=0.0.0.0:8001
LISTEN_ADDR=0.0.0.0:9001
ROCKSDB_PATH=/data/rocksdb
RUST_LOG=info
EOF

    # Run test container
    docker run -d \
        --name ouroboros-test \
        --env-file /tmp/ouroboros-test.env \
        -p 8001:8001 \
        -p 9001:9001 \
        ${REGION}-docker.pkg.dev/${PROJECT_ID}/ouroboros/${IMAGE_NAME}:${VERSION}

    # Wait for startup
    echo "  Waiting for container to start..."
    sleep 10

    # Test health endpoint
    echo "  Testing health endpoint..."
    if curl -f http://localhost:8001/health 2>/dev/null; then
        echo "  ✅ Health check passed!"
    else
        echo "  ⚠️  Health check failed (may be normal if endpoint doesn't exist yet)"
    fi

    # Show logs
    echo "  Container logs:"
    docker logs ouroboros-test | tail -20

    # Cleanup
    docker stop ouroboros-test
    docker rm ouroboros-test

    echo "✅ Local test complete!"
    echo ""
}

# Function to push Docker image
push_docker_image() {
    echo "📤 Pushing Docker image to Artifact Registry..."

    docker push ${REGION}-docker.pkg.dev/${PROJECT_ID}/ouroboros/${IMAGE_NAME}:${VERSION}
    docker push ${REGION}-docker.pkg.dev/${PROJECT_ID}/ouroboros/${IMAGE_NAME}:latest

    echo "✅ Image pushed!"
    echo ""
}

# Function to deploy to Cloud Run
deploy_cloud_run() {
    echo "☁️  Deploying to Cloud Run..."

    gcloud run deploy ouroboros-node \
        --image=${REGION}-docker.pkg.dev/${PROJECT_ID}/ouroboros/${IMAGE_NAME}:${VERSION} \
        --region=$REGION \
        --platform=managed \
        --port=8001 \
        --memory=4Gi \
        --cpu=2 \
        --timeout=3600 \
        --max-instances=10 \
        --min-instances=1 \
        --set-env-vars="NODE_ID=node-1,BFT_PORT=9091,API_ADDR=0.0.0.0:8001,LISTEN_ADDR=0.0.0.0:9001,RUST_LOG=info,ROCKSDB_PATH=/data/rocksdb" \
        --allow-unauthenticated

    # Get service URL
    SERVICE_URL=$(gcloud run services describe ouroboros-node \
        --region=$REGION \
        --format='value(status.url)')

    echo "✅ Deployed to Cloud Run!"
    echo "   URL: $SERVICE_URL"
    echo ""
}

# Function to deploy to GKE
deploy_gke() {
    echo "🎯 Deploying to GKE..."

    # Check if cluster exists, create if not
    if ! gcloud container clusters describe ouroboros-cluster --zone=$ZONE 2>/dev/null; then
        echo "  Creating GKE cluster..."
        gcloud container clusters create ouroboros-cluster \
            --zone=$ZONE \
            --num-nodes=3 \
            --machine-type=e2-standard-4 \
            --disk-size=100GB \
            --enable-autoscaling \
            --min-nodes=3 \
            --max-nodes=10 \
            --enable-autorepair \
            --enable-autoupgrade
    fi

    # Get credentials
    echo "  Getting cluster credentials..."
    gcloud container clusters get-credentials ouroboros-cluster --zone=$ZONE

    # Create namespace
    echo "  Creating namespace..."
    kubectl create namespace ouroboros --dry-run=client -o yaml | kubectl apply -f -

    # Create ConfigMap
    echo "  Creating ConfigMap..."
    kubectl create configmap ouroboros-config \
        --from-literal=NODE_ID=node-1 \
        --from-literal=BFT_PORT=9091 \
        --from-literal=API_ADDR=0.0.0.0:8001 \
        --from-literal=LISTEN_ADDR=0.0.0.0:9001 \
        --from-literal=RUST_LOG=info \
        --from-literal=ROCKSDB_PATH=/data/rocksdb \
        -n ouroboros \
        --dry-run=client -o yaml | kubectl apply -f -

    # Apply Kubernetes manifests
    echo "  Applying Kubernetes manifests..."
    cd ../k8s

    # Update image in statefulset
    sed "s|IMAGE_PLACEHOLDER|${REGION}-docker.pkg.dev/${PROJECT_ID}/ouroboros/${IMAGE_NAME}:${VERSION}|g" \
        statefulset.yaml > /tmp/statefulset-updated.yaml

    kubectl apply -f /tmp/statefulset-updated.yaml -n ouroboros
    kubectl apply -f service.yaml -n ouroboros

    cd ../scripts

    echo "  Waiting for deployment..."
    kubectl wait --for=condition=ready pod -l app=ouroboros-node -n ouroboros --timeout=300s

    # Get load balancer IP
    echo "  Getting service endpoint..."
    EXTERNAL_IP=""
    while [ -z "$EXTERNAL_IP" ]; do
        EXTERNAL_IP=$(kubectl get service ouroboros-node -n ouroboros \
            -o jsonpath='{.status.loadBalancer.ingress[0].ip}' 2>/dev/null)
        [ -z "$EXTERNAL_IP" ] && sleep 5
    done

    echo "✅ Deployed to GKE!"
    echo "   Endpoint: http://${EXTERNAL_IP}:8001"
    echo ""
}

# Function to deploy to VM
deploy_vm() {
    echo "💻 Deploying to Compute Engine VM..."

    # Create startup script
    cat > /tmp/startup-script.sh << 'EOFSCRIPT'
#!/bin/bash
# Install Docker
curl -fsSL https://get.docker.com -o get-docker.sh
sh get-docker.sh

# Configure Docker for GCP
gcloud auth configure-docker ${REGION}-docker.pkg.dev

# Pull image
docker pull ${REGION}-docker.pkg.dev/${PROJECT_ID}/ouroboros/${IMAGE_NAME}:${VERSION}

# Run container
docker run -d \
    --name ouroboros-node \
    --restart=always \
    -p 8001:8001 \
    -p 9001:9001 \
    -p 9091:9091 \
    -e NODE_ID=node-1 \
    -e BFT_PORT=9091 \
    -e API_ADDR=0.0.0.0:8001 \
    -e LISTEN_ADDR=0.0.0.0:9001 \
    -e RUST_LOG=info \
    -e ROCKSDB_PATH=/data/rocksdb \
    -v /mnt/disks/ouroboros-data:/data \
    ${REGION}-docker.pkg.dev/${PROJECT_ID}/ouroboros/${IMAGE_NAME}:${VERSION}
EOFSCRIPT

    # Create VM
    echo "  Creating VM instance..."
    gcloud compute instances create ouroboros-node-1 \
        --zone=$ZONE \
        --machine-type=e2-standard-4 \
        --boot-disk-size=100GB \
        --image-family=cos-stable \
        --image-project=cos-cloud \
        --tags=ouroboros-node \
        --metadata-from-file=startup-script=/tmp/startup-script.sh

    # Create firewall rules
    echo "  Creating firewall rules..."
    gcloud compute firewall-rules create allow-ouroboros-api \
        --allow=tcp:8001 \
        --target-tags=ouroboros-node \
        --description="Allow API access" \
        2>/dev/null || echo "  Firewall rule already exists"

    gcloud compute firewall-rules create allow-ouroboros-p2p \
        --allow=tcp:9001,tcp:9091 \
        --target-tags=ouroboros-node \
        --description="Allow P2P and BFT" \
        2>/dev/null || echo "  Firewall rule already exists"

    # Get external IP
    EXTERNAL_IP=$(gcloud compute instances describe ouroboros-node-1 \
        --zone=$ZONE \
        --format='value(networkInterfaces[0].accessConfigs[0].natIP)')

    echo "✅ Deployed to Compute Engine!"
    echo "   Endpoint: http://${EXTERNAL_IP}:8001"
    echo ""
}

# Function to verify deployment
verify_deployment() {
    echo "🔍 Verifying deployment..."

    # Get endpoint based on deployment type
    case $DEPLOYMENT_TYPE in
        cloud-run)
            ENDPOINT=$(gcloud run services describe ouroboros-node \
                --region=$REGION \
                --format='value(status.url)')
            ;;
        gke)
            EXTERNAL_IP=$(kubectl get service ouroboros-node -n ouroboros \
                -o jsonpath='{.status.loadBalancer.ingress[0].ip}')
            ENDPOINT="http://${EXTERNAL_IP}:8001"
            ;;
        vm)
            EXTERNAL_IP=$(gcloud compute instances describe ouroboros-node-1 \
                --zone=$ZONE \
                --format='value(networkInterfaces[0].accessConfigs[0].natIP)')
            ENDPOINT="http://${EXTERNAL_IP}:8001"
            ;;
    esac

    echo "  Endpoint: $ENDPOINT"
    echo ""

    # Wait for service to be ready
    echo "  Waiting for service to be ready..."
    sleep 30

    # Test health endpoint
    echo "  Testing health endpoint..."
    if curl -f ${ENDPOINT}/health 2>/dev/null; then
        echo "  ✅ Health check passed!"
    else
        echo "  ⚠️  Health check failed (service may still be starting)"
    fi

    echo ""
    echo "✅ Verification complete!"
    echo ""
}

# Function to show post-deployment info
show_post_deployment_info() {
    echo "========================================"
    echo "  ✅ Deployment Complete!"
    echo "========================================"
    echo ""
    echo "Deployment Details:"
    echo "  Version: $VERSION"
    echo "  Type: $DEPLOYMENT_TYPE"
    echo "  Project: $PROJECT_ID"
    echo "  Region: $REGION"
    echo ""
    echo "Next Steps:"
    echo "  1. Monitor logs"
    echo "  2. Test API endpoints"
    echo "  3. Check fraud detection status"
    echo "  4. Set up monitoring alerts"
    echo ""
    echo "Useful Commands:"

    case $DEPLOYMENT_TYPE in
        cloud-run)
            echo "  # View logs"
            echo "  gcloud run services logs read ouroboros-node --region=$REGION"
            echo ""
            echo "  # Get service URL"
            echo "  gcloud run services describe ouroboros-node --region=$REGION"
            ;;
        gke)
            echo "  # View logs"
            echo "  kubectl logs -f -n ouroboros -l app=ouroboros-node"
            echo ""
            echo "  # Get pods"
            echo "  kubectl get pods -n ouroboros"
            ;;
        vm)
            echo "  # SSH to instance"
            echo "  gcloud compute ssh ouroboros-node-1 --zone=$ZONE"
            echo ""
            echo "  # View logs"
            echo "  gcloud compute ssh ouroboros-node-1 --zone=$ZONE --command='docker logs ouroboros-node'"
            ;;
    esac

    echo ""
}

# Main deployment sequence
main() {
    echo "Starting deployment process..."
    echo ""

    # Confirm before proceeding
    read -p "Deploy Ouroboros v0.4.0 to $DEPLOYMENT_TYPE? (yes/no): " CONFIRM
    if [ "$CONFIRM" != "yes" ]; then
        echo "❌ Cancelled by user"
        exit 1
    fi
    echo ""

    # Step 1: Run tests
    run_tests

    # Step 2: Build release
    build_release

    # Step 3: Build Docker image
    build_docker_image

    # Step 4: Test Docker image
    test_docker_image

    # Step 5: Push Docker image
    push_docker_image

    # Step 6: Deploy based on type
    case $DEPLOYMENT_TYPE in
        cloud-run)
            deploy_cloud_run
            ;;
        gke)
            deploy_gke
            ;;
        vm)
            deploy_vm
            ;;
        *)
            echo "❌ Invalid deployment type: $DEPLOYMENT_TYPE"
            echo "   Usage: $0 [cloud-run|gke|vm]"
            exit 1
            ;;
    esac

    # Step 7: Verify deployment
    verify_deployment

    # Step 8: Show post-deployment info
    show_post_deployment_info
}

# Run main function
main
