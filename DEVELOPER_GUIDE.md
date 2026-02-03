# Ouroboros Developer Guide - Complete Setup

**Version**: v0.4.2
**Last Updated**: 2025-12-28

---

## Table of Contents

1. [Prerequisites](#prerequisites)
2. [Development Environment Setup](#development-environment-setup)
3. [Running a Mainchain Validator Node](#running-a-mainchain-validator-node)
4. [Setting Up a Private Subchain](#setting-up-a-private-subchain)
5. [Smart Contract Development](#smart-contract-development)
6. [Testing & Debugging](#testing--debugging)
7. [Production Deployment](#production-deployment)
8. [Troubleshooting](#troubleshooting)

---

# Prerequisites

## System Requirements

### Minimum (Development)
- **CPU**: 2 cores
- **RAM**: 4GB
- **Storage**: 20GB SSD
- **OS**: Windows 10+, Ubuntu 20.04+, macOS 12+

### Recommended (Production Validator)
- **CPU**: 4+ cores
- **RAM**: 8GB+
- **Storage**: 100GB+ SSD
- **Network**: 100 Mbps+
- **OS**: Ubuntu 22.04 LTS

---

# Development Environment Setup

## Step 1: Install Rust

### Windows
```powershell
# Download and run rustup-init.exe from https://rustup.rs/

# Or use winget
winget install Rustlang.Rustup

# After installation, open new terminal and verify
rustc --version
cargo --version
```

### Linux/macOS
```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Add to PATH (add to ~/.bashrc or ~/.zshrc)
source $HOME/.cargo/env

# Verify installation
rustc --version
cargo --version
```

## Step 2: Install Dependencies

### Windows
```powershell
# Install Visual Studio Build Tools (if using MSVC) OR MinGW-w64 (GNU)

# Option A: GNU Toolchain (Recommended for Ouroboros)
# Download from: https://www.mingw-w64.org/downloads/
# Or use chocolatey
choco install mingw

# Set GNU as default
rustup default stable-x86_64-pc-windows-gnu

# Install PostgreSQL (optional)
choco install postgresql

# Install CMake (required for RocksDB)
choco install cmake

# Verify
cmake --version
```

### Ubuntu/Debian
```bash
# Update system
sudo apt update && sudo apt upgrade -y

# Install build essentials
sudo apt install -y build-essential pkg-config libssl-dev

# Install PostgreSQL (optional)
sudo apt install -y postgresql postgresql-contrib

# Install CMake
sudo apt install -y cmake

# Install additional dependencies
sudo apt install -y clang llvm

# Verify
cmake --version
gcc --version
```

### macOS
```bash
# Install Homebrew if not installed
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

# Install dependencies
brew install cmake postgresql openssl pkg-config

# Verify
cmake --version
```

## Step 3: Clone Repository

```bash
# Clone the repository
git clone https://github.com/your-org/ouroboros.git
cd ouroboros

# Or if already cloned, update
git pull origin main
```

## Step 4: Build Ouroboros

```bash
# Navigate to main directory
cd ouro_dag

# Build in development mode (faster compilation)
cargo build

# Or build in release mode (optimized, slower compilation)
cargo build --release

# This will take 10-30 minutes on first build
# Subsequent builds are much faster due to incremental compilation
```

**Expected output:**
```
   Compiling ouroboros v0.4.2
    Finished dev [unoptimized + debuginfo] target(s) in 15m 32s
```

## Step 5: Run Tests

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_transaction_validation

# Run tests with output
cargo test -- --nocapture

# Run tests in release mode (faster)
cargo test --release
```

---

# Running a Mainchain Validator Node

## Step 1: Generate Validator Keys

```bash
# Generate BFT validator keypair
cd ouro_dag
cargo run --release --bin keygen

# Output:
# Private Key: 5c2a3f7b1e8d9a4c6b8e2f1a9d7c5b3e8f1a2c4d6e8b9a1c3e5f7b2d4a6c8e1a
# Public Key: 8e1a3c5f7b2d4a6c8e1a3c5f7b2d4a6c8e1a3c5f7b2d4a6c8e1a3c5f7b2d4a6c

# SAVE THESE KEYS SECURELY!
```

## Step 2: Configure Environment

```bash
# Copy example environment file
cp .env.example .env

# Edit .env file
nano .env  # or vim, code, notepad, etc.
```

**Mainchain Validator Configuration (.env):**

```bash
# ============================================================================
# MAINCHAIN VALIDATOR NODE CONFIGURATION
# ============================================================================

# Node Identity
NODE_TYPE=validator
NODE_ID=validator-001
NODE_NUMBER=1  # Unique number for this validator

# Network
API_PORT=3030
P2P_PORT=9000
P2P_LISTEN_ADDR=0.0.0.0:9000

# Database (choose one)
STORAGE_MODE=full  # or 'postgres' for production
ROCKSDB_PATH=./mainchain_data

# PostgreSQL (if using STORAGE_MODE=postgres)
# DATABASE_URL=postgresql://postgres:password@localhost/ouroboros_mainchain

# BFT Consensus
BFT_PRIVATE_KEY=<your-private-key-from-step-1>
BFT_PUBLIC_KEY=<your-public-key-from-step-1>
BFT_VALIDATOR_STAKE=1000000000000  # 10,000 OURO (in smallest units)

# Initial Validators (comma-separated public keys)
BFT_INITIAL_VALIDATORS=<your-pubkey>,<validator2-pubkey>,<validator3-pubkey>

# Network Configuration
BOOTSTRAP_PEERS=/ip4/seed1.ouroboros.network/tcp/9000,/ip4/seed2.ouroboros.network/tcp/9000

# Security
P2P_MAX_MESSAGES_PER_WINDOW=600
P2P_RATE_LIMIT_WINDOW_SECS=60
P2P_MAX_CONNECTIONS_PER_IP=10

# Logging
RUST_LOG=info  # debug, info, warn, error

# Optional: TLS for HTTPS
# ENABLE_TLS=true
# TLS_CERT_PATH=/path/to/cert.pem
# TLS_KEY_PATH=/path/to/key.pem

# Optional: Tor for hybrid darkweb operation
# TOR_ENABLED=true
# TOR_SOCKS5_PROXY=127.0.0.1:9050
