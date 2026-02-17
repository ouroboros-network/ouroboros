pub mod keys;
pub mod merkle;

// Phase 6: Post-quantum cryptography modules
pub mod hybrid;
pub mod pq; // Post-quantum primitives (Dilithium5, Kyber1024) // Hybrid signatures (Ed25519 + Dilithium5)

use anyhow::{Context, Result};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use hex;
use rand::rngs::OsRng;
use std::fs;
use std::io::Write;
use std::path::Path;

/// Verify ed25519 signature where public key and signature are hex strings.
/// Returns true on successful verification, false on error/invalid lengths.
pub fn verify_ed25519_hex(pubkey_hex: &str, sig_hex: &str, message: &[u8]) -> bool {
    let pk_bytes = match hex::decode(pubkey_hex) {
        Ok(b) => b,
        Err(_) => return false,
    };
    let sig_bytes = match hex::decode(sig_hex) {
        Ok(b) => b,
        Err(_) => return false,
    };

    let pubkey_array: [u8; 32] = match pk_bytes.try_into() {
        Ok(arr) => arr,
        Err(_) => return false,
    };
    let pubkey = match VerifyingKey::from_bytes(&pubkey_array) {
        Ok(pk) => pk,
        Err(_) => return false,
    };

    let sig_array: [u8; 64] = match sig_bytes.try_into() {
        Ok(arr) => arr,
        Err(_) => return false,
    };
    let signature = Signature::from_bytes(&sig_array);

    pubkey.verify(message, &signature).is_ok()
}

// Simple ed25519 key utilities for generation, load, sign, verify.
// Production: replace raw filesystem keystore with encrypted KMS.

pub type SigBytes = Vec<u8>;
pub type PubKeyBytes = Vec<u8>;

/// Generate a new signing key and write raw bytes to `path`.
/// Sets file permissions to owner-only (0600) on Unix systems.
pub fn generate_and_write_signing_key(path: &Path) -> Result<SigningKey> {
    let mut csprng = OsRng {};
    let sk = SigningKey::generate(&mut csprng);
    let bytes = sk.to_keypair_bytes();
    let mut f =
        fs::File::create(path).with_context(|| format!("create key file {}", path.display()))?;
    f.write_all(&bytes)?;
    set_restrictive_permissions(path);
    Ok(sk)
}

/// Set file permissions to owner-only (0600) on Unix systems.
/// No-op on Windows (ACLs handle security differently).
pub fn set_restrictive_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = fs::set_permissions(path, fs::Permissions::from_mode(0o600)) {
            log::warn!("Failed to set permissions on {}: {}", path.display(), e);
        }
    }
    let _ = path; // suppress unused warning on non-unix
}

/// Load a signing key from raw 64-byte file.
pub fn load_signing_key(path: &Path) -> Result<SigningKey> {
    let raw = fs::read(path).with_context(|| format!("read key file {}", path.display()))?;
    let raw_array: [u8; 64] = raw
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid key bytes"))?;
    let sk = SigningKey::from_keypair_bytes(&raw_array)
        .map_err(|e| anyhow::anyhow!("invalid keypair bytes: {}", e))?;
    Ok(sk)
}

/// Extract public key bytes (32 bytes) from signing key.
pub fn pubkey_bytes(sk: &SigningKey) -> PubKeyBytes {
    sk.verifying_key().to_bytes().to_vec()
}

/// Sign message bytes and return signature bytes.
pub fn sign_bytes(sk: &SigningKey, msg: &[u8]) -> SigBytes {
    sk.sign(msg).to_bytes().to_vec()
}

/// Verify signature with public key bytes.
pub fn verify_bytes(pubkey: &[u8], msg: &[u8], sig: &[u8]) -> bool {
    let pubkey_array: [u8; 32] = match pubkey.try_into() {
        Ok(arr) => arr,
        Err(_) => return false,
    };
    let pk = match VerifyingKey::from_bytes(&pubkey_array) {
        Ok(p) => p,
        Err(_) => return false,
    };

    let sig_array: [u8; 64] = match sig.try_into() {
        Ok(arr) => arr,
        Err(_) => return false,
    };
    let s = Signature::from_bytes(&sig_array);
    pk.verify(msg, &s).is_ok()
}
