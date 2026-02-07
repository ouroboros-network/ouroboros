@echo off
echo ==========================================
echo Ouroboros RocksDB Deployment to GCP
echo ==========================================
echo.

SET PROJECT_ID=ultimate-flame-407206
SET ZONE=us-central1-a
SET INSTANCE_NAME=ouro-node-rocksdb
SET MACHINE_TYPE=e2-medium
SET OLD_INSTANCE=ouro-node-1

REM Set project
gcloud config set project %PROJECT_ID%

REM Create persistent disk for RocksDB data
echo Creating persistent disk for blockchain data...
gcloud compute disks create %INSTANCE_NAME%-data --size=50GB --zone=%ZONE% --type=pd-standard 2>nul
if errorlevel 1 echo Disk may already exist, continuing...

REM Create firewall rules
echo Setting up firewall rules...
gcloud compute firewall-rules create ouro-p2p --allow=tcp:9000 --description="Ouroboros P2P port" --direction=INGRESS 2>nul
gcloud compute firewall-rules create ouro-api --allow=tcp:8000 --description="Ouroboros API port" --direction=INGRESS 2>nul

REM Create startup script
echo Creating startup script...
(
echo #!/bin/bash
echo set -e
echo apt-get update
echo apt-get install -y docker.io git curl
echo systemctl start docker
echo systemctl enable docker
echo mkdir -p /mnt/disks/data
echo DEVICE=/dev/disk/by-id/google-%INSTANCE_NAME%-data
echo if [ -e "$DEVICE" ]; then
echo   if ! blkid $DEVICE; then mkfs.ext4 -F $DEVICE; fi
echo   mount -o discard,defaults $DEVICE /mnt/disks/data
echo   echo "$DEVICE /mnt/disks/data ext4 discard,defaults,nofail 0 2" ^>^> /etc/fstab
echo fi
echo cd /opt
echo git clone https://github.com/ipswyworld/ouroboros.git ^|^| true
echo cd ouroboros ^&^& git pull
echo cd ouro_dag
echo docker build -t ouroboros-node .
echo docker stop ouro-node 2^>/dev/null ^|^| true
echo docker rm ouro-node 2^>/dev/null ^|^| true
echo docker run -d --name ouro-node --restart unless-stopped -p 8000:8000 -p 9000:9000 -v /mnt/disks/data:/data -e ROCKSDB_PATH=/data/rocksdb -e RUST_LOG=info ouroboros-node
echo echo "Node started!"
echo docker logs ouro-node
) > %USERPROFILE%\.ouroboros\startup-script.sh

REM Create instance
echo Creating compute instance...
gcloud compute instances create %INSTANCE_NAME% ^
    --zone=%ZONE% ^
    --machine-type=%MACHINE_TYPE% ^
    --image-family=debian-12 ^
    --image-project=debian-cloud ^
    --boot-disk-size=30GB ^
    --boot-disk-type=pd-standard ^
    --disk=name=%INSTANCE_NAME%-data,mode=rw ^
    --metadata-from-file=startup-script=%USERPROFILE%\.ouroboros\startup-script.sh ^
    --tags=ouro-node ^
    --scopes=cloud-platform

REM Get IP
echo.
echo Getting external IP...
for /f "tokens=*" %%i in ('gcloud compute instances describe %INSTANCE_NAME% --zone=%ZONE% --format="get(networkInterfaces[0].accessConfigs[0].natIP)"') do set EXTERNAL_IP=%%i

echo.
echo ==========================================
echo Deployment Complete!
echo ==========================================
echo Instance: %INSTANCE_NAME%
echo External IP: %EXTERNAL_IP%
echo API: http://%EXTERNAL_IP%:8000
echo P2P: %EXTERNAL_IP%:9000
echo.
echo The node is starting up... (may take 2-3 minutes)
echo.
echo To check status:
echo   gcloud compute ssh %INSTANCE_NAME% --zone=%ZONE% --command="docker logs ouro-node"
echo.
pause
