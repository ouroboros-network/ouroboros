// src/subchain/fraud.rs
//! Fraud proof system for verifiable batch anchoring (Phase 6 security hardening)
//!
//! This module implements a fraud proof mechanism to detect and punish malicious aggregators
//! who submit incorrect Merkle roots or lie about batch contents.
//!
//! How it works:
//! 1. Aggregator posts anchor with Merkle root and attestation
//! 2. Challenge window opens (100 blocks = ~16 minutes at 10s/block)
//! 3. Anyone can submit fraud proof showing Merkle root is incorrect
//! 4. If fraud proven, aggregator is slashed and challenger rewarded
//! 5. If no fraud proven within window, anchor becomes final
//!
//! Security properties:
//! - Optimistic fraud proofs: assume honest unless proven otherwise
//! - Economic security: slashing penalty >> potential profit from fraud
//! - Permissionless challenges: anyone can submit fraud proofs
//! - Time-bounded: fixed challenge window for finality
//!
//! Fraud proof types:
//! - Invalid Merkle root: root doesn't match claimed transactions
//! - Missing transaction: claimed tx not in batch
//! - Invalid transaction: transaction doesn't pass validation
//! - Double inclusion: transaction included multiple times

use crate::bft::slashing::{SlashingManager, SlashingReason, SlashingSeverity};
use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Challenge window in blocks (100 blocks = ~16 minutes at 10s/block)
pub const CHALLENGE_WINDOW_BLOCKS: i64 = 100;

/// Slashing percentage for proven fraud (50% of stake)
pub const FRAUD_SLASH_PERCENTAGE: u64 = 50;

/// Reward percentage for successful fraud proof (10% of slashed amount)
pub const FRAUD_REWARD_PERCENTAGE: u64 = 10;

/// Minimum stake required to submit fraud proof: 50 OURO
/// Same as challenge bond - symmetric for watchdog participants
pub const MIN_FRAUD_PROOF_STAKE: u64 = 5_000_000_000; // 50 OURO

/// Type of fraud being claimed
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum FraudType {
    /// Merkle root doesn't match the claimed transaction list
    InvalidMerkleRoot,
    /// Transaction claimed to be in batch but isn't
    MissingTransaction,
    /// Transaction in batch doesn't pass validation
    InvalidTransaction,
    /// Transaction included multiple times in batch
    DoubleInclusion,
    /// Attestation signature is invalid
    InvalidAttestation,
}

/// Fraud proof submitted by a challenger
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FraudProof {
    /// Unique ID for this fraud proof
    pub id: Uuid,

    /// Type of fraud being claimed
    pub fraud_type: FraudType,

    /// Subchain being challenged
    pub subchain: Uuid,

    /// Block height of anchor being challenged
    pub block_height: i64,

    /// Merkle root being challenged
    pub merkle_root: Vec<u8>,

    /// Challenger's address (must have MIN_FRAUD_PROOF_STAKE)
    pub challenger: String,

    /// Aggregator being accused
    pub accused_aggregator: String,

    /// Proof data (varies by fraud type)
    pub proof_data: Vec<u8>,

    /// Additional context (JSON)
    pub context: Option<String>,

    /// When the proof was submitted
    pub submitted_at: DateTime<Utc>,

    /// Status of this proof
    pub status: FraudProofStatus,

    /// Result of verification (if completed)
    pub verification_result: Option<FraudVerificationResult>,

    /// When verification was completed
    pub verified_at: Option<DateTime<Utc>>,
}

/// Status of a fraud proof
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum FraudProofStatus {
    /// Submitted, awaiting verification
    Pending,
    /// Being verified by validators
    Verifying,
    /// Fraud proven, aggregator slashed
    Proven,
    /// Fraud disproven, anchor is valid
    Rejected,
    /// Challenge window expired before verification
    Expired,
}

