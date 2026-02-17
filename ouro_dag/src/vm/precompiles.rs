// src/vm/precompiles.rs
//! Precompiled contracts - Native Rust implementations
//!
//! These run at NATIVE speed, called from WASM contracts via host functions

use anyhow::{bail, Result};
use blake2::{Blake2b512, Digest as Blake2Digest};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use k256::ecdsa::signature::Verifier as EcdsaVerifier;
use k256::ecdsa::{Signature as EcdsaSignature, VerifyingKey as EcdsaVerifyingKey};
use num_bigint::BigUint;
use sha2::{Digest as Sha2Digest, Sha256};

/// Precompiled contract dispatcher
pub struct Precompiles;

impl Precompiles {
    /// SHA256 hash
    /// Cost: 60 gas + 12 gas per word
    pub fn sha256(data: &[u8]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hasher.finalize().into()
    }

    /// Keccak256 hash (also used by Ethereum, but OVM-native)
    /// Cost: 30 gas + 6 gas per word (OVM gas units)
    pub fn keccak256(data: &[u8]) -> [u8; 32] {
        use tiny_keccak::{Hasher, Keccak};
        let mut hasher = Keccak::v256();
        hasher.update(data);
        let mut output = [0u8; 32];
        hasher.finalize(&mut output);
        output
    }

    /// Blake2b hash (512-bit, truncate to 256)
    /// Cost: 30 gas + 6 gas per word
    pub fn blake2(data: &[u8]) -> [u8; 32] {
        let hash = Blake2b512::digest(data);
        let mut output = [0u8; 32];
        output.copy_from_slice(&hash[..32]);
        output
    }

    /// Ed25519 signature verification
    /// Cost: 3000 gas
    ///
    /// # Arguments
    /// * `public_key` - 32 bytes Ed25519 public key
    /// * `signature` - 64 bytes Ed25519 signature
    /// * `message` - Message that was signed
    pub fn ed25519_verify(public_key: &[u8], signature: &[u8], message: &[u8]) -> Result<bool> {
        // Validate lengths
        if public_key.len() != 32 {
            bail!(
                "Invalid public key length: expected 32, got {}",
                public_key.len()
            );
        }
        if signature.len() != 64 {
            bail!(
                "Invalid signature length: expected 64, got {}",
                signature.len()
            );
        }

        // Parse public key
        let mut pk_bytes = [0u8; 32];
        pk_bytes.copy_from_slice(public_key);
        let pubkey = VerifyingKey::from_bytes(&pk_bytes)
            .map_err(|e| anyhow::anyhow!("Invalid public key: {}", e))?;

        // Parse signature
        let mut sig_bytes = [0u8; 64];
        sig_bytes.copy_from_slice(signature);
        let sig = Signature::from_bytes(&sig_bytes);

        // Verify
        Ok(pubkey.verify(message, &sig).is_ok())
    }

    /// ECDSA (secp256k1) signature verification
    /// Cost: 3000 gas (OVM gas units)
    ///
    /// # Arguments
    /// * `public_key` - 33 bytes compressed or 65 bytes uncompressed public key
    /// * `signature` - 64 bytes ECDSA signature (r || s)
    /// * `message_hash` - 32 bytes message hash (already hashed)
    pub fn ecdsa_verify(public_key: &[u8], signature: &[u8], message_hash: &[u8]) -> Result<bool> {
        // Validate signature length
        if signature.len() != 64 {
            bail!(
                "Invalid signature length: expected 64, got {}",
                signature.len()
            );
        }

        // Validate message hash length
        if message_hash.len() != 32 {
            bail!(
                "Invalid message hash length: expected 32, got {}",
                message_hash.len()
            );
        }

        // Parse public key (handle both compressed and uncompressed)
        let pubkey = if public_key.len() == 33 {
            // Compressed
            EcdsaVerifyingKey::from_sec1_bytes(public_key)
                .map_err(|e| anyhow::anyhow!("Invalid compressed public key: {}", e))?
        } else if public_key.len() == 65 {
            // Uncompressed
            EcdsaVerifyingKey::from_sec1_bytes(public_key)
                .map_err(|e| anyhow::anyhow!("Invalid uncompressed public key: {}", e))?
        } else {
            bail!(
                "Invalid public key length: expected 33 or 65, got {}",
                public_key.len()
            );
        };

        // Parse signature
        let sig = EcdsaSignature::from_slice(signature)
            .map_err(|e| anyhow::anyhow!("Invalid signature: {}", e))?;

        // Verify
        Ok(pubkey.verify(message_hash, &sig).is_ok())
    }

    /// Modular exponentiation: (base^exp) mod modulus
    /// Cost: 200 gas + dynamic based on input size
    ///
    /// Used for RSA verification and other big integer crypto
    pub fn modexp(base: &[u8], exp: &[u8], modulus: &[u8]) -> Vec<u8> {
        // Handle edge cases
        if modulus.is_empty() || modulus.iter().all(|&b| b == 0) {
            return vec![0];
        }

        let base_int = BigUint::from_bytes_be(base);
        let exp_int = BigUint::from_bytes_be(exp);
        let mod_int = BigUint::from_bytes_be(modulus);

        let result = base_int.modpow(&exp_int, &mod_int);
        result.to_bytes_be()
    }

