#!/bin/bash
# Fix file descriptor limits for RocksDB node

echo "Current file descriptor limit:"
ulimit -n

echo ""
echo "Setting temporary limit to 524288..."
ulimit -n 524288

echo "New limit: $(ulimit -n)"

echo ""
echo "Adding permanent system-wide limits..."
sudo tee -a /etc/security/limits.conf > /dev/null <<EOF

# Ouroboros RocksDB limits
* soft nofile 524288
* hard nofile 524288
root soft nofile 524288
root hard nofile 524288
EOF

echo ""
echo "Adding systemd service limits..."
sudo mkdir -p /etc/systemd/system/ouroboros.service.d
sudo tee /etc/systemd/system/ouroboros.service.d/limits.conf > /dev/null <<EOF
[Service]
LimitNOFILE=524288
EOF

echo ""
echo "Restarting node..."
sudo systemctl daemon-reload
sudo systemctl restart ouroboros

echo ""
echo "Done! Check node status:"
echo "  sudo systemctl status ouroboros"
echo "  sudo journalctl -u ouroboros -f"
