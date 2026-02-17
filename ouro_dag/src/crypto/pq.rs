// src/crypto/pq.rs
//! Post-quantum cryptography implementation for Ouroboros
//!
//! This module provides quantum-resistant cryptographic primitives using NIST-approved
//! algorithms to protect against future quantum computer attacks.
//!
//! Algorithms used:
//! - CRYSTALS-Dilithium: Post-quantum digital signatures (NIST winner)
//! - CRYSTALS-Kyber: Post-quantum key encapsulation mechanism (NIST winner)
//!
//! Security properties:
//! - Dilithium5 provides ~256-bit security against quantum attacks (Grover's algorithm)
//! - Resistant to Shor's algorithm (breaks RSA/ECC in polynomial time)
//! - Kyber1024 provides ~256-bit quantum security for key exchange
//!
//! Tradeoffs:
//! - Signature size: Dilithium5 = ~4595 bytes vs Ed25519 = 64 bytes (72x larger)
//! - Public key size: Dilithium5 = ~2592 bytes vs Ed25519 = 32 bytes (81x larger)
//! - Verification is ~10x slower than Ed25519
//! - These costs are acceptable for long-term security against quantum threats

use anyhow::{bail, Result};
use pqcrypto_dilithium::dilithium5;
use pqcrypto_kyber::kyber1024;
use pqcrypto_traits::kem::{
    Ciphertext, PublicKey as KemPublicKey, SecretKey as KemSecretKey, SharedSecret,
};
use pqcrypto_traits::sign::{
    DetachedSignature, PublicKey as SignPublicKey, SecretKey as SignSecretKey,
};
use serde::{Deserialize, Serialize};

/// Dilithium5 public key wrapper (quantum-resistant signatures)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DilithiumPublicKey {
    /// Raw public key bytes (2592 bytes for Dilithium5)
    pub bytes: Vec<u8>,
}

impl DilithiumPublicKey {
    /// Create from raw bytes
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        if bytes.len() != dilithium5::public_key_bytes() {
            bail!(
                "Invalid Dilithium public key length: {} (expected {})",
                bytes.len(),
                dilithium5::public_key_bytes()
            );
        }
        Ok(Self { bytes })
    }

    /// Get the underlying bytes
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Verify a detached signature
    pub fn verify(&self, message: &[u8], signature: &DilithiumSignature) -> Result<()> {
        let pk = dilithium5::PublicKey::from_bytes(&self.bytes)
            .map_err(|e| anyhow::anyhow!("Invalid public key: {:?}", e))?;

        let sig = dilithium5::DetachedSignature::from_bytes(&signature.bytes)
            .map_err(|e| anyhow::anyhow!("Invalid signature: {:?}", e))?;

        dilithium5::verify_detached_signature(&sig, message, &pk)
            .map_err(|e| anyhow::anyhow!("Signature verification failed: {:?}", e))?;

        Ok(())
    }
}

/// Dilithium5 secret key wrapper
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DilithiumSecretKey {
    /// Raw secret key bytes (4864 bytes for Dilithium5)
    bytes: Vec<u8>,
}

impl DilithiumSecretKey {
    /// Create from raw bytes
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        if bytes.len() != dilithium5::secret_key_bytes() {
            bail!(
                "Invalid Dilithium secret key length: {} (expected {})",
                bytes.len(),
                dilithium5::secret_key_bytes()
            );
        }
        Ok(Self { bytes })
    }

    /// Get the underlying bytes
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Sign a message (detached signature)
    pub fn sign(&self, message: &[u8]) -> Result<DilithiumSignature> {
        let sk = dilithium5::SecretKey::from_bytes(&self.bytes)
            .map_err(|e| anyhow::anyhow!("Invalid secret key: {:?}", e))?;

        let sig = dilithium5::detached_sign(message, &sk);

        Ok(DilithiumSignature {
            bytes: sig.as_bytes().to_vec(),
        })
    }
}

/// Dilithium5 signature wrapper
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DilithiumSignature {
    /// Raw signature bytes (~4595 bytes for Dilithium5)
    pub bytes: Vec<u8>,
}

