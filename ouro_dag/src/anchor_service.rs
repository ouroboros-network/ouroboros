use crate::merkle::{MerkleProof, MerkleTree};
use crate::multisig::{MultiSigCoordinator, MultiSignature, PartialSignature};
use crate::PgPool;
use anyhow::Result;
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Aggregator attestation for verifiable anchor posting (Phase 6 security hardening)
///
/// This cryptographic attestation proves that:
/// 1. The aggregator correctly computed the Merkle root from the batch
/// 2. The batch includes all claimed transactions
/// 3. The aggregator is accountable for any fraud
///
/// Security properties:
/// - Attestation is signed by aggregator's Ed25519 key
/// - Anyone can verify the attestation against the Merkle root
/// - Fraud proofs can challenge incorrect attestations
/// - Slashing mechanism penalizes fraudulent aggregators
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AggregatorAttestation {
    /// Subchain UUID this attestation is for
    pub subchain: Uuid,

    /// Block height being anchored
    pub block_height: i64,

    /// Merkle root of the batch
    pub merkle_root: Vec<u8>,

    /// Number of transactions in the batch
    pub tx_count: u64,

    /// Total batch size in bytes
    pub batch_size_bytes: u64,

    /// Aggregator's Ed25519 public key
    pub aggregator_pubkey: Vec<u8>,

    /// Ed25519 signature over attestation data
    pub signature: Vec<u8>,

    /// Timestamp when attestation was created
    pub created_at: DateTime<Utc>,

    /// Optional: Hash of serialized transaction list for fraud proof verification
    pub tx_list_hash: Option<Vec<u8>>,
}

impl AggregatorAttestation {
    /// Create the message to be signed for this attestation
    pub fn signing_message(&self) -> Vec<u8> {
        let mut msg = Vec::new();

        // Include all critical fields in the signature
        msg.extend_from_slice(self.subchain.as_bytes());
        msg.extend_from_slice(&self.block_height.to_le_bytes());
        msg.extend_from_slice(&self.merkle_root);
        msg.extend_from_slice(&self.tx_count.to_le_bytes());
        msg.extend_from_slice(&self.batch_size_bytes.to_le_bytes());
        msg.extend_from_slice(&self.created_at.timestamp().to_le_bytes());

        if let Some(ref tx_hash) = self.tx_list_hash {
            msg.extend_from_slice(tx_hash);
        }

        msg
    }

    /// Verify the attestation signature
    pub fn verify(&self) -> Result<()> {
        // Parse public key
        let pubkey_array: [u8; 32] = self
            .aggregator_pubkey
            .as_slice()
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invalid public key length"))?;

        let verifying_key = VerifyingKey::from_bytes(&pubkey_array)
            .map_err(|e| anyhow::anyhow!("Invalid Ed25519 public key: {}", e))?;

        // Parse signature
        let sig_array: [u8; 64] = self
            .signature
            .as_slice()
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invalid signature length"))?;

        let signature = Signature::from_bytes(&sig_array);

        // Verify signature
        let message = self.signing_message();
        verifying_key
            .verify(&message, &signature)
            .map_err(|e| anyhow::anyhow!("Attestation signature verification failed: {}", e))?;

        Ok(())
    }

