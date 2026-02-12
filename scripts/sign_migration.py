#!/usr/bin/env python3
"""
Migration Signing Utility

Signs SQL migration files with Ed25519 signatures.

Usage:
    python sign_migration.py <migration_file> <private_key_hex>

Example:
    python sign_migration.py migrations/001_create_users.sql $(cat migration_signing.key)

Generates:
    migrations/001_create_users.sql.sig (64-byte signature file)
"""

import sys
import hashlib
from pathlib import Path

try:
    from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
    from cryptography.hazmat.primitives import serialization
except ImportError:
    print("ERROR: cryptography library not installed")
    print("Install with: pip install cryptography")
    sys.exit(1)


def sign_migration(migration_path: str, private_key_hex: str) -> bytes:
    """
    Sign a migration file with Ed25519.

    Args:
        migration_path: Path to the SQL migration file
        private_key_hex: Private key as hex string (64 characters = 32 bytes)

    Returns:
        64-byte signature
    """
    # Read migration content
    migration_file = Path(migration_path)
    if not migration_file.exists():
        raise FileNotFoundError(f"Migration file not found: {migration_path}")

    content = migration_file.read_bytes()

    # Hash the content (SHA-256)
    content_hash = hashlib.sha256(content).digest()

    # Parse private key
    try:
        private_key_bytes = bytes.fromhex(private_key_hex.strip())
    except ValueError:
        raise ValueError("Private key must be a valid hex string")

    if len(private_key_bytes) != 32:
        raise ValueError(f"Private key must be 32 bytes, got {len(private_key_bytes)}")

    # Create Ed25519 private key
    private_key = Ed25519PrivateKey.from_private_bytes(private_key_bytes)

    # Sign the hash
    signature = private_key.sign(content_hash)

    print(f"✅ Signed migration: {migration_path}")
    print(f"   Content hash: {content_hash.hex()[:16]}...")
    print(f"   Signature: {signature.hex()[:16]}...")

    return signature


def generate_keypair():
    """Generate a new Ed25519 keypair for migration signing."""
    private_key = Ed25519PrivateKey.generate()
    public_key = private_key.public_key()

    # Export keys
    private_bytes = private_key.private_bytes(
        encoding=serialization.Encoding.Raw,
        format=serialization.PrivateFormat.Raw,
        encryption_algorithm=serialization.NoEncryption()
    )

    public_bytes = public_key.public_bytes(
        encoding=serialization.Encoding.Raw,
        format=serialization.PublicFormat.Raw
    )

    print("=== NEW MIGRATION SIGNING KEYPAIR ===")
    print()
    print("Private Key (KEEP SECRET!):")
    print(private_bytes.hex())
    print()
    print("Public Key (embed in binary):")
    print(public_bytes.hex())
    print()
    print("⚠️  SECURITY WARNING:")
    print("   - Store the private key securely (offline, encrypted)")
    print("   - Never commit the private key to version control")
    print("   - Only authorized personnel should have access")
    print("   - Update MIGRATION_SIGNING_PUBLIC_KEY_HEX in migration_signing.rs")


def main():
    if len(sys.argv) < 2:
        print("Usage:")
        print("  Sign migration:    python sign_migration.py <migration_file> <private_key_hex>")
        print("  Generate keypair:  python sign_migration.py --generate")
        print()
        print("Examples:")
        print("  python sign_migration.py migrations/001_init.sql abc123...")
        print("  python sign_migration.py --generate")
        sys.exit(1)

    if sys.argv[1] == "--generate":
        generate_keypair()
        return

    if len(sys.argv) < 3:
        print("ERROR: Missing private key argument")
        print("Usage: python sign_migration.py <migration_file> <private_key_hex>")
        sys.exit(1)

    migration_path = sys.argv[1]
    private_key_hex = sys.argv[2]

    try:
        # Sign the migration
        signature = sign_migration(migration_path, private_key_hex)

        # Write signature file
        sig_path = Path(migration_path).with_suffix(".sql.sig")
        sig_path.write_bytes(signature)

        print(f"✅ Signature written to: {sig_path}")

    except Exception as e:
        print(f"ERROR: {e}")
        sys.exit(1)


if __name__ == "__main__":
    main()