impl DilithiumSignature {
    /// Create from raw bytes
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        if bytes.len() != dilithium5::signature_bytes() {
            bail!(
                "Invalid Dilithium signature length: {} (expected {})",
                bytes.len(),
                dilithium5::signature_bytes()
            );
        }
        Ok(Self { bytes })
    }

    /// Get the underlying bytes
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Dilithium5 keypair
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DilithiumKeypair {
    pub public_key: DilithiumPublicKey,
    pub secret_key: DilithiumSecretKey,
}

impl DilithiumKeypair {
    /// Generate a new random Dilithium5 keypair
    pub fn generate() -> Self {
        let (pk, sk) = dilithium5::keypair();

        Self {
            public_key: DilithiumPublicKey {
                bytes: pk.as_bytes().to_vec(),
            },
            secret_key: DilithiumSecretKey {
                bytes: sk.as_bytes().to_vec(),
            },
        }
    }

    /// Sign a message
    pub fn sign(&self, message: &[u8]) -> Result<DilithiumSignature> {
        self.secret_key.sign(message)
    }

    /// Verify a signature
    pub fn verify(&self, message: &[u8], signature: &DilithiumSignature) -> Result<()> {
        self.public_key.verify(message, signature)
    }
}

/// Kyber1024 public key wrapper (quantum-resistant KEM)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KyberPublicKey {
    /// Raw public key bytes (1568 bytes for Kyber1024)
    pub bytes: Vec<u8>,
}

impl KyberPublicKey {
    /// Create from raw bytes
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        if bytes.len() != kyber1024::public_key_bytes() {
            bail!(
                "Invalid Kyber public key length: {} (expected {})",
                bytes.len(),
                kyber1024::public_key_bytes()
            );
        }
        Ok(Self { bytes })
    }

    /// Get the underlying bytes
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Encapsulate a shared secret (returns ciphertext and shared secret)
    pub fn encapsulate(&self) -> Result<(KyberCiphertext, KyberSharedSecret)> {
        let pk = kyber1024::PublicKey::from_bytes(&self.bytes)
            .map_err(|e| anyhow::anyhow!("Invalid public key: {:?}", e))?;

        let (ss, ct) = kyber1024::encapsulate(&pk);

        Ok((
            KyberCiphertext {
                bytes: ct.as_bytes().to_vec(),
            },
            KyberSharedSecret {
                bytes: ss.as_bytes().to_vec(),
            },
        ))
    }
}

/// Kyber1024 secret key wrapper
#[derive(Clone, Debug)]
pub struct KyberSecretKey {
    /// Raw secret key bytes (3168 bytes for Kyber1024)
    bytes: Vec<u8>,
}

impl KyberSecretKey {
    /// Create from raw bytes
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        if bytes.len() != kyber1024::secret_key_bytes() {
            bail!(
                "Invalid Kyber secret key length: {} (expected {})",
                bytes.len(),
                kyber1024::secret_key_bytes()
            );
        }
        Ok(Self { bytes })
    }

    /// Get the underlying bytes
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Decapsulate a shared secret from ciphertext
    pub fn decapsulate(&self, ciphertext: &KyberCiphertext) -> Result<KyberSharedSecret> {
        let sk = kyber1024::SecretKey::from_bytes(&self.bytes)
            .map_err(|e| anyhow::anyhow!("Invalid secret key: {:?}", e))?;

        let ct = kyber1024::Ciphertext::from_bytes(&ciphertext.bytes)
            .map_err(|e| anyhow::anyhow!("Invalid ciphertext: {:?}", e))?;

        let ss = kyber1024::decapsulate(&ct, &sk);

        Ok(KyberSharedSecret {
            bytes: ss.as_bytes().to_vec(),
        })
    }
}

/// Kyber1024 ciphertext
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KyberCiphertext {
    /// Raw ciphertext bytes (1568 bytes for Kyber1024)
    pub bytes: Vec<u8>,
}

