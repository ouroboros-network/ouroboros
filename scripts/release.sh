#!/usr/bin/env bash
# Build locally and upload binaries to a GitHub release.
# Usage: ./scripts/release.sh v1.5.2
#
# Prerequisites:
#   - Rust toolchain installed
#   - gh CLI authenticated (gh auth login)
#
# This builds for the current platform only. To upload binaries
# built on other machines, use:
#   gh release upload <tag> <file> --clobber

set -euo pipefail

TAG="${1:?Usage: release.sh <tag>  (e.g. v1.5.2)}"
REPO="ouroboros-network/ouroboros"

echo "=== Building release binary for $TAG ==="

cd "$(dirname "$0")/../ouro_dag"
cargo build --release --bin ouro

# Detect platform and name the binary accordingly
case "$(uname -s)-$(uname -m)" in
    Linux-x86_64)   BINARY_NAME="ouro-linux-x64" ;;
    Linux-aarch64)  BINARY_NAME="ouro-linux-arm64" ;;
    Darwin-x86_64)  BINARY_NAME="ouro-macos-x64" ;;
    Darwin-arm64)   BINARY_NAME="ouro-macos-arm64" ;;
    MINGW*|MSYS*)   BINARY_NAME="ouro-windows-x64.exe" ;;
    *)              echo "Unknown platform: $(uname -s)-$(uname -m)"; exit 1 ;;
esac

# Copy binary to dist location
DIST_DIR="$(dirname "$0")/../dist"
mkdir -p "$DIST_DIR"

if [[ "$BINARY_NAME" == *.exe ]]; then
    cp target/release/ouro.exe "$DIST_DIR/$BINARY_NAME"
else
    cp target/release/ouro "$DIST_DIR/$BINARY_NAME"
    chmod +x "$DIST_DIR/$BINARY_NAME"
fi

echo "=== Built: $DIST_DIR/$BINARY_NAME ==="

# Create release if it doesn't exist yet
if ! gh release view "$TAG" --repo "$REPO" &>/dev/null; then
    echo "=== Creating release $TAG ==="
    gh release create "$TAG" --repo "$REPO" --generate-notes
fi

# Upload binary
echo "=== Uploading $BINARY_NAME to $TAG ==="
gh release upload "$TAG" "$DIST_DIR/$BINARY_NAME" --repo "$REPO" --clobber

echo "=== Done! ==="
echo "Release: https://github.com/$REPO/releases/tag/$TAG"
echo ""
echo "To upload binaries from other platforms, run on each machine:"
echo "  gh release upload $TAG <binary-file> --repo $REPO --clobber"
