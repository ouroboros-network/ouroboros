# Changelog

All notable changes to the Ouroboros project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.1] - 2024-12-16

### Fixed
- **Critical**: Fixed peer connection issue where lightweight nodes could not establish ESTABLISHED connections
  - Root cause: `inbound_rx` channel was never consumed, causing backpressure
  - Solution: Added spawned task to continuously process inbound transactions
  - Impact: Lightweight nodes can now successfully join and participate in the P2P network
  - Locations: `ouro_dag/src/lib.rs` lines 633-642

### Added
- Enhanced debug logging for peer discovery and connection status
- Comprehensive test documentation for peer connection fix

### Documentation
- Added `PEER_CONNECTION_FIX_TEST_REPORT.md` with detailed test procedures
- Added `RELEASE_NOTES_v0.2.1.md` with full release information

## [0.2.0] - 2024-12-10

### Added
- PostgreSQL integration for full nodes
- RocksDB support for lightweight nodes
- BFT consensus implementation
- Microchain support (experimental)
- Subchain aggregation
- DAG-based mainchain
- Tor integration for privacy
- Peer discovery system
- Transaction propagation
- RESTful API endpoints

### Features
- Join command for lightweight nodes
- Multi-node support
- Docker deployment
- Kubernetes configurations

### Documentation
- Initial README and deployment guides
- Network information and testing guides

## [0.1.0] - 2024-11-15

### Added
- Initial project structure
- Basic blockchain infrastructure
- Core cryptographic primitives
- Network layer foundation

---

## Release Types

- **Major** (x.0.0): Breaking changes, major new features
- **Minor** (0.x.0): New features, backward compatible
- **Patch** (0.0.x): Bug fixes, no new features

## Categories

- **Added**: New features
- **Changed**: Changes in existing functionality
- **Deprecated**: Soon-to-be removed features
- **Removed**: Removed features
- **Fixed**: Bug fixes
- **Security**: Security fixes
