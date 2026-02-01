#!/bin/bash
set -e

echo "=== Installing Prometheus & Grafana ==="

# Install Prometheus
echo "Installing Prometheus..."
cd /tmp
wget -q https://github.com/prometheus/prometheus/releases/download/v2.48.1/prometheus-2.48.1.linux-amd64.tar.gz
tar xzf prometheus-2.48.1.linux-amd64.tar.gz
sudo mv prometheus-2.48.1.linux-amd64 /opt/prometheus
sudo useradd --no-create-home --shell /bin/false prometheus || true

# Create Prometheus config
sudo tee /opt/prometheus/prometheus.yml > /dev/null <<'EOF'
global:
  scrape_interval: 15s
  evaluation_interval: 15s

scrape_configs:
  - job_name: 'ouroboros-node'
    static_configs:
      - targets: ['localhost:8000']
    metrics_path: '/metrics'
    scrape_interval: 5s
EOF

# Create Prometheus systemd service
sudo tee /etc/systemd/system/prometheus.service > /dev/null <<'EOF'
[Unit]
Description=Prometheus
Wants=network-online.target
After=network-online.target

[Service]
User=prometheus
Group=prometheus
Type=simple
ExecStart=/opt/prometheus/prometheus \
  --config.file=/opt/prometheus/prometheus.yml \
  --storage.tsdb.path=/opt/prometheus/data \
  --web.listen-address=0.0.0.0:9090

[Install]
WantedBy=multi-user.target
EOF

sudo mkdir -p /opt/prometheus/data
sudo chown -R prometheus:prometheus /opt/prometheus

# Install Grafana
echo "Installing Grafana..."
sudo apt-get install -y apt-transport-https software-properties-common
wget -q -O - https://packages.grafana.com/gpg.key | sudo apt-key add -
echo "deb https://packages.grafana.com/oss/deb stable main" | sudo tee /etc/apt/sources.list.d/grafana.list
sudo apt-get update
sudo apt-get install -y grafana

# Start services
echo "Starting services..."
sudo systemctl daemon-reload
sudo systemctl enable prometheus
sudo systemctl start prometheus
sudo systemctl enable grafana-server
sudo systemctl start grafana-server

echo ""
echo "=== Installation Complete! ==="
echo "Prometheus: http://$(curl -s ifconfig.me):9090"
echo "Grafana: http://$(curl -s ifconfig.me):3000"
echo "  Default login: admin/admin"
echo ""
echo "Node metrics: http://localhost:8000/metrics"
