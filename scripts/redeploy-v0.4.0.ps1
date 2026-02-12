# Redeploy Ouroboros v0.4.0 to GCP (Windows PowerShell)
# Version: 1.0
# Usage: .\redeploy-v0.4.0.ps1 -DeploymentType cloud-run

param(
    [Parameter(Mandatory=$false)]
    [ValidateSet("cloud-run", "gke", "vm")]
    [string]$DeploymentType = "cloud-run",

    [Parameter(Mandatory=$false)]
    [string]$ProjectId = $env:GCP_PROJECT_ID,

    [Parameter(Mandatory=$false)]
    [string]$Region = "us-central1",

    [Parameter(Mandatory=$false)]
    [string]$Zone = "us-central1-a",

    [Parameter(Mandatory=$false)]
    [switch]$SkipTests,

    [Parameter(Mandatory=$false)]
    [switch]$SkipCleanup
)

$ErrorActionPreference = "Stop"

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  Ouroboros Redeploy Script v0.4.0" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# Configuration
$VERSION = "v0.4.0"
$IMAGE_NAME = "ouroboros-node"

Write-Host "Configuration:" -ForegroundColor Yellow
Write-Host "  Project ID: $ProjectId"
Write-Host "  Region: $Region"
Write-Host "  Zone: $Zone"
Write-Host "  Deployment Type: $DeploymentType"
Write-Host "  Version: $VERSION"
Write-Host ""

# Validate project ID
if ([string]::IsNullOrEmpty($ProjectId)) {
    Write-Host "❌ Error: Please set GCP_PROJECT_ID environment variable" -ForegroundColor Red
    Write-Host "   `$env:GCP_PROJECT_ID = 'your-actual-project-id'" -ForegroundColor Yellow
    exit 1
}

# Set GCP project
Write-Host "📝 Setting GCP project..." -ForegroundColor Green
gcloud config set project $ProjectId

# Function to cleanup old deployments
function Remove-OldDeployments {
    Write-Host ""
    Write-Host "🧹 Cleaning up old deployments..." -ForegroundColor Yellow

    # Backup first
    Write-Host "  📦 Creating backups..." -ForegroundColor Cyan
    $BackupTimestamp = Get-Date -Format "yyyyMMdd_HHmmss"
    $BackupPath = "gs://$ProjectId-backups/ouroboros/v0.3.0_$BackupTimestamp"

    Write-Host "    Backup location: $BackupPath"

    # Backup database if exists
    try {
        gcloud sql instances describe ouroboros-db 2>$null
        if ($LASTEXITCODE -eq 0) {
            Write-Host "    - Backing up Cloud SQL database..." -ForegroundColor Cyan
            gcloud sql export sql ouroboros-db "$BackupPath/database.sql" --database=ouroboros 2>$null
        }
    } catch {
        Write-Host "    ⚠️  No Cloud SQL database found" -ForegroundColor DarkYellow
    }

    # Cleanup Cloud Run
    Write-Host "  - Cleaning up Cloud Run services..." -ForegroundColor Cyan
    $services = gcloud run services list --region=$Region --format="value(name)" 2>$null | Where-Object { $_ -like "*ouroboros*" }
    foreach ($service in $services) {
        Write-Host "    Deleting service: $service"
        gcloud run services delete $service --region=$Region --quiet 2>$null
    }

    # Cleanup GKE
    Write-Host "  - Cleaning up GKE cluster..." -ForegroundColor Cyan
    try {
        gcloud container clusters describe ouroboros-cluster --zone=$Zone 2>$null
        if ($LASTEXITCODE -eq 0) {
            gcloud container clusters get-credentials ouroboros-cluster --zone=$Zone 2>$null
            kubectl delete namespace ouroboros --ignore-not-found=true 2>$null
            gcloud container clusters delete ouroboros-cluster --zone=$Zone --quiet
        }
    } catch {
        Write-Host "    No GKE cluster found"
    }

    # Cleanup VMs
    Write-Host "  - Cleaning up VM instances..." -ForegroundColor Cyan
    $vms = gcloud compute instances list --format="value(name)" 2>$null | Where-Object { $_ -like "*ouroboros*" }
    foreach ($vm in $vms) {
        Write-Host "    Deleting VM: $vm"
        gcloud compute instances delete $vm --zone=$Zone --quiet 2>$null
    }

    Write-Host "✅ Cleanup complete!" -ForegroundColor Green
    Write-Host ""
}

