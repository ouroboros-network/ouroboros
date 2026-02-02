// Light client (SPV-style verification)
// Allows mobile/browser nodes without full blockchain

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// Block header (minimal data for light clients)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockHeader {
    pub height: u64,
    pub hash: Vec<u8>,
    pub prev_hash: Vec<u8>,
    pub merkle_root: Vec<u8>,
    pub timestamp: u64,
    pub validator: String,
}

impl BlockHeader {
    /// Verify header integrity
    pub fn verify(&self) -> bool {
        let computed_hash = self.compute_hash();
        computed_hash == self.hash
    }

    /// Compute block hash
    fn compute_hash(&self) -> Vec<u8> {
        let mut hasher = Sha256::new();
        hasher.update(&self.height.to_le_bytes());
        hasher.update(&self.prev_hash);
        hasher.update(&self.merkle_root);
        hasher.update(&self.timestamp.to_le_bytes());
        hasher.update(self.validator.as_bytes());
        hasher.finalize().to_vec()
    }
}

/// Merkle proof for transaction inclusion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleProof {
    /// Transaction hash
    pub tx_hash: Vec<u8>,
    /// Merkle tree siblings
    pub siblings: Vec<Vec<u8>>,
    /// Position in tree
    pub index: usize,
}

impl MerkleProof {
    /// Verify transaction is in merkle root
    pub fn verify(&self, merkle_root: &[u8]) -> bool {
        let mut current = self.tx_hash.clone();

        let mut idx = self.index;
        for sibling in &self.siblings {
            current = if idx % 2 == 0 {
                hash_pair(&current, sibling)
            } else {
                hash_pair(sibling, &current)
            };
            idx /= 2;
        }

        current == merkle_root
    }
}

/// Light client state
pub struct LightClient {
    /// Known headers (sparse, not all)
    headers: HashMap<u64, BlockHeader>,
    /// Current tip
    tip_height: u64,
    /// Trusted checkpoint (security assumption)
    checkpoint: BlockHeader,
}

impl LightClient {
    /// Create new light client from checkpoint
    pub fn new(checkpoint: BlockHeader) -> Self {
        let tip_height = checkpoint.height;

        let mut headers = HashMap::new();
        headers.insert(checkpoint.height, checkpoint.clone());

        Self {
            headers,
            tip_height,
            checkpoint,
        }
    }

    /// Sync new header
    pub fn sync_header(&mut self, header: BlockHeader) -> Result<(), String> {
        // Verify header integrity
        if !header.verify() {
            return Err("Invalid header hash".to_string());
        }

        // Verify connects to known chain
        if header.height > 0 {
            if let Some(prev) = self.headers.get(&(header.height - 1)) {
                if header.prev_hash != prev.hash {
                    return Err("Header does not connect to previous".to_string());
                }
            }
        }

        // Add header
        self.headers.insert(header.height, header.clone());

        // Update tip
        if header.height > self.tip_height {
            self.tip_height = header.height;
        }

        Ok(())
    }

    /// Verify transaction with merkle proof
    pub fn verify_transaction(
        &self,
        block_height: u64,
        tx_hash: Vec<u8>,
        proof: MerkleProof,
    ) -> Result<bool, String> {
        // Get block header
        let header = self
            .headers
            .get(&block_height)
            .ok_or("Block header not found")?;

        // Verify proof against merkle root
        if !proof.verify(&header.merkle_root) {
            return Ok(false);
        }

        // Verify tx_hash matches
        if proof.tx_hash != tx_hash {
            return Ok(false);
        }

        Ok(true)
    }

    /// Get current tip
    pub fn get_tip(&self) -> Option<&BlockHeader> {
        self.headers.get(&self.tip_height)
    }

    /// Check if header exists
    pub fn has_header(&self, height: u64) -> bool {
        self.headers.contains_key(&height)
    }
}

/// Header sync request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeaderSyncRequest {
    pub start_height: u64,
    pub count: usize,
}

/// Header sync response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeaderSyncResponse {
    pub headers: Vec<BlockHeader>,
}