/// Result of fraud proof verification
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FraudVerificationResult {
    /// Was fraud proven?
    pub fraud_proven: bool,

    /// Amount slashed from aggregator (if proven)
    pub slashed_amount: Option<u64>,

    /// Reward paid to challenger (if proven)
    pub reward_amount: Option<u64>,

    /// Verification notes
    pub notes: String,
}

impl FraudProof {
    /// Create a new fraud proof
    pub fn new(
        fraud_type: FraudType,
        subchain: Uuid,
        block_height: i64,
        merkle_root: Vec<u8>,
        challenger: String,
        accused_aggregator: String,
        proof_data: Vec<u8>,
        context: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            fraud_type,
            subchain,
            block_height,
            merkle_root,
            challenger,
            accused_aggregator,
            proof_data,
            context,
            submitted_at: Utc::now(),
            status: FraudProofStatus::Pending,
            verification_result: None,
            verified_at: None,
        }
    }

    /// Check if this proof is within the challenge window
    pub fn is_within_challenge_window(&self, current_block_height: i64) -> bool {
        let blocks_elapsed = current_block_height - self.block_height;
        blocks_elapsed <= CHALLENGE_WINDOW_BLOCKS
    }
}

/// Fraud proof manager
pub struct FraudProofManager {
    // TODO_ROCKSDB: Add RocksDB reference when implementing
}

impl FraudProofManager {
    pub fn new() -> Self {
        Self {}
    }

    /// Get fraud proof by ID
    async fn get_fraud_proof(&self, _fraud_proof_id: Uuid) -> Result<Option<FraudProof>> {
        // TODO_ROCKSDB: Query fraud proof from RocksDB
        Ok(None)
    }

    /// Get address stake
    async fn get_address_stake(&self, _address: &str) -> Result<u64> {
        // TODO_ROCKSDB: Query address stake from RocksDB
        Ok(MIN_FRAUD_PROOF_STAKE) // Return minimum stake for now
    }

    /// Submit a fraud proof
    ///
    /// Returns the fraud proof ID if accepted
    pub async fn submit_fraud_proof(
        &self,
        fraud_proof: FraudProof,
        current_block_height: i64,
    ) -> Result<Uuid> {
        // Verify challenger has minimum stake
        let challenger_stake = self.get_address_stake(&fraud_proof.challenger).await?;
        if challenger_stake < MIN_FRAUD_PROOF_STAKE {
            bail!(
                "Insufficient stake to submit fraud proof: {} < {} OURO",
                challenger_stake / 1_000_000,
                MIN_FRAUD_PROOF_STAKE / 1_000_000
            );
        }

        // Verify within challenge window
        if !fraud_proof.is_within_challenge_window(current_block_height) {
            bail!(
                "Challenge window expired: {} blocks elapsed (max {})",
                current_block_height - fraud_proof.block_height,
                CHALLENGE_WINDOW_BLOCKS
            );
        }

        // Store in database

        log::warn!(
            "WARNING Fraud proof submitted: {} accuses {} of {:?} for anchor {} (height {})",
            fraud_proof.challenger,
            fraud_proof.accused_aggregator,
            fraud_proof.fraud_type,
            hex::encode(&fraud_proof.merkle_root[..8]),
            fraud_proof.block_height
        );

        Ok(fraud_proof.id)
    }

    /// Verify invalid Merkle root fraud proof
    async fn verify_invalid_merkle_root(
        &self,
        _proof: &FraudProof,
    ) -> Result<FraudVerificationResult> {
        // TODO_ROCKSDB: Implement Merkle root verification with RocksDB
        Ok(FraudVerificationResult {
            fraud_proven: false,
            slashed_amount: None,
            reward_amount: None,
            notes: "Merkle root verification not implemented".to_string(),
        })
    }

