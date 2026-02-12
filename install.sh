#!/bin/bash
# Ouroboros Node Installer
# Downloads precompiled binary from GitHub releases

set -e

# Configuration
REPO="ouroboros-network/ouroboros"
BINARY_NAME="ouro"
INSTALL_DIR="/usr/local/bin"
CONFIG_DIR="$HOME/.ouroboros"
VERSION="latest"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[0;33m'
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
        ARCH_TYPE="x64"
        ;;
    aarch64|arm64)
        ARCH_TYPE="arm64"
        ;;
    *)
        echo -e "${RED}Unsupported Architecture: $ARCH${NC}"
        exit 1
        ;;
esac

ASSET_NAME="${BINARY_NAME}-${OS_TYPE}-${ARCH_TYPE}"
echo -e "Platform detected: ${GREEN}${OS_TYPE}-${ARCH_TYPE}${NC}"

# 2. Resolve download URL
if [ "$VERSION" = "latest" ]; then
    DOWNLOAD_URL="https://github.com/${REPO}/releases/latest/download/${ASSET_NAME}"
else
    DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${VERSION}/${ASSET_NAME}"
fi

echo -e "Downloading ${BLUE}${ASSET_NAME}${NC} from GitHub releases..."

# 3. Download binary
TMP_DIR=$(mktemp -d)
TMP_BIN="${TMP_DIR}/${BINARY_NAME}"

if command -v curl >/dev/null 2>&1; then
    HTTP_CODE=$(curl -sL -w "%{http_code}" -o "$TMP_BIN" "$DOWNLOAD_URL")
    if [ "$HTTP_CODE" != "200" ]; then
        echo -e "${YELLOW}Download failed (HTTP $HTTP_CODE). Falling back to build from source...${NC}"
        rm -rf "$TMP_DIR"
        if command -v cargo >/dev/null 2>&1; then
            echo -e "Building from source using Cargo..."
            cargo build --release --bin ouro
            TMP_BIN="target/release/ouro"
        else
            echo -e "${RED}Error: Download failed and 'cargo' is not installed.${NC}"
            echo -e "Install Rust from https://rustup.rs/ or check the release page:"
            echo -e "  https://github.com/${REPO}/releases/latest"
            exit 1
        fi
    else
        echo -e "${GREEN}Download complete.${NC}"
    fi
elif command -v wget >/dev/null 2>&1; then
    if ! wget -q -O "$TMP_BIN" "$DOWNLOAD_URL" 2>/dev/null; then
        echo -e "${YELLOW}Download failed. Falling back to build from source...${NC}"
        rm -rf "$TMP_DIR"
        if command -v cargo >/dev/null 2>&1; then
            echo -e "Building from source using Cargo..."
            cargo build --release --bin ouro
            TMP_BIN="target/release/ouro"
        else
            echo -e "${RED}Error: Download failed and 'cargo' is not installed.${NC}"
            exit 1
        fi
    else
        echo -e "${GREEN}Download complete.${NC}"
    fi
else
    echo -e "${RED}Error: Neither curl nor wget found.${NC}"
    exit 1
fi

# 4. Install Binary
chmod +x "$TMP_BIN"
echo -e "Installing to ${INSTALL_DIR}..."
if [ -w "$INSTALL_DIR" ]; then
    mv "$TMP_BIN" "${INSTALL_DIR}/${BINARY_NAME}"
else
    echo -e "Requesting sudo permissions to install to ${INSTALL_DIR}..."
    sudo mv "$TMP_BIN" "${INSTALL_DIR}/${BINARY_NAME}"
fi

# Cleanup temp dir if it still exists
[ -d "$TMP_DIR" ] && rm -rf "$TMP_DIR"

# 5. Initialize Configuration
echo -e "Initializing configuration in ${CONFIG_DIR}..."
mkdir -p "$CONFIG_DIR"

if [ ! -f "${CONFIG_DIR}/config.json" ]; then
    "${INSTALL_DIR}/${BINARY_NAME}" register-node > /dev/null 2>&1 || true
    echo -e "Default configuration created."
fi

# 6. Verify installation
if command -v "$BINARY_NAME" >/dev/null 2>&1; then
    INSTALLED_VERSION=$("$BINARY_NAME" --version 2>/dev/null || echo "unknown")
    echo -e ""
    echo -e "${GREEN}=== Installation Complete! ===${NC}"
    echo -e "Version: ${INSTALLED_VERSION}"
else
    echo -e ""
    echo -e "${GREEN}=== Installation Complete! ===${NC}"
fi

echo -e ""
echo -e "Start your node:"
echo -e "  ${BLUE}ouro start${NC}"
echo -e ""
echo -e "See all commands:"
echo -e "  ${BLUE}ouro --help${NC}"
