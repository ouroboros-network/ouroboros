// Stealth addresses for recipient privacy
// One-time addresses that only recipient can detect

use curve25519_dalek::{constants::ED25519_BASEPOINT_TABLE, edwards::EdwardsPoint, scalar::Scalar};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};

/// Stealth address data included in transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StealthData {
    /// Ephemeral public key
    pub ephemeral_pubkey: Vec<u8>,
    /// One-time address
    pub one_time_address: Vec<u8>,
}

/// Recipient's view key pair
#[derive(Clone)]
pub struct ViewKeyPair {
    pub secret: Scalar,
    pub public: EdwardsPoint,
}

/// Recipient's spend key pair
#[derive(Clone)]
pub struct SpendKeyPair {
    pub secret: Scalar,
    pub public: EdwardsPoint,
}

/// Generate stealth address for recipient
pub fn generate_stealth_address(
    recipient_view_pubkey: &[u8],
    recipient_spend_pubkey: &[u8],
) -> Result<StealthData, String> {
    let mut rng = rand::thread_rng();

    // Parse recipient keys
    let view_pub = decompress_point(
        recipient_view_pubkey
            .try_into()
            .map_err(|_| "Invalid view key".to_string())?,
    )
    .ok_or("Invalid view key point".to_string())?;

    let spend_pub = decompress_point(
        recipient_spend_pubkey
            .try_into()
            .map_err(|_| "Invalid spend key".to_string())?,
    )
    .ok_or("Invalid spend key point".to_string())?;

    // Generate random ephemeral key
    let mut r_bytes = [0u8; 32];
    rng.fill_bytes(&mut r_bytes);
    let r = Scalar::from_bytes_mod_order(r_bytes);
    let ephemeral_pubkey = ED25519_BASEPOINT_TABLE * &r;

    // Compute shared secret: r * view_pub
    let shared_secret = r * view_pub;

    // Hash shared secret to scalar
    let hs = hash_to_scalar(&shared_secret);

    // One-time address: P' = H(r*V)*G + B
    let one_time_pubkey = (ED25519_BASEPOINT_TABLE * &hs) + spend_pub;

    Ok(StealthData {
        ephemeral_pubkey: ephemeral_pubkey.compress().to_bytes().to_vec(),
        one_time_address: one_time_pubkey.compress().to_bytes().to_vec(),
    })
}

/// Check if stealth address belongs to us (recipient scans blockchain)
pub fn scan_stealth_address(
    stealth_data: &StealthData,
    view_secret: &[u8],
    spend_pubkey: &[u8],
) -> Result<Option<Vec<u8>>, String> {
    // Parse ephemeral public key
    let ephemeral_pub = decompress_point(
        stealth_data
            .ephemeral_pubkey
            .as_slice()
            .try_into()
            .map_err(|_| "Invalid ephemeral key".to_string())?,
    )
    .ok_or("Invalid ephemeral point".to_string())?;

    // Parse our spend public key
    let spend_pub = decompress_point(
        spend_pubkey
            .try_into()
            .map_err(|_| "Invalid spend key".to_string())?,
    )
    .ok_or("Invalid spend point".to_string())?;

    // Parse view secret
    let v = Scalar::from_bytes_mod_order(
        view_secret
            .try_into()
            .map_err(|_| "Invalid view secret".to_string())?,
    );

    // Compute shared secret: v * R (where R is ephemeral pubkey)
    let shared_secret = v * ephemeral_pub;

    // Hash shared secret
    let hs = hash_to_scalar(&shared_secret);

    // Reconstruct expected one-time address: H(v*R)*G + B
    let expected_address = (ED25519_BASEPOINT_TABLE * &hs) + spend_pub;

    // Parse actual one-time address from transaction
    let actual_address = decompress_point(
        stealth_data
            .one_time_address
            .as_slice()
            .try_into()
            .map_err(|_| "Invalid address".to_string())?,
    )
    .ok_or("Invalid address point".to_string())?;

    // Check if they match
    if expected_address == actual_address {
        // This transaction is for us!
        // Return one-time private key: x' = H(v*R) + b
        Ok(Some(hs.to_bytes().to_vec()))
    } else {
        Ok(None)
    }
}

/// Derive one-time private key for spending
pub fn derive_one_time_privkey(
    shared_secret_hash: &[u8],
    spend_secret: &[u8],
) -> Result<Vec<u8>, String> {
    let hs = Scalar::from_bytes_mod_order(
        shared_secret_hash
            .try_into()
            .map_err(|_| "Invalid hash".to_string())?,
    );
    let b = Scalar::from_bytes_mod_order(
        spend_secret
            .try_into()
            .map_err(|_| "Invalid spend key".to_string())?,
    );

    // x' = H(r*V) + b
    let one_time_key = hs + b;

    Ok(one_time_key.to_bytes().to_vec())
}

/// Generate view and spend key pairs
pub fn generate_keypairs() -> (ViewKeyPair, SpendKeyPair) {
    let mut rng = rand::thread_rng();

    let mut view_secret_bytes = [0u8; 32];
    rng.fill_bytes(&mut view_secret_bytes);
    let view_secret = Scalar::from_bytes_mod_order(view_secret_bytes);
    let view_public = ED25519_BASEPOINT_TABLE * &view_secret;

    let mut spend_secret_bytes = [0u8; 32];
    rng.fill_bytes(&mut spend_secret_bytes);
    let spend_secret = Scalar::from_bytes_mod_order(spend_secret_bytes);
    let spend_public = ED25519_BASEPOINT_TABLE * &spend_secret;

    (
        ViewKeyPair {
            secret: view_secret,
            public: view_public,
        },
        SpendKeyPair {
            secret: spend_secret,
            public: spend_public,
        },
    )
}

/// Hash point to scalar
fn hash_to_scalar(point: &EdwardsPoint) -> Scalar {
    let mut hasher = Sha512::new();
    hasher.update(b"stealth_address_hash");
    hasher.update(point.compress().as_bytes());
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
    fn test_stealth_addresses() {
        // Generate recipient keypairs
        let (view_key, spend_key) = generate_keypairs();

        let view_pub = view_key.public.compress().to_bytes();
        let spend_pub = spend_key.public.compress().to_bytes();

        // Sender generates stealth address
        let stealth = generate_stealth_address(&view_pub, &spend_pub).unwrap();

        // Recipient scans and detects it's for them
        let result =
            scan_stealth_address(&stealth, &view_key.secret.to_bytes(), &spend_pub).unwrap();

        assert!(result.is_some(), "Recipient should detect stealth address");

        // Derive one-time private key for spending
        let one_time_hash = result.unwrap();
        let one_time_key =
            derive_one_time_privkey(&one_time_hash, &spend_key.secret.to_bytes()).unwrap();

        assert!(!one_time_key.is_empty());
    }

    #[test]
    fn test_stealth_wrong_recipient() {
        // Generate two recipients
        let (view_key1, spend_key1) = generate_keypairs();
        let (view_key2, _) = generate_keypairs();

        let view_pub = view_key1.public.compress().to_bytes();
        let spend_pub = spend_key1.public.compress().to_bytes();

        // Sender generates stealth for recipient 1
        let stealth = generate_stealth_address(&view_pub, &spend_pub).unwrap();

        // Recipient 2 scans (should not detect)
        let result =
            scan_stealth_address(&stealth, &view_key2.secret.to_bytes(), &spend_pub).unwrap();

        assert!(result.is_none(), "Wrong recipient should not detect");
    }
}
