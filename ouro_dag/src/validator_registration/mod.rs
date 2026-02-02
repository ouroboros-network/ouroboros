use crate::PgPool;
// src/validator_registration/mod.rs
// Permissionless Validator Registration System
//
// Allows anyone to become a validator by:
// 1. Staking minimum OURO tokens (10,000 OURO minimum)
// 2. Posting a bond (slashable for misbehavior)
// 3. Providing BFT public key and network endpoint
// 4. Automatic inclusion after stake confirmation
//
// Features:
// - Stake-based permissionless registration
// - Slashing for Byzantine behavior
// - Reputation tracking
// - Automatic validator set updates
// - Graceful validator exit with unbonding period

pub mod api;

use anyhow::{bail, Result};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

/// Minimum stake required to become a validator (in OURO microunits)
/// 10,000 OURO = 0.01% of 100M supply, allows ~10,000 validators max
pub const MIN_VALIDATOR_STAKE: u64 = 1_000_000_000_000; // 10,000 OURO

/// Unbonding period after validator exit (in days)
pub const UNBONDING_PERIOD_DAYS: i64 = 14;

/// Validator status in the registry
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ValidatorStatus {
    /// Pending activation (stake confirmed but not yet active in consensus)
    Pending,
    /// Active validator participating in consensus
    Active,
    /// Unbonding (exit requested, waiting for unbonding period)
    Unbonding,
    /// Slashed for misbehavior
    Slashed,
    /// Exited (unbonding complete, stake returned)
    Exited,
}

impl std::fmt::Display for ValidatorStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidatorStatus::Pending => write!(f, "pending"),
            ValidatorStatus::Active => write!(f, "active"),
            ValidatorStatus::Unbonding => write!(f, "unbonding"),
            ValidatorStatus::Slashed => write!(f, "slashed"),
            ValidatorStatus::Exited => write!(f, "exited"),
        }
    }
}

impl std::str::FromStr for ValidatorStatus {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "pending" => Ok(ValidatorStatus::Pending),
            "active" => Ok(ValidatorStatus::Active),
            "unbonding" => Ok(ValidatorStatus::Unbonding),
            "slashed" => Ok(ValidatorStatus::Slashed),
            "exited" => Ok(ValidatorStatus::Exited),
            _ => bail!("Invalid validator status: {}", s),
        }
    }
}

/// Information about a registered validator
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ValidatorInfo {
    /// Unique validator ID
    pub id: Uuid,
    /// Validator address (Ouro address holding the stake)
    pub address: String,
    /// Ed25519 public key for BFT consensus (hex)
    pub bft_pubkey: String,
    /// Network endpoint for P2P communication
    pub network_endpoint: String,
    /// BFT consensus port
    pub bft_port: u16,
    /// Amount staked (in OURO microunits)
    pub stake_amount: u64,
    /// Current status
    pub status: ValidatorStatus,
    /// Reputation score (0-100)
    pub reputation: u8,
    /// Number of blocks proposed
    pub blocks_proposed: u64,
    /// Number of blocks signed
    pub blocks_signed: u64,
    /// Number of missed proposals
    pub missed_proposals: u64,
    /// Total slashing amount (if slashed)
    pub slashed_amount: u64,
    /// Timestamp when registered
    pub registered_at: DateTime<Utc>,
    /// Timestamp when activated (if active)
    pub activated_at: Option<DateTime<Utc>>,
    /// Timestamp when exit requested (if unbonding)
    pub exit_requested_at: Option<DateTime<Utc>>,
    /// Timestamp when unbonding completes
    pub unbonding_complete_at: Option<DateTime<Utc>>,
}

impl ValidatorInfo {
    /// Check if validator is eligible for activation
    pub fn can_activate(&self) -> bool {
        self.status == ValidatorStatus::Pending && self.stake_amount >= MIN_VALIDATOR_STAKE
    }

    /// Check if unbonding period is complete
    pub fn is_unbonding_complete(&self) -> bool {
        if let Some(complete_at) = self.unbonding_complete_at {
            Utc::now() >= complete_at
        } else {
            false
        }
    }

    /// Calculate voting power (proportional to stake)
    pub fn voting_power(&self) -> u64 {
        if self.status == ValidatorStatus::Active {
            self.stake_amount
        } else {
            0
        }
    }
}

/// Slashing reason
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SlashingReason {
    /// Double signing (equivocation)
    DoubleSign,
    /// Offline for extended period
    Downtime,
    /// Invalid block proposal
    InvalidBlock,
    /// Byzantine behavior detected
    Byzantine,
}