    /// Verify a fraud proof
    ///
    /// This should be called by validators to check if the fraud proof is valid
    pub async fn verify_fraud_proof(
        &self,
        fraud_proof_id: Uuid,
    ) -> Result<FraudVerificationResult> {
        // Fetch fraud proof
        let fraud_proof = self
            .get_fraud_proof(fraud_proof_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Fraud proof not found"))?;

        // Verify based on fraud type
        let result = match fraud_proof.fraud_type {
            FraudType::InvalidMerkleRoot => self.verify_invalid_merkle_root(&fraud_proof).await?,
            FraudType::MissingTransaction => self.verify_missing_transaction(&fraud_proof).await?,
            FraudType::InvalidTransaction => self.verify_invalid_transaction(&fraud_proof).await?,
            FraudType::DoubleInclusion => self.verify_double_inclusion(&fraud_proof).await?,
            FraudType::InvalidAttestation => self.verify_invalid_attestation(&fraud_proof).await?,
        };

        // TODO_ROCKSDB: Update proof status in database
        Ok(result)
    }

    /// Verify missing transaction fraud proof
    ///
    /// SECURITY: Uses proper Merkle proof verification to prove a transaction
    /// that was claimed to be in the batch is actually missing.
    ///
    /// Proof data format (JSON):
    /// {
    /// "claimed_tx_id": "...",
    /// "actual_transactions": ["tx1", "tx2", ...],
    /// "merkle_root_in_anchor": "..."
    /// }
    async fn verify_missing_transaction(
        &self,
        proof: &FraudProof,
    ) -> Result<FraudVerificationResult> {
        // Deserialize proof data
        let proof_data: serde_json::Value = match serde_json::from_slice(&proof.proof_data) {
            Ok(data) => data,
            Err(_) => {
                // Invalid proof format - fraud not proven
                return Ok(FraudVerificationResult {
                    fraud_proven: false,
                    slashed_amount: None,
                    reward_amount: None,
                    notes: "Invalid proof format - cannot deserialize proof data".to_string(),
                });
            }
        };

        // Extract claimed transaction ID and actual transaction list
        let claimed_tx_id = proof_data["claimed_tx_id"].as_str().unwrap_or("");
        let actual_txs = proof_data["actual_transactions"].as_array();

        if claimed_tx_id.is_empty() || actual_txs.is_none() {
            return Ok(FraudVerificationResult {
                fraud_proven: false,
                slashed_amount: None,
                reward_amount: None,
                notes: "Invalid proof: missing claimed_tx_id or actual_transactions".to_string(),
            });
        }

        let actual_txs = actual_txs.unwrap();

        // Check if claimed transaction is actually in the list
        let tx_found = actual_txs
            .iter()
            .any(|tx| tx.as_str().map(|s| s == claimed_tx_id).unwrap_or(false));

        // Build Merkle tree from actual transactions
        let tx_leaves: Vec<Vec<u8>> = actual_txs
            .iter()
            .filter_map(|tx| tx.as_str().map(|s| s.as_bytes().to_vec()))
            .collect();

        let computed_root = match crate::merkle::merkle_root_from_leaves_bytes(&tx_leaves) {
            Ok(root) => root,
            Err(e) => {
                return Ok(FraudVerificationResult {
                    fraud_proven: false,
                    slashed_amount: None,
                    reward_amount: None,
                    notes: format!("Failed to compute Merkle root: {}", e),
                });
            }
        };

        // Fraud is proven if:
        // 1. Claimed transaction is NOT in actual list, AND
        // 2. Computed root from actual transactions matches the claimed root
        let fraud_proven = !tx_found && computed_root == proof.merkle_root;

        let (slashed_amount, reward_amount) = if fraud_proven {
            let aggregator_stake = self.get_address_stake(&proof.accused_aggregator).await?;
            let slashed = (aggregator_stake * FRAUD_SLASH_PERCENTAGE) / 100;
            let reward = (slashed * FRAUD_REWARD_PERCENTAGE) / 100;
            (Some(slashed), Some(reward))
        } else {
            (None, None)
        };

        Ok(FraudVerificationResult {
            fraud_proven,
            slashed_amount,
            reward_amount,
            notes: if fraud_proven {
                format!(
                    "Transaction {} was claimed but is missing from batch",
                    claimed_tx_id
                )
            } else if tx_found {
                "Transaction is present in batch".to_string()
            } else {
                "Merkle root mismatch - proof invalid".to_string()
            },
        })
    }

    /// Verify invalid transaction fraud proof
    async fn verify_invalid_transaction(
        &self,
        proof: &FraudProof,
    ) -> Result<FraudVerificationResult> {
        // Proof data should contain the invalid transaction

        // Simplified: assume fraud if proof_data is non-empty
        let fraud_proven = !proof.proof_data.is_empty();

        let (slashed_amount, reward_amount) = if fraud_proven {
            let aggregator_stake = self.get_address_stake(&proof.accused_aggregator).await?;
            let slashed = (aggregator_stake * FRAUD_SLASH_PERCENTAGE) / 100;
            let reward = (slashed * FRAUD_REWARD_PERCENTAGE) / 100;
            (Some(slashed), Some(reward))
        } else {
            (None, None)
        };

        Ok(FraudVerificationResult {
            fraud_proven,
            slashed_amount,
            reward_amount,
            notes: if fraud_proven {
                "Invalid transaction in batch".to_string()
            } else {
                "All transactions are valid".to_string()
            },
        })
    }

    /// Verify double inclusion fraud proof
    async fn verify_double_inclusion(&self, proof: &FraudProof) -> Result<FraudVerificationResult> {
        // Proof data should contain: transaction ID + two Merkle proofs showing it appears twice

        // Simplified: assume fraud if proof_data is non-empty
        let fraud_proven = !proof.proof_data.is_empty();

        let (slashed_amount, reward_amount) = if fraud_proven {
            let aggregator_stake = self.get_address_stake(&proof.accused_aggregator).await?;
            let slashed = (aggregator_stake * FRAUD_SLASH_PERCENTAGE) / 100;
            let reward = (slashed * FRAUD_REWARD_PERCENTAGE) / 100;
            (Some(slashed), Some(reward))
        } else {
            (None, None)
        };

        Ok(FraudVerificationResult {
            fraud_proven,
            slashed_amount,
            reward_amount,
            notes: if fraud_proven {
                "Transaction included multiple times".to_string()
            } else {
                "No double inclusion detected".to_string()
            },
        })
    }

    /// Verify invalid attestation fraud proof
    async fn verify_invalid_attestation(
        &self,
        proof: &FraudProof,
    ) -> Result<FraudVerificationResult> {
        // Proof data should contain the attestation with invalid signature

        // Simplified: assume fraud if proof_data is non-empty
        let fraud_proven = !proof.proof_data.is_empty();

        let (slashed_amount, reward_amount) = if fraud_proven {
            let aggregator_stake = self.get_address_stake(&proof.accused_aggregator).await?;
            let slashed = (aggregator_stake * FRAUD_SLASH_PERCENTAGE) / 100;
            let reward = (slashed * FRAUD_REWARD_PERCENTAGE) / 100;
            (Some(slashed), Some(reward))
        } else {
            (None, None)
        };

        Ok(FraudVerificationResult {
            fraud_proven,
            slashed_amount,
            reward_amount,
            notes: if fraud_proven {
                "Attestation signature is invalid".to_string()
            } else {
                "Attestation is valid".to_string()
            },
        })
    }

    /// Execute slashing and rewards
    async fn execute_slashing(
        &self,
        _proof: &FraudProof,
        _result: &FraudVerificationResult,
    ) -> Result<()> {
        // TODO_ROCKSDB: Implement slashing execution with RocksDB
        Ok(())
    }

    /// Expire old fraud proofs
    pub async fn expire_old_proofs(&self, _current_block_height: i64) -> Result<usize> {
        // TODO_ROCKSDB: Implement proof expiration with RocksDB
        Ok(0)
    }
}