    /// Big integer addition
    /// Cost: 20 gas
    pub fn bigint_add(a: &[u8], b: &[u8]) -> Vec<u8> {
        let a_int = BigUint::from_bytes_be(a);
        let b_int = BigUint::from_bytes_be(b);
        (a_int + b_int).to_bytes_be()
    }

    /// Big integer multiplication
    /// Cost: 50 gas
    pub fn bigint_mul(a: &[u8], b: &[u8]) -> Vec<u8> {
        let a_int = BigUint::from_bytes_be(a);
        let b_int = BigUint::from_bytes_be(b);
        (a_int * b_int).to_bytes_be()
    }

    /// Big integer division
    /// Cost: 50 gas
    pub fn bigint_div(a: &[u8], b: &[u8]) -> Result<Vec<u8>> {
        let a_int = BigUint::from_bytes_be(a);
        let b_int = BigUint::from_bytes_be(b);

        if b_int == BigUint::from(0u32) {
            bail!("Division by zero");
        }

        Ok((a_int / b_int).to_bytes_be())
    }

    /// Big integer modulo
    /// Cost: 50 gas
    pub fn bigint_mod(a: &[u8], b: &[u8]) -> Result<Vec<u8>> {
        let a_int = BigUint::from_bytes_be(a);
        let b_int = BigUint::from_bytes_be(b);

        if b_int == BigUint::from(0u32) {
            bail!("Modulo by zero");
        }

        Ok((a_int % b_int).to_bytes_be())
    }

    /// RIPEMD-160 hash
    /// Cost: 600 gas + 120 gas per word (OVM gas units)
    pub fn ripemd160(data: &[u8]) -> [u8; 20] {
        use ripemd::Digest;
        use ripemd::Ripemd160;

        let mut hasher = Ripemd160::new();
        hasher.update(data);
        hasher.finalize().into()
    }

    /// Identity function (returns input unchanged)
    /// Cost: 15 gas + 3 gas per word
    /// Used for data copying
    pub fn identity(data: &[u8]) -> Vec<u8> {
        data.to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256() {
        let data = b"hello world";
        let hash = Precompiles::sha256(data);

        // Known SHA256 hash of "hello world"
        let expected =
            hex::decode("b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9")
                .unwrap();
        assert_eq!(hash.to_vec(), expected);
    }

    #[test]
    fn test_keccak256() {
        let data = b"hello world";
        let hash = Precompiles::keccak256(data);

        assert_eq!(hash.len(), 32);
    }

    #[test]
    fn test_blake2() {
        let data = b"hello world";
        let hash = Precompiles::blake2(data);

        assert_eq!(hash.len(), 32);
    }

    #[test]
    fn test_ed25519_verify() {
        use ed25519_dalek::{Signer, SigningKey};
        use rand::rngs::OsRng;

        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();

        let message = b"test message";
        let signature = signing_key.sign(message);

        let result =
            Precompiles::ed25519_verify(verifying_key.as_bytes(), &signature.to_bytes(), message)
                .unwrap();

        assert!(result);

        // Test with wrong message
        let result = Precompiles::ed25519_verify(
            verifying_key.as_bytes(),
            &signature.to_bytes(),
            b"wrong message",
        )
        .unwrap();

        assert!(!result);
    }

    #[test]
    fn test_modexp() {
        // Test: 2^3 mod 5 = 8 mod 5 = 3
        let base = vec![2];
        let exp = vec![3];
        let modulus = vec![5];

        let result = Precompiles::modexp(&base, &exp, &modulus);
        assert_eq!(result, vec![3]);

        // Test: 10^5 mod 7 = 100000 mod 7 = 5
        let base = vec![10];
        let exp = vec![5];
        let modulus = vec![7];

        let result = Precompiles::modexp(&base, &exp, &modulus);
        assert_eq!(result, vec![5]);
    }

    #[test]
    fn test_bigint_add() {
        let a = vec![255, 255]; // 65535
        let b = vec![0, 1]; // 1

        let result = Precompiles::bigint_add(&a, &b);
        assert_eq!(result, vec![1, 0, 0]); // 65536
    }

    #[test]
    fn test_bigint_mul() {
        let a = vec![100];
        let b = vec![200];

        let result = Precompiles::bigint_mul(&a, &b);
        let expected = BigUint::from(20000u32).to_bytes_be();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_bigint_div() {
        let a = vec![100];
        let b = vec![10];

        let result = Precompiles::bigint_div(&a, &b).unwrap();
        assert_eq!(result, vec![10]);

        // Division by zero
        assert!(Precompiles::bigint_div(&a, &[0]).is_err());
    }

    #[test]
    fn test_identity() {
        let data = b"test data";
        let result = Precompiles::identity(data);

        assert_eq!(result, data);
    }
}
