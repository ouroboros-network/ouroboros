// src/crypto/hybrid.rs
//! Hybrid signature scheme combining Ed25519 and Dilithium5
//!
//! This module provides a transitional security mechanism that combines:
//! - Ed25519: Fast, small signatures (64 bytes) - quantum-vulnerable
//! - Dilithium5: Quantum-resistant signatures (~4595 bytes)
//!
//! Why hybrid signatures?
//! 1. **Defense in depth**: System remains secure even if one scheme is broken
//! 2. **Smooth migration**: Validators can gradually adopt PQ crypto without hard fork
//! 3. **Backward compatibility**: Old nodes can verify Ed25519, new nodes verify both
//! 4. **Risk mitigation**: If Dilithium has implementation bugs, Ed25519 provides fallback
//!
//! Security model:
//! - Both signatures must verify for the hybrid signature to be valid
//! - Provides security of the stronger scheme (Dilithium5 after quantum computers)
//! - Current security: max(Ed25519, Dilithium5) = Dilithium5 (quantum-resistant)
//!
//! Verification policy (for migration period):
//! - Phase 1 (Now-2026): Ed25519 OR Hybrid accepted
//! - Phase 2 (2026-2028): Hybrid required, Ed25519 deprecated
//! - Phase 3 (2028+): Pure Dilithium5, Ed25519 rejected