impl std::fmt::Display for SlashingReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SlashingReason::DoubleSign => write!(f, "double_sign"),
            SlashingReason::Downtime => write!(f, "downtime"),
            SlashingReason::InvalidBlock => write!(f, "invalid_block"),
            SlashingReason::Byzantine => write!(f, "byzantine"),
        }
    }
}

/// Slashing event record
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SlashingEvent {
    pub validator_id: Uuid,
    pub reason: SlashingReason,
    pub amount_slashed: u64,
    pub evidence: serde_json::Value,
    pub timestamp: DateTime<Utc>,
}

/// Validator registry manager
pub struct ValidatorRegistry {
    pg: PgPool,
    /// In-memory cache of active validators
    active_validators: Arc<RwLock<HashMap<String, ValidatorInfo>>>,
}

impl ValidatorRegistry {
    pub fn new(pg: PgPool) -> Self {
        Self {
            pg,
            active_validators: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a new validator
    ///
    /// Requirements:
    /// - Stake >= MIN_VALIDATOR_STAKE
    /// - Valid BFT public key
    /// - Unique network endpoint
    pub async fn register_validator(
        &self,
        address: String,
        bft_pubkey: String,
        network_endpoint: String,
        bft_port: u16,
        stake_amount: u64,
    ) -> Result<Uuid> {
        // Validate stake amount
        if stake_amount < MIN_VALIDATOR_STAKE {
            bail!(
                "Insufficient stake: {} < 100 OURO",
                stake_amount as f64 / 1_000_000.0
            );
        }

        // Validate BFT public key (should be 64 hex chars for Ed25519)
        if bft_pubkey.len() != 64 {
            bail!("Invalid BFT public key length");
        }

        // TODO_ROCKSDB: Check if address/endpoint already registered and insert validator record
        let validator_id = Uuid::new_v4();

        log::info!(
            "New validator registered: {} ({}) - stake: {} OURO",
            address,
            network_endpoint,
            stake_amount as f64 / 1_000_000.0
        );

        Ok(validator_id)
    }

    /// Activate a pending validator (called after stake confirmation)
    pub async fn activate_validator(&self, _validator_id: Uuid) -> Result<()> {
        // TODO_ROCKSDB: Update validator status to active in RocksDB
        Ok(())
    }

    /// Request validator exit (starts unbonding period)
    pub async fn request_exit(&self, _validator_id: Uuid) -> Result<DateTime<Utc>> {
        let unbonding_complete = Utc::now() + ChronoDuration::days(UNBONDING_PERIOD_DAYS);
        // TODO_ROCKSDB: Update validator status to unbonding in RocksDB
        Ok(unbonding_complete)
    }

    /// Complete validator exit after unbonding period
    pub async fn complete_exit(&self, _validator_id: Uuid) -> Result<u64> {
        // TODO_ROCKSDB: Update validator status and return stake
        Ok(0)
    }

    /// Slash a validator for misbehavior
    pub async fn slash_validator(
        &self,
        _validator_id: Uuid,
        _reason: SlashingReason,
        _slash_percentage: u8,
        _evidence: serde_json::Value,
    ) -> Result<u64> {
        // TODO_ROCKSDB: Implement slashing with RocksDB
        Ok(0)
    }

    /// Get validator info by ID
    pub async fn get_validator(&self, _validator_id: Uuid) -> Result<Option<ValidatorInfo>> {
        // TODO_ROCKSDB: Query validator from RocksDB
        Ok(None)
    }

    /// Get all active validators
    pub async fn get_active_validators(&self) -> Result<Vec<ValidatorInfo>> {
        // TODO_ROCKSDB: Query active validators from RocksDB
        Ok(Vec::new())
    }

    /// Reload active validators cache
    pub async fn reload_active_validators(&self) -> Result<()> {
        // TODO_ROCKSDB: Reload validators from RocksDB
        Ok(())
    }

    /// Get cached active validators
    pub fn get_cached_active_validators(&self) -> Vec<ValidatorInfo> {
        self.active_validators.read().values().cloned().collect()
    }

    /// Record block proposal
    pub async fn record_block_proposal(&self, _validator_id: Uuid) -> Result<()> {
        // TODO_ROCKSDB: Record proposal in RocksDB
        Ok(())
    }

    /// Record block signature
    pub async fn record_block_signature(&self, _validator_id: Uuid) -> Result<()> {
        // TODO_ROCKSDB: Record signature in RocksDB
        Ok(())
    }

    /// Record missed proposal
    pub async fn record_missed_proposal(&self, _validator_id: Uuid) -> Result<()> {
        // TODO_ROCKSDB: Record missed proposal in RocksDB
        Ok(())
    }
}