impl KyberCiphertext {
    /// Create from raw bytes
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        if bytes.len() != kyber1024::ciphertext_bytes() {
            bail!(
                "Invalid Kyber ciphertext length: {} (expected {})",
                bytes.len(),
                kyber1024::ciphertext_bytes()
            );
        }
        Ok(Self { bytes })
    }

    /// Get the underlying bytes
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Kyber1024 shared secret
#[derive(Clone, Debug)]
pub struct KyberSharedSecret {
    /// Raw shared secret bytes (32 bytes for Kyber1024)
    bytes: Vec<u8>,
}

impl KyberSharedSecret {
    /// Get the underlying bytes
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Kyber1024 keypair
#[derive(Clone, Debug)]
pub struct KyberKeypair {
    pub public_key: KyberPublicKey,
    pub secret_key: KyberSecretKey,
}

impl KyberKeypair {
    /// Generate a new random Kyber1024 keypair
    pub fn generate() -> Self {
        let (pk, sk) = kyber1024::keypair();

        Self {
            public_key: KyberPublicKey {
                bytes: pk.as_bytes().to_vec(),
            },
            secret_key: KyberSecretKey {
                bytes: sk.as_bytes().to_vec(),
            },
        }
    }

    /// Encapsulate a shared secret
    pub fn encapsulate(&self) -> Result<(KyberCiphertext, KyberSharedSecret)> {
        self.public_key.encapsulate()
    }

    /// Decapsulate a shared secret
    pub fn decapsulate(&self, ciphertext: &KyberCiphertext) -> Result<KyberSharedSecret> {
        self.secret_key.decapsulate(ciphertext)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dilithium_sign_verify() {
        let keypair = DilithiumKeypair::generate();
        let message = b"Hello, post-quantum world!";

        // Sign message
        let signature = keypair.sign(message).unwrap();

        // Verify signature
        assert!(keypair.verify(message, &signature).is_ok());

        // Verify with wrong message should fail
        let wrong_message = b"Wrong message";
        assert!(keypair.verify(wrong_message, &signature).is_err());

        // Check signature size
        assert_eq!(signature.bytes.len(), dilithium5::signature_bytes());
        println!(
            " Dilithium5 signature size: {} bytes",
            signature.bytes.len()
        );
    }

    #[test]
    fn test_kyber_kem() {
        let keypair = KyberKeypair::generate();

        // Alice encapsulates a shared secret with Bob's public key
        let (ciphertext, shared_secret_alice) = keypair.public_key.encapsulate().unwrap();

        // Bob decapsulates the shared secret with his private key
        let shared_secret_bob = keypair.secret_key.decapsulate(&ciphertext).unwrap();

        // Both should have the same shared secret
        assert_eq!(shared_secret_alice.as_bytes(), shared_secret_bob.as_bytes());

        // Check sizes
        assert_eq!(ciphertext.bytes.len(), kyber1024::ciphertext_bytes());
        assert_eq!(
            shared_secret_alice.bytes.len(),
            kyber1024::shared_secret_bytes()
        );
        println!(
            " Kyber1024 ciphertext size: {} bytes",
            ciphertext.bytes.len()
        );
        println!(
            " Kyber1024 shared secret size: {} bytes",
            shared_secret_alice.bytes.len()
        );
    }

    #[test]
    fn test_dilithium_key_sizes() {
        let keypair = DilithiumKeypair::generate();

        println!("Dilithium5 key sizes:");
        println!(" Public key: {} bytes", keypair.public_key.bytes.len());
        println!(" Secret key: {} bytes", keypair.secret_key.bytes.len());

        assert_eq!(
            keypair.public_key.bytes.len(),
            dilithium5::public_key_bytes()
        );
        assert_eq!(
            keypair.secret_key.bytes.len(),
            dilithium5::secret_key_bytes()
        );
    }

    #[test]
    fn test_kyber_key_sizes() {
        let keypair = KyberKeypair::generate();

        println!("Kyber1024 key sizes:");
        println!(" Public key: {} bytes", keypair.public_key.bytes.len());
        println!(" Secret key: {} bytes", keypair.secret_key.bytes.len());

        assert_eq!(
            keypair.public_key.bytes.len(),
            kyber1024::public_key_bytes()
        );
        assert_eq!(
            keypair.secret_key.bytes.len(),
            kyber1024::secret_key_bytes()
        );
    }
}