use anyhow::{bail, Result};
use ed25519_dalek::{Signature as Ed25519Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

use super::pq::{DilithiumKeypair, DilithiumPublicKey, DilithiumSignature};

/// Hybrid signature containing both Ed25519 and Dilithium5 signatures
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HybridSignature {
    /// Ed25519 signature (64 bytes)
    pub ed25519: Vec<u8>,
    /// Dilithium5 signature (~4595 bytes)
    pub dilithium: Vec<u8>,
}

impl HybridSignature {
    /// Create a new hybrid signature
    pub fn new(ed25519: Vec<u8>, dilithium: Vec<u8>) -> Result<Self> {
        if ed25519.len() != 64 {
            bail!("Invalid Ed25519 signature length: {}", ed25519.len());
        }
        if dilithium.len() != pqcrypto_dilithium::dilithium5::signature_bytes() {
            bail!("Invalid Dilithium signature length: {}", dilithium.len());
        }
        Ok(Self { ed25519, dilithium })
    }

    /// Get total signature size
    pub fn size_bytes(&self) -> usize {
        self.ed25519.len() + self.dilithium.len()
    }

    /// Verify both signatures
    pub fn verify(
        &self,
        message: &[u8],
        ed25519_pubkey: &VerifyingKey,
        dilithium_pubkey: &DilithiumPublicKey,
    ) -> Result<()> {
        // Verify Ed25519 signature
        let ed_sig = Ed25519Signature::from_bytes(
            self.ed25519
                .as_slice()
                .try_into()
                .map_err(|_| anyhow::anyhow!("Invalid Ed25519 signature length"))?,
        );

        ed25519_pubkey
            .verify(message, &ed_sig)
            .map_err(|e| anyhow::anyhow!("Ed25519 verification failed: {}", e))?;

        // Verify Dilithium signature
        let dil_sig = DilithiumSignature::from_bytes(self.dilithium.clone())?;
        dilithium_pubkey.verify(message, &dil_sig)?;

        Ok(())
    }
}

/// Hybrid keypair containing both Ed25519 and Dilithium5 keypairs
pub struct HybridKeypair {
    pub ed25519_keypair: (SigningKey, VerifyingKey),
    pub dilithium_keypair: DilithiumKeypair,
}

impl HybridKeypair {
    /// Generate a new random hybrid keypair
    pub fn generate() -> Self {
        // Generate Ed25519 keypair
        let mut csprng = rand::rngs::OsRng;
        let ed25519_signing = SigningKey::generate(&mut csprng);
        let ed25519_verifying = ed25519_signing.verifying_key();

        // Generate Dilithium5 keypair
        let dilithium_keypair = DilithiumKeypair::generate();

        Self {
            ed25519_keypair: (ed25519_signing, ed25519_verifying),
            dilithium_keypair,
        }
    }

    /// Create from existing keypairs
    pub fn from_keypairs(
        ed25519_signing: SigningKey,
        ed25519_verifying: VerifyingKey,
        dilithium_keypair: DilithiumKeypair,
    ) -> Self {
        Self {
            ed25519_keypair: (ed25519_signing, ed25519_verifying),
            dilithium_keypair,
        }
    }

    /// Sign a message with both keys
    pub fn sign(&self, message: &[u8]) -> Result<HybridSignature> {
        // Sign with Ed25519
        let ed_sig = self.ed25519_keypair.0.sign(message);

        // Sign with Dilithium5
        let dil_sig = self.dilithium_keypair.sign(message)?;

        HybridSignature::new(ed_sig.to_bytes().to_vec(), dil_sig.bytes)
    }

    /// Verify a hybrid signature
    pub fn verify(&self, message: &[u8], signature: &HybridSignature) -> Result<()> {
        signature.verify(
            message,
            &self.ed25519_keypair.1,
            &self.dilithium_keypair.public_key,
        )
    }

    /// Get Ed25519 public key
    pub fn ed25519_public_key(&self) -> &VerifyingKey {
        &self.ed25519_keypair.1
    }

    /// Get Dilithium5 public key
    pub fn dilithium_public_key(&self) -> &DilithiumPublicKey {
        &self.dilithium_keypair.public_key
    }
}

/// Hybrid public key bundle for verification
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HybridPublicKey {
    /// Ed25519 public key (32 bytes)
    pub ed25519: Vec<u8>,
    /// Dilithium5 public key (2592 bytes)
    pub dilithium: Vec<u8>,
}

impl HybridPublicKey {
    /// Create from raw bytes
    pub fn new(ed25519: Vec<u8>, dilithium: Vec<u8>) -> Result<Self> {
        if ed25519.len() != 32 {
            bail!("Invalid Ed25519 public key length: {}", ed25519.len());
        }
        if dilithium.len() != pqcrypto_dilithium::dilithium5::public_key_bytes() {
            bail!("Invalid Dilithium public key length: {}", dilithium.len());
        }
        Ok(Self { ed25519, dilithium })
    }

    /// Verify a hybrid signature
    pub fn verify(&self, message: &[u8], signature: &HybridSignature) -> Result<()> {
        // Parse Ed25519 public key
        let ed_pubkey = VerifyingKey::from_bytes(
            self.ed25519
                .as_slice()
                .try_into()
                .map_err(|_| anyhow::anyhow!("Invalid Ed25519 public key length"))?,
        )
        .map_err(|e| anyhow::anyhow!("Invalid Ed25519 public key: {}", e))?;

        // Parse Dilithium public key
        let dil_pubkey = DilithiumPublicKey::from_bytes(self.dilithium.clone())?;

        // Verify hybrid signature
        signature.verify(message, &ed_pubkey, &dil_pubkey)
    }

    /// Get total public key size
    pub fn size_bytes(&self) -> usize {
        self.ed25519.len() + self.dilithium.len()
    }
}

/// Migration policy for hybrid signatures
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MigrationPhase {
    /// Phase 1: Ed25519 OR Hybrid accepted (backward compatible)
    Phase1EdOrHybrid,
    /// Phase 2: Hybrid required (migration period)
    Phase2HybridOnly,
    /// Phase 3: Pure Dilithium5 (full quantum resistance)
    Phase3DilithiumOnly,
}

impl MigrationPhase {
    /// Get current migration phase based on block height or timestamp
    pub fn current_phase(block_height: i64) -> Self {
        // Phase boundaries (example - adjust based on mainnet timeline)
        const PHASE2_START: i64 = 1_000_000; // ~2026 (assuming 10s blocks)
        const PHASE3_START: i64 = 5_000_000; // ~2028

        if block_height < PHASE2_START {
            MigrationPhase::Phase1EdOrHybrid
        } else if block_height < PHASE3_START {
            MigrationPhase::Phase2HybridOnly
        } else {
            MigrationPhase::Phase3DilithiumOnly
        }
    }

    /// Check if signature type is accepted in this phase
    pub fn accepts_ed25519_only(&self) -> bool {
        matches!(self, MigrationPhase::Phase1EdOrHybrid)
    }

    /// Check if hybrid signatures are required
    pub fn requires_hybrid(&self) -> bool {
        matches!(self, MigrationPhase::Phase2HybridOnly)
    }

    /// Check if pure Dilithium is required
    pub fn requires_dilithium_only(&self) -> bool {
        matches!(self, MigrationPhase::Phase3DilithiumOnly)
    }
}

/// Verify a signature according to migration policy
pub fn verify_with_migration_policy(
    message: &[u8],
    signature_bytes: &[u8],
    ed25519_pubkey: Option<&VerifyingKey>,
    dilithium_pubkey: Option<&DilithiumPublicKey>,
    migration_phase: &MigrationPhase,
) -> Result<()> {
    match migration_phase {
        MigrationPhase::Phase1EdOrHybrid => {
            // Try Ed25519 first (backward compatible)
            if let Some(ed_pk) = ed25519_pubkey {
                if signature_bytes.len() == 64 {
                    let sig = Ed25519Signature::from_bytes(
                        signature_bytes
                            .try_into()
                            .map_err(|_| anyhow::anyhow!("Invalid Ed25519 signature"))?,
                    );
                    return ed_pk
                        .verify(message, &sig)
                        .map_err(|e| anyhow::anyhow!("Ed25519 verification failed: {}", e));
                }
            }

            // Try hybrid signature
            if let (Some(ed_pk), Some(dil_pk)) = (ed25519_pubkey, dilithium_pubkey) {
                let hybrid_sig: HybridSignature = bincode::deserialize(signature_bytes)
                    .map_err(|e| anyhow::anyhow!("Failed to parse hybrid signature: {}", e))?;
                return hybrid_sig.verify(message, ed_pk, dil_pk);
            }

            bail!("No valid signature format found in Phase 1");
        }

        MigrationPhase::Phase2HybridOnly => {
            // Hybrid required
            let ed_pk = ed25519_pubkey
                .ok_or_else(|| anyhow::anyhow!("Ed25519 pubkey required for hybrid"))?;
            let dil_pk = dilithium_pubkey
                .ok_or_else(|| anyhow::anyhow!("Dilithium pubkey required for hybrid"))?;

            let hybrid_sig: HybridSignature = bincode::deserialize(signature_bytes)
                .map_err(|e| anyhow::anyhow!("Failed to parse hybrid signature: {}", e))?;

            hybrid_sig.verify(message, ed_pk, dil_pk)
        }

        MigrationPhase::Phase3DilithiumOnly => {
            // Pure Dilithium only
            let dil_pk =
                dilithium_pubkey.ok_or_else(|| anyhow::anyhow!("Dilithium pubkey required"))?;

            let dil_sig = DilithiumSignature::from_bytes(signature_bytes.to_vec())?;
            dil_pk.verify(message, &dil_sig)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hybrid_sign_verify() {
        let keypair = HybridKeypair::generate();
        let message = b"Hybrid quantum-resistant message";

        // Sign with both keys
        let signature = keypair.sign(message).unwrap();

        // Verify signature
        assert!(keypair.verify(message, &signature).is_ok());

        // Verify with wrong message should fail
        let wrong_message = b"Wrong message";
        assert!(keypair.verify(wrong_message, &signature).is_err());

        println!(" Hybrid signature size: {} bytes", signature.size_bytes());
        println!(" - Ed25519: {} bytes", signature.ed25519.len());
        println!(" - Dilithium: {} bytes", signature.dilithium.len());
    }

    #[test]
    fn test_hybrid_public_key_verify() {
        let keypair = HybridKeypair::generate();
        let message = b"Test message for hybrid verification";

        // Create signature
        let signature = keypair.sign(message).unwrap();

        // Create public key bundle
        let pubkey = HybridPublicKey::new(
            keypair.ed25519_public_key().to_bytes().to_vec(),
            keypair.dilithium_public_key().bytes.clone(),
        )
        .unwrap();

        // Verify with public key bundle
        assert!(pubkey.verify(message, &signature).is_ok());

        println!(" Hybrid public key size: {} bytes", pubkey.size_bytes());
        println!(" - Ed25519: {} bytes", pubkey.ed25519.len());
        println!(" - Dilithium: {} bytes", pubkey.dilithium.len());
    }

    #[test]
    fn test_migration_phases() {
        // Test phase transitions
        assert_eq!(
            MigrationPhase::current_phase(0),
            MigrationPhase::Phase1EdOrHybrid
        );
        assert_eq!(
            MigrationPhase::current_phase(500_000),
            MigrationPhase::Phase1EdOrHybrid
        );
        assert_eq!(
            MigrationPhase::current_phase(1_000_000),
            MigrationPhase::Phase2HybridOnly
        );
        assert_eq!(
            MigrationPhase::current_phase(3_000_000),
            MigrationPhase::Phase2HybridOnly
        );
        assert_eq!(
            MigrationPhase::current_phase(5_000_000),
            MigrationPhase::Phase3DilithiumOnly
        );

        // Test policy checks
        assert!(MigrationPhase::Phase1EdOrHybrid.accepts_ed25519_only());
        assert!(!MigrationPhase::Phase2HybridOnly.accepts_ed25519_only());
        assert!(!MigrationPhase::Phase3DilithiumOnly.accepts_ed25519_only());

        assert!(!MigrationPhase::Phase1EdOrHybrid.requires_hybrid());
        assert!(MigrationPhase::Phase2HybridOnly.requires_hybrid());
        assert!(!MigrationPhase::Phase3DilithiumOnly.requires_hybrid());
    }

    #[test]
    fn test_migration_policy_phase1() {
        let keypair = HybridKeypair::generate();
        let message = b"Migration test message";

        // Phase 1: Ed25519 should work
        let ed_sig = keypair.ed25519_keypair.0.sign(message);
        let result = verify_with_migration_policy(
            message,
            &ed_sig.to_bytes(),
            Some(&keypair.ed25519_keypair.1),
            None,
            &MigrationPhase::Phase1EdOrHybrid,
        );
        assert!(result.is_ok(), "Phase 1 should accept Ed25519");

        // Phase 1: Hybrid should also work
        let hybrid_sig = keypair.sign(message).unwrap();
        let hybrid_bytes = bincode::serialize(&hybrid_sig).unwrap();
        let result = verify_with_migration_policy(
            message,
            &hybrid_bytes,
            Some(&keypair.ed25519_keypair.1),
            Some(&keypair.dilithium_keypair.public_key),
            &MigrationPhase::Phase1EdOrHybrid,
        );
        assert!(result.is_ok(), "Phase 1 should accept hybrid");
    }

    #[test]
    fn test_signature_size_comparison() {
        let keypair = HybridKeypair::generate();
        let message = b"Size comparison test";

        // Ed25519 signature
        let ed_sig = keypair.ed25519_keypair.0.sign(message);
        println!("\nSignature size comparison:");
        println!(" Ed25519: {} bytes", ed_sig.to_bytes().len());

        // Dilithium5 signature
        let dil_sig = keypair.dilithium_keypair.sign(message).unwrap();
        println!(" Dilithium5: {} bytes", dil_sig.bytes.len());

        // Hybrid signature
        let hybrid_sig = keypair.sign(message).unwrap();
        println!(" Hybrid: {} bytes", hybrid_sig.size_bytes());
        println!(
            " Overhead ratio: {:.1}x",
            hybrid_sig.size_bytes() as f64 / ed_sig.to_bytes().len() as f64
        );
    }
}
