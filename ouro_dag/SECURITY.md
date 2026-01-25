# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 1.0.x   | Yes                |
| < 1.0   | No                 |

## Reporting a Vulnerability

If you discover a security vulnerability, please report it responsibly:

1. Do NOT create a public GitHub issue
2. Email security details to: security@ouro.network
3. Include:
   - Description of the vulnerability
   - Steps to reproduce
   - Potential impact
   - Suggested fix (if any)

We will respond within 48 hours and work with you to address the issue.

## Security Measures

### Cryptographic Security

- Ed25519 signatures for transaction signing
- Dilithium post-quantum signatures (hybrid mode available)
- SHA-256 and Blake2b for hashing
- Groth16 ZK-SNARKs for privacy features

### Network Security

- Rate limiting on all API endpoints
- Peer allowlist support for validator networks
- TLS 1.3 required for production
- DDoS protection via configurable rate limits

### Consensus Security

- HotStuff BFT with 2/3+1 Byzantine fault tolerance
- Slashing for double-signing and downtime
- VRF-based leader selection

### Smart Contract Security

- Gas metering prevents infinite loops
- Memory limits prevent resource exhaustion
- Deterministic execution

## Security Audit Summary

Last audit: January 2026

### Findings Addressed

1. Fixed: Potential panic in account abstraction execution
2. Fixed: Unsafe unwrap in hybrid signature verification
3. Verified: Input validation on all API endpoints
4. Verified: Integer overflow protection with checked arithmetic
5. Verified: Rate limiting implementation

### Best Practices

- No unsafe Rust blocks in core code
- Comprehensive input validation
- Proper error handling without information leakage
- Cryptographic operations use audited libraries
