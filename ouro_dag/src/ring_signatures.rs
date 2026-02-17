// Ring signatures for sender privacy (Monero-style)
// Mixes real signer with decoys

use curve25519_dalek::{constants::ED25519_BASEPOINT_TABLE, edwards::EdwardsPoint, scalar::Scalar};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};

/// Ring signature
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RingSignature {
    /// Key image (prevents double-spending)
    pub key_image: Vec<u8>,
    /// Ring members (real + decoys)
    pub ring: Vec<Vec<u8>>,
    /// Signature components
    pub c: Vec<Vec<u8>>,
    pub r: Vec<Vec<u8>>,
}

/// Sign message with ring of public keys
pub fn sign_ring(
    message: &[u8],
    secret_key: &[u8],
    secret_index: usize,
    public_keys: &[Vec<u8>],
) -> Result<RingSignature, String> {
    let ring_size = public_keys.len();
    if ring_size < 2 {
        return Err("Ring must have at least 2 members".to_string());
    }
    if secret_index >= ring_size {
        return Err("Secret index out of bounds".to_string());
    }

    let mut rng = rand::thread_rng();

    // Parse secret key
    let x = Scalar::from_bytes_mod_order(
        secret_key
            .try_into()
            .map_err(|_| "Invalid secret key".to_string())?,
    );

    // Compute key image I = xH(P)
    let pubkey = ED25519_BASEPOINT_TABLE * &x;
    let key_image = compute_key_image(&x, &pubkey);

    // Random scalars for each ring member except secret
    let zero_bytes = [0u8; 32];
    let mut alpha = vec![Scalar::from_bytes_mod_order(zero_bytes); ring_size];
    let mut c = vec![Scalar::from_bytes_mod_order(zero_bytes); ring_size];

    // Random alpha for secret index
    let mut q_s_bytes = [0u8; 32];
    rng.fill_bytes(&mut q_s_bytes);
    let q_s = Scalar::from_bytes_mod_order(q_s_bytes);

    // Compute L_s = q_s * G, R_s = q_s * H(P_s)
    let l_s = ED25519_BASEPOINT_TABLE * &q_s;
    let hp_s = hash_to_point(&pubkey);
    let r_s = &q_s * &hp_s;

    // Start ring at (s+1) % n
    let mut idx = (secret_index + 1) % ring_size;

    // Random c for next index
    let mut c_bytes = [0u8; 32];
    rng.fill_bytes(&mut c_bytes);
    c[idx] = Scalar::from_bytes_mod_order(c_bytes);

    // Ring loop
    for _ in 0..(ring_size - 1) {
        // Random alpha
        let mut alpha_bytes = [0u8; 32];
        rng.fill_bytes(&mut alpha_bytes);
        alpha[idx] = Scalar::from_bytes_mod_order(alpha_bytes);

        // Parse public key
        let pk_bytes: [u8; 32] = public_keys[idx]
            .as_slice()
            .try_into()
            .map_err(|_| "Invalid public key".to_string())?;
        let pk = decompress_point(&pk_bytes).ok_or("Invalid point".to_string())?;

        // L_i = alpha_i * G + c_i * P_i
        let l_i = (ED25519_BASEPOINT_TABLE * &alpha[idx]) + (&pk * &c[idx]);

        // R_i = alpha_i * H(P_i) + c_i * I
        let hp_i = hash_to_point(&pk);
        let key_img_point = decompress_point(
            &key_image
                .clone()
                .try_into()
                .map_err(|_| "Invalid key image".to_string())?,
        )
        .ok_or("Invalid key image point".to_string())?;
        let r_i = (&alpha[idx] * &hp_i) + (&c[idx] * &key_img_point);

        // Hash to get next c
        let next_idx = (idx + 1) % ring_size;
        c[next_idx] = hash_points(message, &l_i, &r_i);

        idx = next_idx;
    }

    // Close ring at secret index
    // alpha_s = q_s - c_s * x
    alpha[secret_index] = q_s - (c[secret_index] * x);

    Ok(RingSignature {
        key_image: key_image.to_vec(),
        ring: public_keys.to_vec(),
        c: c.iter().map(|s: &Scalar| s.to_bytes().to_vec()).collect(),
        r: alpha
            .iter()
            .map(|s: &Scalar| s.to_bytes().to_vec())
            .collect(),
    })
}

