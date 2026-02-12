@echo off
echo Cleaning up unnecessary files...

REM Remove session summaries and status reports
del /Q FINAL_SESSION_SUMMARY.md 2>nul
del /Q FINAL_PROJECT_STATUS.md 2>nul
del /Q PROJECT_STATUS.md 2>nul
del /Q CODE_VERIFICATION_SUMMARY.md 2>nul
del /Q EMOJI_REMOVAL_SUMMARY.md 2>nul
del /Q FRAUD_SYSTEM_IMPLEMENTATION_SUMMARY.md 2>nul
del /Q OPTION1_CONTRACT_TESTING_SUMMARY.md 2>nul
del /Q OPTION2_INTEGRATION_GUIDE.md 2>nul
del /Q OPTION5_GOVERNANCE_SUMMARY.md 2>nul

REM Remove release notes and deployment docs
del /Q RELEASE_NOTES_v0.2.1.md 2>nul
del /Q DEPLOYMENT_READY_v0.4.0.md 2>nul
del /Q SECURITY_AUDIT_COMPLETE_v0.4.1.md 2>nul
del /Q SECURITY_FIXES_COMPLETE.md 2>nul
del /Q SECURITY_FIXES_COMPLETE_v0.4.2.md 2>nul
del /Q SECURITY_FIXES_v0.4.1.md 2>nul
del /Q SECURITY_IMPROVEMENTS_SUMMARY.md 2>nul
del /Q AWS_DEPENDENCY_OPTIMIZATION.md 2>nul
del /Q DOCKER_DEPLOYMENT_PLAN.md 2>nul
del /Q GCP_DEPLOYMENT_V0.4.0.md 2>nul
del /Q GCP_FREE_TIER_DEPLOYMENT.md 2>nul
del /Q QUICK_DEPLOY_GUIDE.md 2>nul
del /Q TOR_DEPLOYMENT_GUIDE.md 2>nul

REM Remove fix notes
del /Q NEXT_STEPS.md 2>nul
del /Q PEER_CONNECTION_FIX.md 2>nul
del /Q PEER_CONNECTION_FIX_TEST_REPORT.md 2>nul

REM Remove benchmark scripts and results
del /Q benchmark_simple.py 2>nul
del /Q benchmark_tps.py 2>nul
del /Q benchmark_results.txt 2>nul
del /Q benchmark_final_results.txt 2>nul
del /Q test_balance_tracking.py 2>nul
del /Q test_single_tx.py 2>nul

REM Remove conversion/fix scripts
del /Q convert_to_rocksdb.py 2>nul
del /Q remove_emojis.py 2>nul
del /Q remove_emojis.sh 2>nul
del /Q fix_lib_rs.py 2>nul
del /Q cleanup_orphaned_sql.py 2>nul

REM Remove test scripts
del /Q test_v0.2.1.ps1 2>nul
del /Q test_v0.2.1.sh 2>nul

REM Remove old run scripts
del /Q run_node1.sh 2>nul
del /Q run_node2.sh 2>nul
del /Q run_node3.sh 2>nul
del /Q start_multi_node.sh 2>nul

REM Remove log files
del /Q node1_bench.log 2>nul
del /Q node1_fresh.log 2>nul
del /Q nul 2>nul
del /Q ouro_dag\test_output.log 2>nul
del /Q ouro_dag\node.log 2>nul

REM Remove tar archives
del /Q ouroboros.tar.gz 2>nul
del /Q ouroboros-fixed.tar.gz 2>nul

REM Remove test data directories
rmdir /S /Q ouro_dag\test_data 2>nul
rmdir /S /Q ouro_dag\test_node_data 2>nul
rmdir /S /Q ouro_dag\node1_data 2>nul
rmdir /S /Q ouro_dag\node2_data 2>nul
rmdir /S /Q ouro_dag\node3_data 2>nul
rmdir /S /Q ouro_dag\fresh_test_node 2>nul
rmdir /S /Q sled_data 2>nul
rmdir /S /Q ouro_dag\sled_data 2>nul

REM Clean build artifacts
cd ouro_dag
cargo clean
cd ..

echo Cleanup complete!
echo.
echo Kept important files:
echo - README.md
echo - CHANGELOG.md
echo - API_DOCUMENTATION.md
echo - Developer guides
echo - License and configuration files
