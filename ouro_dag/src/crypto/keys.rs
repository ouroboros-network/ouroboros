// src/crypto/keys.rs
use ed25519_dalek::{Signature, SigningKey, VerifyingKey};
use signature::{Signer, Verifier};

/// Verify a signature over `msg` using `pubkey` (32 bytes) and signature bytes.
/// Returns `true` if signature verifies.
pub fn verify_bytes(pubkey: &[u8], msg: &[u8], sig_bytes: &[u8]) -> bool {
    if pubkey.len() != 32 || sig_bytes.len() != 64 {
        return false;
    }
    // Construct VerifyingKey from bytes
    let vk = match <&[u8; 32]>::try_from(pubkey) {
        Ok(pubkey_array) => match VerifyingKey::from_bytes(pubkey_array) {
            Ok(v) => v,
            Err(_) => return false,
        },
        Err(_) => return false,
    };

    // Construct Signature from bytes.
    let sig = match <&[u8; 64]>::try_from(sig_bytes) {
        Ok(sig_array) => Signature::from_bytes(sig_array),
        Err(_) => return false,
    };

    vk.verify(msg, &sig).is_ok()
}

/// Sign `msg` with the 32-byte secret seed (raw bytes). Returns signature bytes.
/// Returns None for invalid seed length or on error.
pub fn sign_bytes(secret_seed: &[u8], msg: &[u8]) -> Option<Vec<u8>> {
    if secret_seed.len() != 32 {
        return None;
    }

    let sk = match <&[u8; 32]>::try_from(secret_seed) {
        Ok(seed_array) => SigningKey::from_bytes(seed_array),
        Err(_) => return None,
    };

    let sig: Signature = sk.sign(msg);
    Some(sig.to_bytes().to_vec())
}

/// Derive verifying (public) key bytes from a 32-byte seed.
pub fn public_from_seed(seed: &[u8]) -> Option<Vec<u8>> {
    if seed.len() != 32 {
        return None;
    }
    let sk = match <&[u8; 32]>::try_from(seed) {
        Ok(seed_array) => SigningKey::from_bytes(seed_array),
        Err(_) => return None,
    };
    let vk = VerifyingKey::from(&sk);
    Some(vk.to_bytes().to_vec())
}
