# Changelog

All notable changes to the Ouroboros project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.4.1] - 2026-02-15

### Fixed
- **Critical**: Config migration crash when upgrading from v1.3.x to v1.4.0
  - Existing config files missing the `role` field caused deserialization failure
  - `role` now defaults to `heavy` when not present in config
  - Seamless upgrade path from any prior version

## [1.4.0] - 2026-02-14

### Added
- **Three-tier node architecture**: Heavy (Rust), Medium (Python), Light (Python)
  - Heavy nodes: BFT consensus, global finality, full DAG
  - Medium nodes: Subchain aggregation, batch ordering, shadow consensus
  - Light nodes: App microchains, anchor verification, fraud detection
- **Role-aware P2P handshake**: Nodes exchange role during Hello/Challenge messages
- **Diversity-aware peer pruning**: Maintains minimum peers per tier for network health
- **Subchain market**: Medium nodes advertise capacity, Light nodes auto-discover aggregators
- **ZK state proofs**: SHA-256 commitment chain for light node sync without full replay
- **Post-quantum hybrid signatures**: Ed25519 + Dilithium5 (Phase 2)
- **Adaptive difficulty system**: small/medium/large/extra_large tiers with reward multipliers
- **On-chain governance**: Proposal submission, voting, quorum-based approval
- **Native contracts**: Token transfers, staking, cross-chain bridge, governance voting
- **Fraud detection system**: Rate limit tracking, auth failure monitoring, auto-blocking
- **OVM (Ouroboros Virtual Machine)**: WASM smart contract execution with gas metering
- **Python SDK**: `ouro-sdk` package for Python integration
- `ouro benchmark` command for hardware capability testing
- `ouro account new/balance` and `ouro tx send` CLI commands
- `ouro roles` command showing tier details and requirements
- Liveness timer for BFT dead leader detection
- Bearer token auth middleware on Python tier nodes
- Executable-relative path resolution for Python handoff

### Changed
- Reward multipliers now tied to difficulty tier (1x/2x/4x/8x)
- Tier-based reward scaling: Heavy 1.0x, Medium 0.5x, Light 0.1x
- Install scripts download prebuilt binaries (no build-from-source required)

### Removed
- PostgreSQL dependency (replaced with embedded RocksDB)
- "Zombie Mode" from NodeStatus enum

### Security
- Eliminated `%TEMP%` usage from all install scripts
- API keys required for all state-changing endpoints
- Post-quantum cryptography support (Dilithium5, Kyber1024)

## [1.3.3] - 2026-02-12

### Fixed
- Version display bug in join script
- Install script failing silently when node cannot be stopped
- PowerShell `$PID` conflict and `taskkill` path in install script
- Graceful API shutdown for install script upgrades
- `ouro` command using stale cargo bin instead of installed binary

## [1.3.0] - 2026-02-10

### Added
- **P2P discovery**: Automatic peer discovery with DNS seed resolution
- **DNS seeds**: Bootstrap node discovery via DNS
- **Exponential backoff**: Retry logic for peer connections
- **Rust SDK**: `ouro_sdk` crate for Rust integration
- **JavaScript/TypeScript SDK**: `ouro-sdk` npm package
- Docker deployment support
- GCP deployment scripts

### Changed
- Improved peer connection stability
- Enhanced network layer with connection pooling

## [1.0.0] - 2026-01-15

### Added
- BFT consensus implementation (HotStuff-based)
- DAG-based mainchain with transaction propagation
- Microchain support (experimental)
- Subchain aggregation with rent system
- RocksDB embedded storage
- RESTful API with auth middleware
- P2P networking with TLS
- Tor integration for privacy
- CLI dashboard (`ouro status`)
- Node registration and identity management
- Cross-chain bridge support (lock/mint)
- Staking and slashing mechanisms
- Prometheus metrics export

## [0.2.1] - 2024-12-16

### Fixed
- **Critical**: Fixed peer connection issue where lightweight nodes could not establish ESTABLISHED connections
  - Root cause: `inbound_rx` channel was never consumed, causing backpressure
  - Solution: Added spawned task to continuously process inbound transactions

## [0.2.0] - 2024-12-10

### Added
- Initial blockchain infrastructure
- Basic cryptographic primitives (Ed25519)
- Network layer foundation
- Peer discovery system
- Transaction propagation
- RESTful API endpoints

---

## Release Types

- **Major** (x.0.0): Breaking changes, major new features
- **Minor** (0.x.0): New features, backward compatible
- **Patch** (0.0.x): Bug fixes, no new features
