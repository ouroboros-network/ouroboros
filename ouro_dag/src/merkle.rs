use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Merkle proof for inclusion verification
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MerkleProof {
    /// Index of the leaf in the tree
    pub index: usize,

    /// Leaf value (transaction hash or block hash)
    pub leaf: Vec<u8>,

    /// Sibling hashes from leaf to root
    /// Each tuple is (sibling_hash, is_left) where is_left indicates if the current node is on the left
    pub siblings: Vec<(Vec<u8>, bool)>,

    /// Expected root hash
    pub root: Vec<u8>,
}

impl MerkleProof {
    /// Verify this Merkle proof
    pub fn verify(&self) -> Result<bool> {
        let mut current_hash = {
            let mut hasher = Sha256::new();
            hasher.update(&self.leaf);
            hasher.finalize().to_vec()
        };

        for (sibling, is_left) in &self.siblings {
            let mut hasher = Sha256::new();
            if *is_left {
                // Current node is on the left, sibling on the right
                hasher.update(&current_hash);
                hasher.update(sibling);
            } else {
                // Current node is on the right, sibling on the left
                hasher.update(sibling);
                hasher.update(&current_hash);
            }
            current_hash = hasher.finalize().to_vec();
        }

        Ok(current_hash == self.root)
    }

    /// Create proof from hex strings
    pub fn from_hex(
        index: usize,
        leaf_hex: &str,
        siblings_hex: &[(String, bool)],
        root_hex: &str,
    ) -> Result<Self> {
        Ok(Self {
            index,
            leaf: hex::decode(leaf_hex)?,
            siblings: siblings_hex
                .iter()
                .map(|(s, is_left)| Ok((hex::decode(s)?, *is_left)))
                .collect::<Result<Vec<_>>>()?,
            root: hex::decode(root_hex)?,
        })
    }
}

pub struct MerkleTree {
    nodes: Vec<Vec<[u8; 32]>>,     // levels
    original_leaves: Vec<Vec<u8>>, // Store original leaf data for proof generation
}

impl MerkleTree {
    pub fn from_hashes(hashes: &[String]) -> Self {
        let level: Vec<[u8; 32]> = hashes
            .iter()
            .map(|h| {
                let mut d = [0u8; 32];
                d.copy_from_slice(&hex::decode(h).expect("bad hash")[..32]);
                d
            })
            .collect();
        let mut nodes = vec![level.clone()];
        while nodes
            .last()
            .expect("nodes vector should never be empty")
            .len()
            > 1
        {
            let prev = nodes.last().expect("nodes vector should never be empty");
            let mut next = vec![];
            for i in (0..prev.len()).step_by(2) {
                let left = prev[i];
                let right = if i + 1 < prev.len() {
                    prev[i + 1]
                } else {
                    prev[i]
                };
                let mut hasher = Sha256::new();
                hasher.update(&left);
                hasher.update(&right);
                let res = hasher.finalize();
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&res[..32]);
                next.push(arr);
            }
            nodes.push(next);
        }
        MerkleTree {
            nodes,
            original_leaves: vec![],
        }
    }

    /// Build Merkle tree from raw byte leaves
    pub fn from_leaves(leaves: &[Vec<u8>]) -> Self {
        if leaves.is_empty() {
            return MerkleTree {
                nodes: vec![vec![[0u8; 32]]],
                original_leaves: vec![],
            };
        }

        let level: Vec<[u8; 32]> = leaves
            .iter()
            .map(|leaf| {
                let mut hasher = Sha256::new();
                hasher.update(leaf);
                let res = hasher.finalize();
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&res[..32]);
                arr
            })
            .collect();

        let mut nodes = vec![level.clone()];
        while nodes
            .last()
            .expect("nodes vector should never be empty")
            .len()
            > 1
        {
            let prev = nodes.last().expect("nodes vector should never be empty");
            let mut next = vec![];
            for i in (0..prev.len()).step_by(2) {
                let left = prev[i];
                let right = if i + 1 < prev.len() {
                    prev[i + 1]
                } else {
                    prev[i]
                };
                let mut hasher = Sha256::new();
                hasher.update(&left);
                hasher.update(&right);
                let res = hasher.finalize();
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&res[..32]);
                next.push(arr);
            }
            nodes.push(next);
        }
        MerkleTree {
            nodes,
            original_leaves: leaves.to_vec(),
        }
    }

    pub fn root_hex(&self) -> String {
        if let Some(root_level) = self.nodes.last() {
            hex::encode(root_level[0])
        } else {
            hex::encode([0u8; 32])
        }
    }

    pub fn root_bytes(&self) -> Vec<u8> {
        if let Some(root_level) = self.nodes.last() {
            root_level[0].to_vec()
        } else {
            vec![0u8; 32]
        }
    }

    // inclusion proof returns Vec<(sibling_hash_hex, is_left)>
    pub fn proof_for_index(&self, index: usize) -> Vec<(String, bool)> {
        let mut proof = vec![];
        let mut idx = index;
        for level in &self.nodes {
            if level.len() == 1 {
                break;
            }
            let sibling_index = if idx % 2 == 0 { idx + 1 } else { idx - 1 };
            let sibling = if sibling_index < level.len() {
                level[sibling_index]
            } else {
                level[idx]
            };
            proof.push((hex::encode(sibling), idx % 2 == 0)); // is_left = true if current is left
            idx /= 2;
        }
        proof
    }

    /// Generate a MerkleProof for a specific index
    pub fn generate_proof(&self, index: usize) -> Result<MerkleProof> {
        if self.nodes.is_empty() {
            bail!("Empty Merkle tree");
        }

        let leaves = &self.nodes[0];
        if index >= leaves.len() {
            bail!(
                "Index {} out of bounds (tree has {} leaves)",
                index,
                leaves.len()
            );
        }

        // Use original leaf data if available, otherwise use the hash
        let leaf = if index < self.original_leaves.len() {
            self.original_leaves[index].clone()
        } else {
            leaves[index].to_vec()
        };

        let siblings_hex = self.proof_for_index(index);
        let siblings: Vec<(Vec<u8>, bool)> = siblings_hex
            .iter()
            .map(|(s, is_left)| -> Result<(Vec<u8>, bool)> { Ok((hex::decode(s)?, *is_left)) })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(MerkleProof {
            index,
            leaf,
            siblings,
            root: self.root_bytes(),
        })
    }
}