/// Transaction proof request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxProofRequest {
    pub block_height: u64,
    pub tx_hash: Vec<u8>,
}

/// Transaction proof response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxProofResponse {
    pub proof: Option<MerkleProof>,
}

/// Hash pair of nodes
fn hash_pair(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(left);
    hasher.update(right);
    hasher.finalize().to_vec()
}

/// Build merkle tree and get root
pub fn build_merkle_tree(tx_hashes: &[Vec<u8>]) -> Vec<u8> {
    if tx_hashes.is_empty() {
        return vec![0; 32];
    }

    if tx_hashes.len() == 1 {
        return tx_hashes[0].clone();
    }

    let mut level = tx_hashes.to_vec();

    while level.len() > 1 {
        let mut next_level = Vec::new();

        for i in (0..level.len()).step_by(2) {
            if i + 1 < level.len() {
                next_level.push(hash_pair(&level[i], &level[i + 1]));
            } else {
                next_level.push(level[i].clone());
            }
        }

        level = next_level;
    }

    level[0].clone()
}

/// Generate merkle proof for transaction
pub fn generate_merkle_proof(tx_hashes: &[Vec<u8>], tx_index: usize) -> Option<MerkleProof> {
    if tx_index >= tx_hashes.len() {
        return None;
    }

    let mut siblings = Vec::new();
    let mut level = tx_hashes.to_vec();
    let mut index = tx_index;

    while level.len() > 1 {
        let sibling_index = if index % 2 == 0 { index + 1 } else { index - 1 };

        if sibling_index < level.len() {
            siblings.push(level[sibling_index].clone());
        }

        let mut next_level = Vec::new();
        for i in (0..level.len()).step_by(2) {
            if i + 1 < level.len() {
                next_level.push(hash_pair(&level[i], &level[i + 1]));
            } else {
                next_level.push(level[i].clone());
            }
        }

        level = next_level;
        index /= 2;
    }

    Some(MerkleProof {
        tx_hash: tx_hashes[tx_index].clone(),
        siblings,
        index: tx_index,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_header() {
        let header = BlockHeader {
            height: 100,
            hash: vec![],
            prev_hash: vec![1, 2, 3],
            merkle_root: vec![4, 5, 6],
            timestamp: 1234567890,
            validator: "validator1".to_string(),
        };

        let hash = header.compute_hash();
        let mut header_with_hash = header.clone();
        header_with_hash.hash = hash;

        assert!(header_with_hash.verify());
    }

    #[test]
    fn test_merkle_proof() {
        let tx_hashes = vec![vec![1, 1, 1], vec![2, 2, 2], vec![3, 3, 3], vec![4, 4, 4]];

        let merkle_root = build_merkle_tree(&tx_hashes);

        // Generate proof for tx at index 1
        let proof = generate_merkle_proof(&tx_hashes, 1).unwrap();

        // Verify proof
        assert!(proof.verify(&merkle_root));

        // Wrong root fails
        assert!(!proof.verify(&[0; 32]));
    }

    #[test]
    fn test_light_client() {
        // Create checkpoint
        let checkpoint = BlockHeader {
            height: 0,
            hash: vec![0; 32],
            prev_hash: vec![],
            merkle_root: vec![],
            timestamp: 0,
            validator: "genesis".to_string(),
        };

        let mut checkpoint_with_hash = checkpoint.clone();
        checkpoint_with_hash.hash = checkpoint_with_hash.compute_hash();

        let mut client = LightClient::new(checkpoint_with_hash);

        // Sync new header
        let new_header = BlockHeader {
            height: 1,
            hash: vec![],
            prev_hash: client.get_tip().unwrap().hash.clone(),
            merkle_root: vec![1, 2, 3],
            timestamp: 100,
            validator: "val1".to_string(),
        };

        let mut new_header_with_hash = new_header.clone();
        new_header_with_hash.hash = new_header_with_hash.compute_hash();

        assert!(client.sync_header(new_header_with_hash).is_ok());
        assert_eq!(client.tip_height, 1);
    }
}
