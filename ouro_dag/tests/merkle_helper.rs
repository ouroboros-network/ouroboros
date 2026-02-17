// tests/merkle_helper.rs
use ouro_dag::crypto::merkle::merkle_root_from_leaves_bytes;
use sha2::{Digest, Sha256};

// Helper to hash leaf with domain separation (0x00 prefix)
fn hash_leaf(data: &[u8]) -> Vec<u8> {
    let mut h = Sha256::new();
    h.update(&[0u8]); // Leaf prefix
    h.update(data);
    h.finalize().to_vec()
}

// Helper to hash internal node with domain separation (0x01 prefix)
fn hash_internal(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut h = Sha256::new();
    h.update(&[1u8]); // Internal node prefix
    h.update(left);
    h.update(right);
    h.finalize().to_vec()
}

#[test]
fn test_merkle_root_single_leaf() {
    let leaf = vec![1, 2, 3, 4];
    let leaves = vec![leaf.clone()];
    // Single leaf: just the hashed leaf with domain separation
    let expected_root = hash_leaf(&leaf);
    let root = merkle_root_from_leaves_bytes(&leaves).unwrap();
    assert_eq!(root, expected_root);
}

#[test]
fn test_merkle_root_two_leaves() {
    let leaf1 = vec![1, 2, 3, 4];
    let leaf2 = vec![5, 6, 7, 8];
    let leaves = vec![leaf1.clone(), leaf2.clone()];

    // Hash leaves with domain separation
    let h1 = hash_leaf(&leaf1);
    let h2 = hash_leaf(&leaf2);
    // Combine with internal node prefix
    let expected_root = hash_internal(&h1, &h2);

    let root = merkle_root_from_leaves_bytes(&leaves).unwrap();
    assert_eq!(root, expected_root);
}

#[test]
fn test_merkle_root_three_leaves() {
    let leaf1 = vec![1, 2, 3, 4];
    let leaf2 = vec![5, 6, 7, 8];
    let leaf3 = vec![9, 10, 11, 12];
    let leaves = vec![leaf1.clone(), leaf2.clone(), leaf3.clone()];

    // Hash leaves with domain separation
    let h1 = hash_leaf(&leaf1);
    let h2 = hash_leaf(&leaf2);
    let h3 = hash_leaf(&leaf3);

    // Build tree: (H(L1) + H(L2)) + (H(L3) + H(L3))
    let h12 = hash_internal(&h1, &h2);
    let h33 = hash_internal(&h3, &h3); // Duplicate for odd count
    let expected_root = hash_internal(&h12, &h33);

    let root = merkle_root_from_leaves_bytes(&leaves).unwrap();
    assert_eq!(root, expected_root);
}

#[test]
fn test_merkle_root_empty_leaves() {
    let leaves: Vec<Vec<u8>> = vec![];
    let expected_root = Sha256::digest(&[]).to_vec();
    let root = merkle_root_from_leaves_bytes(&leaves).unwrap();
    assert_eq!(root, expected_root);
}
