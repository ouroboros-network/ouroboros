// src/bft/crypto_bridge.rs
// Small ed25519 helper for consensus. Replace with your keymgmt.rs/crypto.rs integration if needed.
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey, PUBLIC_KEY_LENGTH};
use rand::rngs::OsRng;
use std::fs;
use std::io::Write;
use std::path::Path;

pub type SigBytes = Vec<u8>;

/// Generate a keypair and write to disk (PEM-like raw).
/// Sets file permissions to owner-only (0600) on Unix systems.
pub fn generate_keypair_write(path: &Path) -> anyhow::Result<SigningKey> {
    let mut csprng = OsRng {};
    let kp = SigningKey::generate(&mut csprng);
    let mut f = fs::File::create(path)?;
    f.write_all(&kp.to_keypair_bytes())?;
    crate::crypto::set_restrictive_permissions(path);
    Ok(kp)
}

/// Load raw keypair bytes (32+32).
pub fn load_keypair(path: &Path) -> anyhow::Result<SigningKey> {
    let raw = fs::read(path)?;
    let raw_array: [u8; 64] = raw
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid keypair length"))?;
    let kp = SigningKey::from_keypair_bytes(&raw_array)?;
    Ok(kp)
}

/// Sign message bytes and return signature bytes.
pub fn sign_message(kp: &SigningKey, msg: &[u8]) -> SigBytes {
    kp.sign(msg).to_vec()
}

/// Verify signature given public key bytes (32 bytes) and message.
pub fn verify_message(pubkey: &[u8], msg: &[u8], sig: &[u8]) -> bool {
    if pubkey.len() != PUBLIC_KEY_LENGTH {
        return false;
    }
    // Safe: length check above guarantees this conversion succeeds
    let pubkey_arr: [u8; PUBLIC_KEY_LENGTH] = match pubkey.try_into() {
        Ok(arr) => arr,
        Err(_) => return false,
    };
    let pk = match VerifyingKey::from_bytes(&pubkey_arr) {
        Ok(p) => p,
        Err(_) => return false,
    };
    let signature = match Signature::try_from(sig) {
        Ok(s) => s,
        Err(_) => return false,
    };
    pk.verify(msg, &signature).is_ok()
}
