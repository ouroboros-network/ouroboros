// src/zk_proofs/mod.rs
// Zero-Knowledge Proofs for privacy and scalability

pub mod batch;
pub mod circuit;
pub mod privacy;
pub mod state_proof;

use ark_bn254::{Bn254, Fr};
use ark_ff::PrimeField;
use ark_groth16::{Groth16, PreparedVerifyingKey, Proof, ProvingKey, VerifyingKey};
use ark_relations::r1cs::ConstraintSynthesizer;
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use ark_snark::SNARK;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// ZK Proof for a transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionProof {
    pub proof: Vec<u8>,
    pub public_inputs: Vec<String>,
}

/// Global proving/verifying keys
static PROVING_KEY: Lazy<Arc<ProvingKey<Bn254>>> = Lazy::new(|| Arc::new(generate_proving_key()));

static VERIFYING_KEY: Lazy<Arc<PreparedVerifyingKey<Bn254>>> =
    Lazy::new(|| Arc::new(prepare_verifying_key()));

/// Generate proving key (one-time setup).
/// WARNING: This uses a local random setup, meaning the node operator knows the
/// "toxic waste" trapdoor. For production use, a multi-party trusted setup ceremony
/// or a transparent proof system (e.g., STARKs) should be used instead.
fn generate_proving_key() -> ProvingKey<Bn254> {
    use ark_std::rand::thread_rng;

    log::warn!(
        "ZK proving key generated with local randomness. \
         This is NOT suitable for production — use a trusted setup ceremony."
    );

    let circuit = circuit::TransactionCircuit::default();
    let rng = &mut thread_rng();

    Groth16::<Bn254>::generate_random_parameters_with_reduction(circuit, rng)
        .expect("Failed to generate proving key")
}

/// Prepare verifying key for faster verification
fn prepare_verifying_key() -> PreparedVerifyingKey<Bn254> {
    use ark_groth16::prepare_verifying_key;
    let vk = PROVING_KEY.vk.clone();
    prepare_verifying_key(&vk)
}

/// Generate ZK proof for a transaction with adaptive difficulty tracking
pub fn generate_proof_adaptive(
    sender_balance: u64,
    amount: u64,
    recipient: &str,
) -> Result<TransactionProof, String> {
    let start = std::time::Instant::now();
    
    let result = generate_proof(sender_balance, amount, recipient);
    
    let duration = start.elapsed();
    let duration_ms = duration.as_millis() as u64;

    // Use a background task or blocking update to adjust difficulty
    // For simplicity in this synchronous function, we use a thread-safe update if possible
    // or just log it. Since we want to match Nexus, we update the config.
    
    tokio::spawn(async move {
        let mut config = crate::config_manager::CONFIG.write().await;
        config.adaptive_difficulty.last_performance_ms = duration_ms;
        
        // Simple adaptive logic:
        // < 500ms -> "extra_large"
        // < 2s    -> "large"
        // < 5s    -> "medium"
        // > 10s   -> "small"
        let mut new_difficulty = if duration_ms < 500 {
            "extra_large".to_string()
        } else if duration_ms < 2000 {
            "large".to_string()
        } else if duration_ms < 5000 {
            "medium".to_string()
        } else {
            "small".to_string()
        };

        // Apply overrides
        if let Some(ref min) = config.adaptive_difficulty.min_difficulty {
            // Very simple precedence check (should ideally use an enum with Ord)
            if difficulty_rank(&new_difficulty) < difficulty_rank(min) {
                new_difficulty = min.clone();
            }
        }
        
        if let Some(ref max) = config.adaptive_difficulty.max_difficulty {
            if difficulty_rank(&new_difficulty) > difficulty_rank(max) {
                new_difficulty = max.clone();
            }
        }

        config.adaptive_difficulty.current = new_difficulty;
        
        let _ = config.save();
    });

    result
}

fn difficulty_rank(diff: &str) -> u8 {
    match diff {
        "extra_large" | "extra_large_4" => 4,
        "large" => 3,
        "medium" => 2,
        "small" => 1,
        _ => 0,
    }
}

/// Generate ZK proof for a transaction
pub fn generate_proof(
    sender_balance: u64,
    amount: u64,
    recipient: &str,
) -> Result<TransactionProof, String> {
    use ark_std::rand::thread_rng;

    // Create circuit with private inputs
    let circuit = circuit::TransactionCircuit {
        sender_balance: Some(Fr::from(sender_balance)),
        amount: Some(Fr::from(amount)),
        recipient_hash: Some(hash_address(recipient)),
    };

    let rng = &mut thread_rng();
    let proof = Groth16::<Bn254>::prove(&PROVING_KEY, circuit, rng)
        .map_err(|e| format!("Proof generation failed: {}", e))?;

    // Serialize proof
    let mut proof_bytes = Vec::new();
    proof
        .serialize_compressed(&mut proof_bytes)
        .map_err(|e| format!("Proof serialization failed: {}", e))?;

    Ok(TransactionProof {
        proof: proof_bytes,
        public_inputs: vec![amount.to_string(), recipient.to_string()],
    })
}

/// Verify a single ZK proof
pub fn verify_proof(proof: &TransactionProof) -> Result<bool, String> {
    // Deserialize proof
    let proof_obj = Proof::<Bn254>::deserialize_compressed(&proof.proof[..])
        .map_err(|e| format!("Proof deserialization failed: {}", e))?;

    // Parse public inputs
    let amount = proof.public_inputs[0]
        .parse::<u64>()
        .map_err(|e| format!("Invalid amount: {}", e))?;
    let recipient = &proof.public_inputs[1];

    let public_inputs = vec![Fr::from(amount), hash_address(recipient)];

    // Verify proof
    Groth16::<Bn254>::verify_with_processed_vk(&VERIFYING_KEY, &public_inputs, &proof_obj)
        .map_err(|e| format!("Verification failed: {}", e))
}

/// Hash address for circuit
fn hash_address(addr: &str) -> Fr {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(addr.as_bytes());
    let result = hasher.finalize();

    // Convert hash to field element
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&result[..32]);
    Fr::from_le_bytes_mod_order(&bytes)
}

/// Verify multiple proofs efficiently (batch verification)
pub fn verify_batch(proofs: &[TransactionProof]) -> Result<bool, String> {
    batch::verify_batch_proofs(proofs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proof_generation() {
        let proof = generate_proof(1000, 100, "recipient_address").unwrap();
        assert!(!proof.proof.is_empty());
    }

    #[test]
    fn test_proof_verification() {
        let proof = generate_proof(1000, 100, "recipient_address").unwrap();
        let valid = verify_proof(&proof).unwrap();
        assert!(valid);
    }
}