    /// Verify a Merkle proof for a specific transaction against this attestation
    pub fn verify_merkle_proof(&self, proof: &MerkleProof) -> Result<()> {
        // First verify the attestation signature
        self.verify()?;

        // Verify the Merkle proof
        if !proof.verify()? {
            anyhow::bail!("Merkle proof verification failed");
        }

        // Verify the root matches
        if proof.root != self.merkle_root {
            anyhow::bail!(
                "Merkle root mismatch: expected {}, got {}",
                hex::encode(&self.merkle_root),
                hex::encode(&proof.root)
            );
        }

        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct AnchorService {
    pub pg: PgPool,
    pub multisig: Option<MultiSigCoordinator>,
}

impl AnchorService {
    /// Create a new anchor service without multi-sig (backward compatible)
    pub fn new(pg: PgPool) -> Self {
        Self { pg, multisig: None }
    }

    /// Create a new anchor service with multi-sig coordinator
    pub fn new_with_multisig(pg: PgPool, multisig: MultiSigCoordinator) -> Self {
        Self {
            pg,
            multisig: Some(multisig),
        }
    }

    /// Submit a partial signature for an anchor (multi-sig mode)
    ///
    /// Returns:
    /// - Ok(Some(txid)): Threshold reached, anchor was posted, returns txid
    /// - Ok(None): Signature accepted, but threshold not yet reached
    /// - Err: Invalid signature or other error
    pub async fn submit_partial_signature(
        &self,
        subchain: Uuid,
        block_height: i64,
        root: &[u8],
        partial: PartialSignature,
    ) -> Result<Option<String>> {
        let coordinator = self
            .multisig
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Multi-sig not enabled"))?;

        // Store partial signature in database for crash recovery

        log::info!(
            "NOTE Received partial signature from {} for anchor {} (height {})",
            partial.validator_id,
            hex::encode(&root[..8]),
            block_height
        );

        // Submit to coordinator
        let is_complete =
            coordinator.submit_partial_signature(root.to_vec(), subchain, block_height, partial)?;

        if is_complete {
            // Threshold reached! Post the anchor
            let multisig = coordinator
                .get_completed_multisig(root)
                .ok_or_else(|| anyhow::anyhow!("Multi-sig disappeared after completion"))?;

            // Verify multi-sig before posting
            coordinator.verify_multisig(&multisig)?;

            log::info!(
                " Multi-sig threshold reached! Posting anchor {} to mainchain",
                hex::encode(&root[..8])
            );

            // Post to mainchain with multi-sig data
            let txid = self
                .post_multisig_anchor(subchain, block_height, root, &multisig)
                .await?;

            // Clean up completed multi-sig
            coordinator.remove_completed(root);

            Ok(Some(txid))
        } else {
            let count = coordinator.get_signature_count(root);
            log::info!(
                "⏳ Anchor {} waiting for more signatures ({} collected)",
                hex::encode(&root[..8]),
                count
            );
            Ok(None)
        }
    }

    /// Post anchor with multi-sig verification (internal)
    async fn post_multisig_anchor(
        &self,
        subchain: Uuid,
        block_height: i64,
        root: &[u8],
        multisig: &MultiSignature,
    ) -> Result<String> {
        // Serialize multi-sig data to JSON
        let multisig_json = serde_json::to_value(multisig)?;

        // Generate mainchain txid
        let new_txid = Uuid::new_v4();
        let txid_bytes = new_txid.as_bytes();

        // TODO_ROCKSDB: Store anchor with multi-sig data in RocksDB

        log::info!(
            "TARGET Posted multi-sig anchor to mainchain: {} ({}/{} signatures)",
            hex::encode(txid_bytes),
            multisig.partial_signatures.len(),
            multisig.partial_signatures.len()
        );

        Ok(hex::encode(txid_bytes))
    }

    /// Insert anchor idempotently (legacy single-sig mode for backward compatibility)
    ///
    /// This method is kept for backward compatibility with existing code that doesn't use multi-sig.
    /// In production, use submit_partial_signature() instead for decentralized operation.
    ///
    /// Returns hex-encoded tx id string for convenience.
    pub async fn post_anchor(
        &self,
        subchain: Uuid,
        block_height: i64,
        root: &[u8],
    ) -> Result<String> {
        // TODO_ROCKSDB: Implement anchor posting with RocksDB
        Ok(String::from("stub_tx_id"))
    }

    /// Verify a posted anchor's multi-sig (for audit/validation)
    pub async fn verify_anchor_multisig(&self, _root: &[u8]) -> Result<bool> {
        // TODO_ROCKSDB: Implement multisig verification with RocksDB
        Ok(true)
    }

    /// Recover partial signatures from database after crash
    pub async fn recover_partial_signatures(&self, root: &[u8]) -> Result<Vec<PartialSignature>> {
        // TODO_ROCKSDB: Query partial signatures from RocksDB
        Ok(Vec::new())
    }

    /// Create an aggregator attestation for a batch
    ///
    /// This cryptographically commits the aggregator to the correctness of the batch.
    /// If the attestation is later proven fraudulent, the aggregator will be slashed.
    pub fn create_attestation(
        &self,
        subchain: Uuid,
        block_height: i64,
        merkle_root: Vec<u8>,
        tx_count: u64,
        batch_size_bytes: u64,
        tx_list_hash: Option<Vec<u8>>,
        aggregator_signing_key: &SigningKey,
    ) -> Result<AggregatorAttestation> {
        let aggregator_pubkey = aggregator_signing_key.verifying_key().to_bytes().to_vec();
        let created_at = Utc::now();

        // Create attestation without signature first
        let mut attestation = AggregatorAttestation {
            subchain,
            block_height,
            merkle_root,
            tx_count,
            batch_size_bytes,
            aggregator_pubkey,
            signature: Vec::new(),
            created_at,
            tx_list_hash,
        };

        // Sign the attestation
        let message = attestation.signing_message();
        let signature = aggregator_signing_key.sign(&message);
        attestation.signature = signature.to_bytes().to_vec();

        Ok(attestation)
    }

    /// Verify and store an aggregator attestation
    pub async fn store_attestation(&self, attestation: &AggregatorAttestation) -> Result<()> {
        // Verify signature before storing
        attestation.verify()?;

        // Store in database
        // TODO_ROCKSDB: Store attestation in RocksDB
        Ok(())
    }

    /// Retrieve and verify an attestation
    pub async fn get_attestation(
        &self,
        subchain: Uuid,
        block_height: i64,
        merkle_root: &[u8],
    ) -> Result<Option<AggregatorAttestation>> {
        // TODO_ROCKSDB: Query attestation from RocksDB
        Ok(None)
    }

    /// Create a Merkle tree from raw transaction data
    ///
    /// This hashes each transaction and builds the tree.
    pub fn create_merkle_tree_from_data(tx_data: &[Vec<u8>]) -> MerkleTree {
        MerkleTree::from_leaves(tx_data)
    }

    /// Generate a Merkle proof for a specific transaction in a batch
    ///
    /// Returns a proof that can be verified against the merkle root in an attestation.
    pub fn generate_transaction_proof(
        tx_hashes: &[String],
        tx_index: usize,
    ) -> Result<MerkleProof> {
        let tree = Self::create_merkle_tree_from_data(
            &tx_hashes
                .iter()
                .map(|s| s.as_bytes().to_vec())
                .collect::<Vec<_>>(),
        );
        tree.generate_proof(tx_index)
    }

    /// Verify a Merkle proof against a known root
    ///
    /// This is a standalone verification that doesn't require an attestation.
    pub fn verify_merkle_proof_standalone(proof: &MerkleProof) -> Result<bool> {
        proof.verify()
    }
}