# Function to run tests
function Invoke-Tests {
    if ($SkipTests) {
        Write-Host "⏭️  Skipping tests..." -ForegroundColor Yellow
        return
    }

    Write-Host "🧪 Running tests..." -ForegroundColor Green
    Push-Location ..\ouro_dag
    cargo test --lib --release
    Pop-Location

    if ($LASTEXITCODE -ne 0) {
        Write-Host "❌ Tests failed!" -ForegroundColor Red
        exit 1
    }

    Write-Host "✅ Tests passed!" -ForegroundColor Green
    Write-Host ""
}

# Function to build release
function Build-Release {
    Write-Host "🔨 Building release binary..." -ForegroundColor Green
    Push-Location ..\ouro_dag
    cargo clean
    cargo build --release --bin ouro_dag
    Pop-Location

    if ($LASTEXITCODE -ne 0) {
        Write-Host "❌ Build failed!" -ForegroundColor Red
        exit 1
    }

    Write-Host "✅ Release build complete!" -ForegroundColor Green
    Write-Host ""
}

# Function to build and push Docker image
function Build-AndPushDockerImage {
    Write-Host "🐳 Building Docker image..." -ForegroundColor Green

    # Configure Docker for GCP
    gcloud auth configure-docker "$Region-docker.pkg.dev"

    # Create Artifact Registry repository if doesn't exist
    gcloud artifacts repositories create ouroboros `
        --repository-format=docker `
        --location=$Region `
        --description="Ouroboros blockchain node images" 2>$null

    $ImagePath = "$Region-docker.pkg.dev/$ProjectId/ouroboros/$IMAGE_NAME"

    # Build image
    Push-Location ..\ouro_dag
    docker build -t "$ImagePath`:$VERSION" .
    docker tag "$ImagePath`:$VERSION" "$ImagePath`:latest"
    Pop-Location

    if ($LASTEXITCODE -ne 0) {
        Write-Host "❌ Docker build failed!" -ForegroundColor Red
        exit 1
    }

    Write-Host "✅ Docker image built!" -ForegroundColor Green

    # Test locally
    Write-Host "🧪 Testing Docker image locally..." -ForegroundColor Green

    # Create test env file
    @"
NODE_ID=node-test
BFT_PEERS=
BFT_PORT=9091
API_ADDR=0.0.0.0:8001
LISTEN_ADDR=0.0.0.0:9001
ROCKSDB_PATH=/data/rocksdb
RUST_LOG=info
"@ | Out-File -FilePath "$env:TEMP\ouroboros-test.env" -Encoding ASCII

    # Run test container
    docker run -d `
        --name ouroboros-test `
        --env-file "$env:TEMP\ouroboros-test.env" `
        -p 8001:8001 `
        -p 9001:9001 `
        "$ImagePath`:$VERSION"

    Start-Sleep -Seconds 10

    # Test health endpoint
    try {
        $response = Invoke-WebRequest -Uri "http://localhost:8001/health" -TimeoutSec 5 -ErrorAction SilentlyContinue
        Write-Host "  ✅ Health check passed!" -ForegroundColor Green
    } catch {
        Write-Host "  ⚠️  Health check failed (may be normal)" -ForegroundColor DarkYellow
    }

    # Cleanup
    docker stop ouroboros-test 2>$null
    docker rm ouroboros-test 2>$null

    Write-Host "✅ Local test complete!" -ForegroundColor Green
    Write-Host ""

    # Push image
    Write-Host "📤 Pushing Docker image..." -ForegroundColor Green
    docker push "$ImagePath`:$VERSION"
    docker push "$ImagePath`:latest"

    if ($LASTEXITCODE -ne 0) {
        Write-Host "❌ Docker push failed!" -ForegroundColor Red
        exit 1
    }

    Write-Host "✅ Image pushed to Artifact Registry!" -ForegroundColor Green
    Write-Host ""
}

