// VRF (Verifiable Random Function) for fair leader selection
// Like Algorand/Cardano - provably random but verifiable

use curve25519_dalek::{constants::ED25519_BASEPOINT_TABLE, edwards::EdwardsPoint, scalar::Scalar};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};

/// VRF output and proof
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VrfOutput {
    /// VRF value (random output)
    pub value: Vec<u8>,
    /// Proof of correctness
    pub proof: VrfProof,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VrfProof {
    gamma: Vec<u8>,
    c: Vec<u8>,
    s: Vec<u8>,
}

/// Generate VRF output and proof
pub fn vrf_prove(secret_key: &[u8], input: &[u8]) -> Result<VrfOutput, String> {
    // Parse secret key
    let x = Scalar::from_bytes_mod_order(
        secret_key
            .try_into()
            .map_err(|_| "Invalid secret key".to_string())?,
    );

    // Public key
    let public_key = ED25519_BASEPOINT_TABLE * &x;

    // Hash input to curve point
    let h = hash_to_curve(input);

    // Gamma = x * H(input)
    let gamma = &x * &h;

    // Generate proof using Fiat-Shamir
    let mut rng = rand::thread_rng();
    use rand::Rng;
    let mut k_bytes = [0u8; 32];
    rng.fill_bytes(&mut k_bytes);
    let k = Scalar::from_bytes_mod_order(k_bytes);

    // Commitments
    let u = ED25519_BASEPOINT_TABLE * &k;
    let v = &k * &h;

    // Challenge
    let c = hash_challenge(&h, &gamma, &u, &v);

    // Response
    let s = k + (c * x);

    // VRF value = Hash(Gamma)
    let value = hash_point(&gamma);

    Ok(VrfOutput {
        value,
        proof: VrfProof {
            gamma: gamma.compress().to_bytes().to_vec(),
            c: c.to_bytes().to_vec(),
            s: s.to_bytes().to_vec(),
        },
    })
}

/// Verify VRF proof
pub fn vrf_verify(public_key: &[u8], input: &[u8], output: &VrfOutput) -> Result<bool, String> {
    // Parse public key
    let pk_bytes: [u8; 32] = public_key
        .try_into()
        .map_err(|_| "Invalid public key".to_string())?;
    let pk = decompress_point(&pk_bytes).ok_or("Invalid public key point".to_string())?;

    // Parse proof
    let gamma_bytes: [u8; 32] = output
        .proof
        .gamma
        .clone()
        .try_into()
        .map_err(|_| "Invalid gamma".to_string())?;
    let gamma = decompress_point(&gamma_bytes).ok_or("Invalid gamma point".to_string())?;

    let c = Scalar::from_bytes_mod_order(
        output
            .proof
            .c
            .clone()
            .try_into()
            .map_err(|_| "Invalid c".to_string())?,
    );

    let s = Scalar::from_bytes_mod_order(
        output
            .proof
            .s
            .clone()
            .try_into()
            .map_err(|_| "Invalid s".to_string())?,
    );

    // Hash input to curve
    let h = hash_to_curve(input);

    // Verify proof: sG = U + cY (where Y = public key)
    let u_check = (ED25519_BASEPOINT_TABLE * &s) - (&pk * &c);

    // Verify: sH = V + cGamma
    let v_check = (&s * &h) - (&c * &gamma);

    // Recompute challenge
    let c_check = hash_challenge(&h, &gamma, &u_check, &v_check);

    // Verify challenge matches
    if c != c_check {
        return Ok(false);
    }

    // Verify VRF value
    let value_check = hash_point(&gamma);
    if value_check != output.value {
        return Ok(false);
    }

    Ok(true)
}

/// Select leader based on VRF output
pub fn select_leader(vrf_value: &[u8], total_stake: u64, my_stake: u64) -> bool {
    // Convert VRF to number in [0, 1)
    let vrf_num = bytes_to_float(vrf_value);

    // Probability proportional to stake
    let threshold = (my_stake as f64) / (total_stake as f64);

    vrf_num < threshold
}

/// Hash input to curve point
fn hash_to_curve(input: &[u8]) -> EdwardsPoint {
    let mut hasher = Sha512::new();
    hasher.update(b"vrf_hash_to_curve");
    hasher.update(input);
    let hash = hasher.finalize();

    let scalar = Scalar::from_bytes_mod_order(hash[0..32].try_into().unwrap());

    ED25519_BASEPOINT_TABLE * &scalar
}

/// Hash point to bytes
fn hash_point(point: &EdwardsPoint) -> Vec<u8> {
    let mut hasher = Sha512::new();
    hasher.update(b"vrf_output");
    hasher.update(point.compress().as_bytes());
    let hash = hasher.finalize();
    hash[0..32].to_vec()
}

/// Hash challenge (Fiat-Shamir)
fn hash_challenge(
    h: &EdwardsPoint,
    gamma: &EdwardsPoint,
    u: &EdwardsPoint,
    v: &EdwardsPoint,
) -> Scalar {
    let mut hasher = Sha512::new();
    hasher.update(b"vrf_challenge");
    hasher.update(h.compress().as_bytes());
    hasher.update(gamma.compress().as_bytes());
    hasher.update(u.compress().as_bytes());
    hasher.update(v.compress().as_bytes());
    let hash = hasher.finalize();

    Scalar::from_bytes_mod_order(hash[0..32].try_into().unwrap())
}

/// Convert bytes to float [0, 1)
fn bytes_to_float(bytes: &[u8]) -> f64 {
    if bytes.len() < 8 {
        return 0.0;
    }

    let num = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
    (num as f64) / (u64::MAX as f64)
}

/// Decompress point
fn decompress_point(bytes: &[u8; 32]) -> Option<EdwardsPoint> {
    curve25519_dalek::edwards::CompressedEdwardsY::from_slice(bytes)
        .ok()?
        .decompress()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::Rng;

    #[test]
    fn test_vrf() {
        let mut rng = rand::thread_rng();

        // Generate key
        let mut secret_bytes = [0u8; 32];
        rng.fill_bytes(&mut secret_bytes);
        let secret = Scalar::from_bytes_mod_order(secret_bytes);
        let public = (ED25519_BASEPOINT_TABLE * &secret).compress().to_bytes();

        let input = b"block_height_12345";

        // Generate VRF
        let output = vrf_prove(&secret.to_bytes(), input).unwrap();

        // Verify
        assert!(vrf_verify(&public, input, &output).unwrap());

        // Wrong input fails
        assert!(!vrf_verify(&public, b"wrong_input", &output).unwrap());
    }

    #[test]
    fn test_leader_selection() {
        let mut rng = rand::thread_rng();
        let mut secret_bytes = [0u8; 32];
        rng.fill_bytes(&mut secret_bytes);
        let secret = Scalar::from_bytes_mod_order(secret_bytes);

        let input = b"epoch_100_slot_50";
        let output = vrf_prove(&secret.to_bytes(), input).unwrap();

        // With 50% stake, ~50% chance of being leader
        let total_stake = 1000;
        let my_stake = 500;

        let is_leader = select_leader(&output.value, total_stake, my_stake);

        // Result is deterministic for given VRF
        assert!(is_leader == select_leader(&output.value, total_stake, my_stake));
    }

    #[test]
    fn test_vrf_deterministic() {
        let mut rng = rand::thread_rng();
        let mut secret_bytes = [0u8; 32];
        rng.fill_bytes(&mut secret_bytes);
        let secret = Scalar::from_bytes_mod_order(secret_bytes);

        let input = b"same_input";

        // Same input always gives same output
        let out1 = vrf_prove(&secret.to_bytes(), input).unwrap();
        let out2 = vrf_prove(&secret.to_bytes(), input).unwrap();

        assert_eq!(out1.value, out2.value);
    }
}
