use crate::PgPool;
// src/subchain/registry.rs
// Subchain Registry and Rent System
//
// Manages subchain lifecycle, rent payments, and discovery.
// Subchains must pay rent to remain active on the network.

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;

/// Rent rate per block (in OURO smallest units)
/// 0.01 OURO per block - with 5,000 OURO deposit = ~58 days runway
pub const RENT_RATE_PER_BLOCK: u64 = 1_000_000; // 0.01 OURO

/// Minimum deposit required to create a subchain (in OURO smallest units)
/// 5,000 OURO = 0.005% of 100M supply, encourages L2 ecosystem growth
pub const MIN_SUBCHAIN_DEPOSIT: u64 = 500_000_000_000; // 5,000 OURO

/// Grace period before subchain termination (in blocks)
/// 1,440 blocks (~2 hours at 5 sec/block)
pub const GRACE_PERIOD_BLOCKS: u64 = 1_440;

/// Calculate rent duration for a given deposit
/// Returns number of blocks the deposit will cover
pub fn calculate_rent_duration(deposit_amount: u64) -> u64 {
 deposit_amount / RENT_RATE_PER_BLOCK
}

/// Calculate rent cost for a given number of blocks
pub fn calculate_rent_cost(num_blocks: u64) -> u64 {
 num_blocks * RENT_RATE_PER_BLOCK
}

/// Subchain lifecycle state
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum SubchainState {
 /// Active and operational
 Active,
 /// In grace period (rent depleted, awaiting top-up or termination)
 GracePeriod,
 /// Terminated (rent expired, no longer operational)
 Terminated,
}

impl std::fmt::Display for SubchainState {
 fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
 match self {
 SubchainState::Active => write!(f, "active"),
 SubchainState::GracePeriod => write!(f, "grace_period"),
 SubchainState::Terminated => write!(f, "terminated"),
 }
 }
}

impl std::str::FromStr for SubchainState {
 type Err = anyhow::Error;

 fn from_str(s: &str) -> Result<Self> {
 match s {
 "active" => Ok(SubchainState::Active),
 "grace_period" => Ok(SubchainState::GracePeriod),
 "terminated" => Ok(SubchainState::Terminated),
 _ => bail!("Invalid subchain state: {}", s),
 }
 }
}

/// Subchain registration information
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubchainInfo {
 /// Unique subchain ID
 pub id: Uuid,

 /// Human-readable name
 pub name: String,

 /// Owner address (who pays rent)
 pub owner_address: String,

 /// Current rent deposit balance (in OURO smallest units)
 pub deposit_balance: u64,

 /// Block height when rent was last charged
 pub last_rent_block: u64,

 /// Current lifecycle state
 pub state: SubchainState,

 /// How often subchain anchors to mainchain (in blocks)
 pub anchor_frequency: u64,

 /// RPC endpoint for subchain access
 pub rpc_endpoint: Option<String>,

 /// When subchain was registered
 pub registered_at: DateTime<Utc>,

 /// When subchain entered grace period (if applicable)
 pub grace_period_start: Option<DateTime<Utc>>,

 /// Block height when grace period started
 pub grace_period_start_block: Option<u64>,

 /// Total blocks served since registration
 pub total_blocks_served: u64,

 /// Total rent paid
 pub total_rent_paid: u64,
}

impl SubchainInfo {
 /// Check if subchain can still operate (has sufficient rent)
 pub fn can_operate(&self) -> bool {
 self.state == SubchainState::Active || self.state == SubchainState::GracePeriod
 }

 /// Check if grace period has expired
 pub fn is_grace_period_expired(&self, current_block: u64) -> bool {
 if let Some(grace_start_block) = self.grace_period_start_block {
 current_block >= grace_start_block + GRACE_PERIOD_BLOCKS
 } else {
 false
 }
 }

 /// Calculate remaining rent balance in blocks
 pub fn remaining_blocks(&self) -> u64 {
 if self.deposit_balance >= RENT_RATE_PER_BLOCK {
 self.deposit_balance / RENT_RATE_PER_BLOCK
 } else {
 0
 }
 }

 /// Estimate when rent will run out
 pub fn estimated_expiry_block(&self, current_block: u64) -> u64 {
 current_block + self.remaining_blocks()
 }
}

/// Subchain registry manager
pub struct SubchainRegistry {
 pg: PgPool,
 /// In-memory cache of active subchains
 active_subchains: Arc<RwLock<HashMap<Uuid, SubchainInfo>>>,
}

impl SubchainRegistry {
 pub fn new(pg: PgPool) -> Self {
 Self {
 pg,
 active_subchains: Arc::new(RwLock::new(HashMap::new())),
 }
 }

 /// Register a new subchain
 ///
 /// Requirements:
 /// - Deposit >= MIN_SUBCHAIN_DEPOSIT
 /// - Unique name
 /// - Valid owner address
 pub async fn register_subchain(
 &self,
 name: String,
 owner_address: String,
 deposit_amount: u64,
 anchor_frequency: u64,
 rpc_endpoint: Option<String>,
 current_block: u64,
 ) -> Result<Uuid> {
 // Validate deposit
 if deposit_amount < MIN_SUBCHAIN_DEPOSIT {
 bail!(
 "Insufficient deposit: {} OURO < 300 OURO minimum",
 deposit_amount as f64 / 100_000_000.0
 );
 }

 // Validate name
 if name.is_empty() || name.len() > 64 {
 bail!("Invalid subchain name (must be 1-64 characters)");
 }

 // Check if name already exists
        // TODO_ROCKSDB: Check and register subchain with RocksDB
        let subchain_id = Uuid::new_v4();
        Ok(subchain_id)
 }

    pub async fn get_subchain_by_name(&self, _name: &str) -> Result<Option<SubchainInfo>> {
        // TODO_ROCKSDB: Query subchain by name from RocksDB
        Ok(None)
 }

    /// Collect rent from all subchains for a given block height
    pub async fn collect_rent_for_block(&self, _block_height: u64) -> Result<u64> {
        // TODO_ROCKSDB: Implement rent collection from RocksDB
        Ok(0)
    }
 }