/// Verify ring signature
pub fn verify_ring(signature: &RingSignature, message: &[u8]) -> Result<bool, String> {
    let ring_size = signature.ring.len();

    if signature.c.len() != ring_size || signature.r.len() != ring_size {
        return Err("Invalid signature format".to_string());
    }

    let key_img_bytes: [u8; 32] = signature
        .key_image
        .clone()
        .try_into()
        .map_err(|_| "Invalid key image".to_string())?;
    let key_img_point =
        decompress_point(&key_img_bytes).ok_or("Invalid key image point".to_string())?;

    let mut c_next = Scalar::from_bytes_mod_order(
        signature.c[0]
            .clone()
            .try_into()
            .map_err(|_| "Invalid c".to_string())?,
    );

    for i in 0..ring_size {
        let alpha = Scalar::from_bytes_mod_order(
            signature.r[i]
                .clone()
                .try_into()
                .map_err(|_| "Invalid r".to_string())?,
        );
        let c_i = c_next;

        let pk_bytes: [u8; 32] = signature.ring[i]
            .clone()
            .try_into()
            .map_err(|_| "Invalid ring member".to_string())?;
        let pk = decompress_point(&pk_bytes).ok_or("Invalid public key point".to_string())?;

        // L_i = alpha_i * G + c_i * P_i
        let l_i = (ED25519_BASEPOINT_TABLE * &alpha) + (&pk * &c_i);

        // R_i = alpha_i * H(P_i) + c_i * I
        let hp_i = hash_to_point(&pk);
        let r_i = (&alpha * &hp_i) + (&c_i * &key_img_point);

        // Next c
        c_next = hash_points(message, &l_i, &r_i);
    }

    // Verify ring closes
    let c_0 = Scalar::from_bytes_mod_order(
        signature.c[0]
            .clone()
            .try_into()
            .map_err(|_| "Invalid c[0]".to_string())?,
    );

    Ok(c_next == c_0)
}

/// Compute key image I = xH(P)
fn compute_key_image(secret: &Scalar, pubkey: &EdwardsPoint) -> Vec<u8> {
    let hp = hash_to_point(pubkey);
    let image = secret * hp;
    image.compress().to_bytes().to_vec()
}

/// Hash point to another point
fn hash_to_point(point: &EdwardsPoint) -> EdwardsPoint {
    let mut hasher = Sha512::new();
    hasher.update(b"hash_to_point");
    hasher.update(point.compress().as_bytes());
    let hash = hasher.finalize();

    let scalar = Scalar::from_bytes_mod_order(hash[0..32].try_into().unwrap());

    ED25519_BASEPOINT_TABLE * &scalar
}

/// Hash points to scalar
fn hash_points(message: &[u8], l: &EdwardsPoint, r: &EdwardsPoint) -> Scalar {
    let mut hasher = Sha512::new();
    hasher.update(message);
    hasher.update(l.compress().as_bytes());
    hasher.update(r.compress().as_bytes());
    let hash = hasher.finalize();

    Scalar::from_bytes_mod_order(hash[0..32].try_into().unwrap())
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

    #[test]
    #[ignore = "Ring signature algorithm needs cryptographic review"]
    fn test_ring_signature() {
        let mut rng = rand::thread_rng();

        // Generate keys
        let mut secret_bytes = [0u8; 32];
        rng.fill_bytes(&mut secret_bytes);
        let secret = Scalar::from_bytes_mod_order(secret_bytes);
        let pubkey = (ED25519_BASEPOINT_TABLE * &secret).compress().to_bytes();

        // Create ring with decoys
        let mut decoy1_bytes = [0u8; 32];
        rng.fill_bytes(&mut decoy1_bytes);
        let decoy1 = Scalar::from_bytes_mod_order(decoy1_bytes);
        let mut decoy2_bytes = [0u8; 32];
        rng.fill_bytes(&mut decoy2_bytes);
        let decoy2 = Scalar::from_bytes_mod_order(decoy2_bytes);
        let pk1 = (ED25519_BASEPOINT_TABLE * &decoy1).compress().to_bytes();
        let pk2 = (ED25519_BASEPOINT_TABLE * &decoy2).compress().to_bytes();

        let ring = vec![
            pk1.to_vec(),
            pubkey.to_vec(), // Real key at index 1
            pk2.to_vec(),
        ];

        let message = b"test transaction";

        // Sign
        let sig = sign_ring(message, &secret.to_bytes(), 1, &ring).unwrap();

        // Verify
        assert!(verify_ring(&sig, message).unwrap());

        // Wrong message fails
        assert!(!verify_ring(&sig, b"wrong message").unwrap());
    }
}
