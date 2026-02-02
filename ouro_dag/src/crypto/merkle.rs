// src/crypto/merkle.rs
use anyhow::Result;
use sha2::{Digest, Sha256};

/// Deterministic merkle root over vector of leaves (each leaf is raw bytes).
/// If leaves is empty returns sha256 of empty marker.
pub fn merkle_root_from_leaves_bytes(leaves: &[Vec<u8>]) -> Result<Vec<u8>> {
    if leaves.is_empty() {
        // canonical empty root
        let mut h = Sha256::new();
        h.update(b"");
        return Ok(h.finalize().to_vec());
    }

    // compute leaf hashes
    let mut level: Vec<Vec<u8>> = leaves
        .iter()
        .map(|l| {
            let mut h = Sha256::new();
            // leaf domain separation prefix 0x00
            h.update(&[0u8]);
            h.update(l);
            h.finalize().to_vec()
        })
        .collect();

    // build tree
    while level.len() > 1 {
        let mut next = Vec::new();
        let mut i = 0;
        while i < level.len() {
            let left = &level[i];
            let right = if i + 1 < level.len() {
                &level[i + 1]
            } else {
                // duplicate last for odd count
                &level[i]
            };
            let mut h = Sha256::new();
            // internal node domain separation prefix 0x01
            h.update(&[1u8]);
            h.update(left);
            h.update(right);
            next.push(h.finalize().to_vec());
            i += 2;
        }
        level = next;
    }

    Ok(level.remove(0))
}
