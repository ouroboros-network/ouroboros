#!/bin/bash
# Ouroboros Node Installer
# Inspired by Nexus CLI installation script

set -e

# Configuration
REPO_URL="https://github.com/your-username/ouroboros" # TODO: Update with actual repo
BINARY_NAME="ouro"
INSTALL_DIR="/usr/local/bin"
CONFIG_DIR="$HOME/.ouroboros"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}=== Ouroboros Node Installer ===${NC}"

# 1. Detect OS and Architecture
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Linux)
        OS_TYPE="linux"
        ;;
    Darwin)
        OS_TYPE="macos"
        ;;
    *)
        echo -e "${RED}Unsupported OS: $OS${NC}"
        exit 1
        ;;
esac

case "$ARCH" in
    x86_64)
        ARCH_TYPE="amd64"
        ;;
    aarch64|arm64)
        ARCH_TYPE="arm64"
        ;;
    *)
        echo -e "${RED}Unsupported Architecture: $ARCH${NC}"
        exit 1
        ;;
esac

echo -e "Platform detected: ${GREEN}$OS_TYPE ($ARCH_TYPE)${NC}"

# 2. Download Binary (Placeholder logic - since we don't have a release server yet)
# In a real scenario, this would point to a GitHub release or CDN
# DOWNLOAD_URL="${REPO_URL}/releases/latest/download/${BINARY_NAME}-${OS_TYPE}-${ARCH_TYPE}"

echo -e "Downloading Ouroboros binary..."
# curl -sSL "$DOWNLOAD_URL" -o "$BINARY_NAME"

# FOR NOW: Since we are in development, we inform the user to build from source
# OR if this were a real production script, it would proceed with the download.
echo -e "${BLUE}NOTE: Precompiled binaries are currently being prepared.${NC}"
echo -e "To complete the installation, the installer will attempt to build from source if 'cargo' is present."

if command -v cargo >/dev/null 2>&1; then
    echo -e "Building from source using Cargo..."
    cargo build --release --bin ouro
    cp target/release/ouro .
else
    echo -e "${RED}Error: Precompiled binary not found and 'cargo' is not installed.${NC}"
    exit 1
fi

# 3. Install Binary
echo -e "Installing to ${INSTALL_DIR}..."
if [ -w "$INSTALL_DIR" ]; then
    mv "$BINARY_NAME" "${INSTALL_DIR}/${BINARY_NAME}"
else
    echo -e "Requesting sudo permissions to install to ${INSTALL_DIR}..."
    sudo mv "$BINARY_NAME" "${INSTALL_DIR}/${BINARY_NAME}"
fi
chmod +x "${INSTALL_DIR}/${BINARY_NAME}"

# 4. Initialize Configuration
echo -e "Initializing configuration in ${CONFIG_DIR}..."
mkdir -p "$CONFIG_DIR"

if [ ! -f "${CONFIG_DIR}/config.json" ]; then
    # Generate a default config or let the binary do it on first run
    "${INSTALL_DIR}/${BINARY_NAME}" register-node > /dev/null 2>&1
    echo -e "Default configuration created."
fi

echo -e "${GREEN}=== Installation Complete! ===${NC}"
echo -e "You can now start your node by running:"
echo -e "  ${BLUE}ouro start${NC}"
echo -e ""
echo -e "To see available commands:"
echo -e "  ${BLUE}ouro --help${NC}"