pub fn merkle_root_from_leaves_bytes(leaves: &[Vec<u8>]) -> anyhow::Result<Vec<u8>> {
    if leaves.is_empty() {
        return Ok(Sha256::digest(&[]).to_vec());
    }

    let mut current_level_hashes: Vec<[u8; 32]> = leaves
        .iter()
        .map(|leaf| {
            let mut hasher = Sha256::new();
            hasher.update(leaf);
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&hasher.finalize()[..32]);
            arr
        })
        .collect();

    while current_level_hashes.len() > 1 {
        let mut next_level_hashes = Vec::new();
        for chunk in current_level_hashes.chunks(2) {
            let left = chunk[0];
            let right = if chunk.len() == 2 { chunk[1] } else { chunk[0] }; // Handle odd number of leaves by duplicating the last one

            let mut hasher = Sha256::new();
            hasher.update(&left);
            hasher.update(&right);
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&hasher.finalize()[..32]);
            next_level_hashes.push(arr);
        }
        current_level_hashes = next_level_hashes;
    }

    Ok(current_level_hashes[0].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merkle_proof_generation_and_verification() {
        // Create leaves
        let leaves = vec![
            b"transaction1".to_vec(),
            b"transaction2".to_vec(),
            b"transaction3".to_vec(),
            b"transaction4".to_vec(),
        ];

        // Build tree
        let tree = MerkleTree::from_leaves(&leaves);
        let root = tree.root_bytes();

        // Generate proof for leaf at index 1
        let proof = tree.generate_proof(1).unwrap();

        // Verify proof
        assert!(proof.verify().unwrap(), "Proof verification failed");
        assert_eq!(proof.root, root, "Root mismatch");
    }

    #[test]
    fn test_merkle_proof_invalid_root() {
        let leaves = vec![b"transaction1".to_vec(), b"transaction2".to_vec()];

        let tree = MerkleTree::from_leaves(&leaves);
        let mut proof = tree.generate_proof(0).unwrap();

        // Tamper with root
        proof.root[0] ^= 0xFF;

        // Verification should fail
        assert!(!proof.verify().unwrap(), "Tampered proof should fail");
    }

    #[test]
    fn test_merkle_proof_single_leaf() {
        let leaves = vec![b"single_transaction".to_vec()];
        let tree = MerkleTree::from_leaves(&leaves);
        let proof = tree.generate_proof(0).unwrap();

        assert!(proof.verify().unwrap(), "Single leaf proof failed");
        assert_eq!(
            proof.siblings.len(),
            0,
            "Single leaf should have no siblings"
        );
    }

    #[test]
    fn test_merkle_proof_odd_number_of_leaves() {
        let leaves = vec![b"tx1".to_vec(), b"tx2".to_vec(), b"tx3".to_vec()];

        let tree = MerkleTree::from_leaves(&leaves);

        // Test proof for each leaf
        for i in 0..3 {
            let proof = tree.generate_proof(i).unwrap();
            assert!(
                proof.verify().unwrap(),
                "Proof verification failed for index {}",
                i
            );
        }
    }

    #[test]
    fn test_merkle_root_from_leaves_bytes() {
        let leaves = vec![b"data1".to_vec(), b"data2".to_vec()];

        let root1 = merkle_root_from_leaves_bytes(&leaves).unwrap();
        let root2 = MerkleTree::from_leaves(&leaves).root_bytes();

        // Both methods should produce the same root
        assert_eq!(root1, root2, "Root calculation mismatch");
    }

    #[test]
    fn test_merkle_proof_from_hex() {
        let leaves = vec![b"tx1".to_vec(), b"tx2".to_vec()];

        let tree = MerkleTree::from_leaves(&leaves);
        let proof = tree.generate_proof(0).unwrap();

        // Convert to hex and back
        let leaf_hex = hex::encode(&proof.leaf);
        let root_hex = hex::encode(&proof.root);
        let siblings_hex: Vec<(String, bool)> = proof
            .siblings
            .iter()
            .map(|(s, is_left)| (hex::encode(s), *is_left))
            .collect();

        let proof2 = MerkleProof::from_hex(0, &leaf_hex, &siblings_hex, &root_hex).unwrap();

        assert!(proof2.verify().unwrap(), "Reconstructed proof failed");
        assert_eq!(proof.leaf, proof2.leaf);
        assert_eq!(proof.root, proof2.root);
    }
}
