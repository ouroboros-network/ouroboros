@echo off
echo ==========================================
echo Ouroboros Node Deployment to GCP
echo ==========================================
echo.

SET PROJECT_ID=ultimate-flame-407206
SET ZONE=us-central1-a
SET INSTANCE_NAME=ouro-node-rocksdb
SET MACHINE_TYPE=e2-medium

REM Set project
gcloud config set project %PROJECT_ID%

REM Create persistent disk for data
echo Creating persistent disk for blockchain data...
gcloud compute disks create %INSTANCE_NAME%-data --size=50GB --zone=%ZONE% --type=pd-standard 2>nul
if errorlevel 1 echo Disk may already exist, continuing...

REM Create firewall rules
echo Setting up firewall rules...
gcloud compute firewall-rules create ouro-p2p --allow=tcp:9000 --description="Ouroboros P2P port" --direction=INGRESS 2>nul
gcloud compute firewall-rules create ouro-api --allow=tcp:8000 --description="Ouroboros API port" --direction=INGRESS 2>nul

REM Create startup script that downloads prebuilt binary
echo Creating startup script...
(
echo #!/bin/bash
echo set -e
echo apt-get update
echo apt-get install -y curl ca-certificates python3 python3-venv
echo mkdir -p /mnt/disks/data
echo DEVICE=/dev/disk/by-id/google-%INSTANCE_NAME%-data
echo if [ -e "$DEVICE" ]; then
echo   if ! blkid $DEVICE; then mkfs.ext4 -F $DEVICE; fi
echo   mount -o discard,defaults $DEVICE /mnt/disks/data
echo   echo "$DEVICE /mnt/disks/data ext4 discard,defaults,nofail 0 2" ^>^> /etc/fstab
echo fi
echo REPO="ouroboros-network/ouroboros"
echo curl -fsSL "https://github.com/$REPO/releases/latest/download/ouro-linux-x64" -o /usr/local/bin/ouro
echo chmod +x /usr/local/bin/ouro
echo /usr/local/bin/ouro register-node ^>/dev/null 2^>^&1 ^|^| true
echo cat ^> /etc/systemd/system/ouroboros.service ^<^< 'SVCEOF'
echo [Unit]
echo Description=Ouroboros Blockchain Node
echo After=network.target
echo [Service]
echo Type=simple
echo Environment="DATABASE_PATH=/mnt/disks/data/rocksdb"
echo Environment="RUST_LOG=info"
echo ExecStart=/usr/local/bin/ouro start
echo Restart=always
echo RestartSec=10
echo [Install]
echo WantedBy=multi-user.target
echo SVCEOF
echo systemctl daemon-reload
echo systemctl enable ouroboros
echo systemctl start ouroboros
echo echo "Node started!"
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
echo To check status:
echo   gcloud compute ssh %INSTANCE_NAME% --zone=%ZONE% --command="journalctl -u ouroboros --no-pager -n 50"
echo.
pause
