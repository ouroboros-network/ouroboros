@echo off
echo ==========================================
echo Remove Old PostgreSQL-based Deployment
echo ==========================================
echo.
echo This will DELETE the old instance: ouro-node-1
echo.
set /p CONFIRM="Type YES to confirm deletion: "

if /i not "%CONFIRM%"=="YES" (
    echo Cancelled.
    exit /b 1
)

SET PROJECT_ID=ultimate-flame-407206
SET ZONE=us-central1-a
SET OLD_INSTANCE=ouro-node-1

echo.
echo Setting project...
gcloud config set project %PROJECT_ID%

echo Stopping instance...
gcloud compute instances stop %OLD_INSTANCE% --zone=%ZONE%

echo Deleting instance...
gcloud compute instances delete %OLD_INSTANCE% --zone=%ZONE% --quiet

echo.
echo ==========================================
echo Old deployment removed successfully!
echo ==========================================
echo.
echo Remaining resources:
gcloud compute instances list
echo.
pause
