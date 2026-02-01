// src/zk_proofs/mod.rs
// Zero-Knowledge Proofs for privacy and scalability

pub mod circuit;
pub mod batch;
pub mod privacy;

use ark_groth16::{Groth16, Proof, ProvingKey, VerifyingKey, PreparedVerifyingKey};
use ark_bn254::{Bn254, Fr};
use ark_relations::r1cs::ConstraintSynthesizer;
use ark_snark::SNARK;
use ark_ff::PrimeField;
use ark_serialize::{CanonicalSerialize, CanonicalDeserialize};
use serde::{Serialize, Deserialize};
use std::sync::Arc;
use once_cell::sync::Lazy;

/// ZK Proof for a transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionProof {
    pub proof: Vec<u8>,
    pub public_inputs: Vec<String>,
}

/// Global proving/verifying keys
static PROVING_KEY: Lazy<Arc<ProvingKey<Bn254>>> = Lazy::new(|| {
    Arc::new(generate_proving_key())
});

static VERIFYING_KEY: Lazy<Arc<PreparedVerifyingKey<Bn254>>> = Lazy::new(|| {
    Arc::new(prepare_verifying_key())
});

/// Generate proving key (one-time setup)
fn generate_proving_key() -> ProvingKey<Bn254> {
    use ark_std::rand::thread_rng;

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
    proof.serialize_compressed(&mut proof_bytes)
        .map_err(|e| format!("Proof serialization failed: {}", e))?;

    Ok(TransactionProof {
        proof: proof_bytes,
        public_inputs: vec![
            amount.to_string(),
            recipient.to_string(),
        ],
    })
}

/// Verify a single ZK proof
pub fn verify_proof(proof: &TransactionProof) -> Result<bool, String> {
    // Deserialize proof
    let proof_obj = Proof::<Bn254>::deserialize_compressed(&proof.proof[..])
        .map_err(|e| format!("Proof deserialization failed: {}", e))?;

    // Parse public inputs
    let amount = proof.public_inputs[0].parse::<u64>()
        .map_err(|e| format!("Invalid amount: {}", e))?;
    let recipient = &proof.public_inputs[1];

    let public_inputs = vec![
        Fr::from(amount),
        hash_address(recipient),
    ];

    // Verify proof
    Groth16::<Bn254>::verify_with_processed_vk(&VERIFYING_KEY, &public_inputs, &proof_obj)
        .map_err(|e| format!("Verification failed: {}", e))
}

/// Hash address for circuit
fn hash_address(addr: &str) -> Fr {
    use sha2::{Sha256, Digest};
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