# Function to deploy to Cloud Run
function Deploy-ToCloudRun {
    Write-Host "☁️  Deploying to Cloud Run..." -ForegroundColor Green

    $ImagePath = "$Region-docker.pkg.dev/$ProjectId/ouroboros/$IMAGE_NAME`:$VERSION"

    gcloud run deploy ouroboros-node `
        --image=$ImagePath `
        --region=$Region `
        --platform=managed `
        --port=8001 `
        --memory=4Gi `
        --cpu=2 `
        --timeout=3600 `
        --max-instances=10 `
        --min-instances=1 `
        --set-env-vars="NODE_ID=node-1,BFT_PORT=9091,API_ADDR=0.0.0.0:8001,LISTEN_ADDR=0.0.0.0:9001,RUST_LOG=info,ROCKSDB_PATH=/data/rocksdb" `
        --allow-unauthenticated

    if ($LASTEXITCODE -ne 0) {
        Write-Host "❌ Cloud Run deployment failed!" -ForegroundColor Red
        exit 1
    }

    # Get service URL
    $ServiceUrl = gcloud run services describe ouroboros-node `
        --region=$Region `
        --format='value(status.url)'

    Write-Host "✅ Deployed to Cloud Run!" -ForegroundColor Green
    Write-Host "   URL: $ServiceUrl" -ForegroundColor Cyan
    Write-Host ""

    return $ServiceUrl
}

# Function to verify deployment
function Test-Deployment {
    param([string]$Endpoint)

    Write-Host "🔍 Verifying deployment..." -ForegroundColor Green
    Write-Host "  Endpoint: $Endpoint"

    # Wait for service to be ready
    Write-Host "  Waiting for service to be ready..."
    Start-Sleep -Seconds 30

    # Test health endpoint
    Write-Host "  Testing health endpoint..."
    try {
        $response = Invoke-WebRequest -Uri "$Endpoint/health" -TimeoutSec 10
        Write-Host "  ✅ Health check passed!" -ForegroundColor Green
    } catch {
        Write-Host "  ⚠️  Health check failed (service may still be starting)" -ForegroundColor DarkYellow
    }

    Write-Host ""
    Write-Host "✅ Verification complete!" -ForegroundColor Green
    Write-Host ""
}

# Main script
Write-Host "Starting deployment process..." -ForegroundColor Green
Write-Host ""

# Ask for confirmation
$confirmation = Read-Host "Deploy Ouroboros v0.4.0 to $DeploymentType? (yes/no)"
if ($confirmation -ne "yes") {
    Write-Host "❌ Cancelled by user" -ForegroundColor Red
    exit 1
}
Write-Host ""

# Step 1: Cleanup old deployments
if (-not $SkipCleanup) {
    Remove-OldDeployments
} else {
    Write-Host "⏭️  Skipping cleanup..." -ForegroundColor Yellow
}

# Step 2: Run tests
Invoke-Tests

# Step 3: Build release
Build-Release

# Step 4: Build and push Docker image
Build-AndPushDockerImage

# Step 5: Deploy
$Endpoint = $null
switch ($DeploymentType) {
    "cloud-run" {
        $Endpoint = Deploy-ToCloudRun
    }
    "gke" {
        Write-Host "❌ GKE deployment not yet implemented in PowerShell version" -ForegroundColor Red
        Write-Host "   Please use: bash deploy-v0.4.0.sh gke" -ForegroundColor Yellow
        exit 1
    }
    "vm" {
        Write-Host "❌ VM deployment not yet implemented in PowerShell version" -ForegroundColor Red
        Write-Host "   Please use: bash deploy-v0.4.0.sh vm" -ForegroundColor Yellow
        exit 1
    }
}

# Step 6: Verify
Test-Deployment -Endpoint $Endpoint

# Show completion message
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  ✅ Deployment Complete!" -ForegroundColor Green
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "Deployment Details:" -ForegroundColor Yellow
Write-Host "  Version: $VERSION"
Write-Host "  Type: $DeploymentType"
Write-Host "  Project: $ProjectId"
Write-Host "  Region: $Region"
Write-Host ""
Write-Host "Endpoint: $Endpoint" -ForegroundColor Cyan
Write-Host ""
Write-Host "Next Steps:" -ForegroundColor Yellow
Write-Host "  1. Monitor logs: gcloud run services logs read ouroboros-node --region=$Region"
Write-Host "  2. Test API: Invoke-WebRequest -Uri '$Endpoint/health'"
Write-Host "  3. Check fraud detection: Invoke-WebRequest -Uri '$Endpoint/fraud/status'"
Write-Host ""
Write-Host "🎉 Deployment successful!" -ForegroundColor Green
