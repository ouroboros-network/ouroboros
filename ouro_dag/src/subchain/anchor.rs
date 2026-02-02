// src/subchain/anchor.rs
use sha2::{Digest, Sha256};

/// Compute a Merkle root from leaves (each leaf is bytes).
pub fn merkle_root(leaves: &[Vec<u8>]) -> Vec<u8> {
    if leaves.is_empty() {
        return vec![0u8; 32];
    }
    let mut level: Vec<Vec<u8>> = leaves.to_vec();
    while level.len() > 1 {
        let mut next = Vec::with_capacity((level.len() + 1) / 2);
        for chunk in level.chunks(2) {
            if chunk.len() == 2 {
                let mut h = Sha256::new();
                h.update(&chunk[0]);
                h.update(&chunk[1]);
                next.push(h.finalize().to_vec());
            } else {
                // odd, promote last
                next.push(chunk[0].clone());
            }
        }
        level = next;
    }
    level.swap_remove(0)
}
