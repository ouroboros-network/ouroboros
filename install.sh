#!/bin/bash
# Ouroboros Node Installer
# Downloads precompiled binary from GitHub releases

set -e

# Configuration
REPO="ouroboros-network/ouroboros"
BINARY_NAME="ouro"
INSTALL_DIR="/usr/local/bin"
CONFIG_DIR="$HOME/.ouroboros"
SEEDS="136.112.101.176:9000,34.57.121.217:9000"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[0;33m'
DIM='\033[2m'
NC='\033[0m'

echo ""
echo -e "${BLUE}=========================================="
echo -e "  Ouroboros Network - Quick Install"
echo -e "==========================================${NC}"
echo ""

# 1. Detect OS and Architecture
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Linux)  OS_TYPE="linux" ;;
    Darwin) OS_TYPE="macos" ;;
    *)
        echo -e "${RED}Unsupported OS: $OS${NC}"
        exit 1
        ;;
esac

case "$ARCH" in
    x86_64)       ARCH_TYPE="x64" ;;
    aarch64|arm64) ARCH_TYPE="arm64" ;;
    *)
        echo -e "${RED}Unsupported Architecture: $ARCH${NC}"
        exit 1
        ;;
esac

ASSET_NAME="${BINARY_NAME}-${OS_TYPE}-${ARCH_TYPE}"
echo -e "[1/5] ${GREEN}Detected:${NC} ${OS_TYPE}-${ARCH_TYPE}"

# 2. Check for existing installation
CURRENT_VERSION=""
if command -v "$BINARY_NAME" >/dev/null 2>&1; then
    CURRENT_VERSION=$("$BINARY_NAME" --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' || echo "")
    if [ -n "$CURRENT_VERSION" ]; then
        echo -e "      Found existing: v${CURRENT_VERSION}"
    fi
fi

# 3. Get latest release version
echo -e "[2/5] ${GREEN}Checking latest release...${NC}"
LATEST_TAG=$(curl -sI "https://github.com/${REPO}/releases/latest" | grep -i "^location:" | grep -oE 'v[0-9]+\.[0-9]+\.[0-9]+' || echo "")
if [ -n "$LATEST_TAG" ]; then
    LATEST_VERSION="${LATEST_TAG#v}"
    echo -e "      Latest: v${LATEST_VERSION}"

    if [ "$CURRENT_VERSION" = "$LATEST_VERSION" ]; then
        echo -e "      ${GREEN}Already up to date!${NC}"
    elif [ -n "$CURRENT_VERSION" ]; then
        echo -e "      Upgrading v${CURRENT_VERSION} -> v${LATEST_VERSION}"
    fi
fi

# 4. Download binary
echo -e "[3/5] ${GREEN}Downloading ${ASSET_NAME}...${NC}"
DOWNLOAD_URL="https://github.com/${REPO}/releases/latest/download/${ASSET_NAME}"

TMP_DIR=$(mktemp -d)
TMP_BIN="${TMP_DIR}/${BINARY_NAME}"

if command -v curl >/dev/null 2>&1; then
    HTTP_CODE=$(curl -sL -w "%{http_code}" -o "$TMP_BIN" "$DOWNLOAD_URL")
    if [ "$HTTP_CODE" != "200" ]; then
        echo -e "${RED}Download failed (HTTP $HTTP_CODE).${NC}"
        echo -e "Check releases: https://github.com/${REPO}/releases/latest"
        rm -rf "$TMP_DIR"
        exit 1
    fi
elif command -v wget >/dev/null 2>&1; then
    if ! wget -q -O "$TMP_BIN" "$DOWNLOAD_URL" 2>/dev/null; then
        echo -e "${RED}Download failed.${NC}"
        rm -rf "$TMP_DIR"
        exit 1
    fi
else
    echo -e "${RED}Error: Neither curl nor wget found.${NC}"
    exit 1
fi

SIZE=$(du -h "$TMP_BIN" | cut -f1)
echo -e "      Downloaded successfully (${SIZE})"

# 5. Install Binary
echo -e "[4/5] ${GREEN}Installing...${NC}"
chmod +x "$TMP_BIN"

if [ -w "$INSTALL_DIR" ]; then
    mv "$TMP_BIN" "${INSTALL_DIR}/${BINARY_NAME}"
else
    sudo mv "$TMP_BIN" "${INSTALL_DIR}/${BINARY_NAME}"
fi
[ -d "$TMP_DIR" ] && rm -rf "$TMP_DIR"

# 5b. Download Python tier files (for Medium/Light roles)
PY_DIR="${CONFIG_DIR}/ouro_py"
RAW_BASE="https://raw.githubusercontent.com/${REPO}/main"
PY_FILES="ouro_py/requirements.txt ouro_py/ouro_medium/main.py ouro_py/ouro_light/main.py"

echo -e "      Downloading Python tier files..."
for f in $PY_FILES; do
    LOCAL_PATH="${CONFIG_DIR}/${f}"
    mkdir -p "$(dirname "$LOCAL_PATH")"
    curl -sL -o "$LOCAL_PATH" "${RAW_BASE}/${f}" 2>/dev/null || true
done
echo -e "      Python tier files installed."

# 6. Initialize Configuration
echo -e "[5/5] ${GREEN}Configuring node...${NC}"
mkdir -p "$CONFIG_DIR"

if [ ! -f "${CONFIG_DIR}/config.json" ]; then
    "${INSTALL_DIR}/${BINARY_NAME}" register-node > /dev/null 2>&1 || true
    echo -e "      Created default configuration."
else
    echo -e "      Using existing configuration."
fi

# 7. Verify and print summary
INSTALLED_VERSION=$("${INSTALL_DIR}/${BINARY_NAME}" --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' || echo "unknown")

echo ""
echo -e "${GREEN}=========================================="
echo -e "  Installation Complete!"
echo -e "==========================================${NC}"
echo ""
echo -e "  Version:  ${GREEN}v${INSTALLED_VERSION}${NC}"
echo -e "  Binary:   ${INSTALL_DIR}/${BINARY_NAME}"
echo -e "  Config:   ${CONFIG_DIR}/"
echo -e "  Seeds:    ${DIM}${SEEDS}${NC}"
echo ""
echo -e "  ${BLUE}Start your node:${NC}"
echo -e "    ouro start                    ${DIM}# Heavy node (default)${NC}"
echo -e "    ouro start --role medium      ${DIM}# Subchain aggregator${NC}"
echo -e "    ouro start --role light       ${DIM}# App node / watchdog${NC}"
echo ""
echo -e "  ${BLUE}Useful commands:${NC}"
echo -e "    ouro status                   ${DIM}# Live dashboard${NC}"
echo -e "    ouro roles                    ${DIM}# Tier details${NC}"
echo -e "    ouro benchmark                ${DIM}# Hardware benchmark${NC}"
echo -e "    ouro --help                   ${DIM}# All commands${NC}"
echo ""
