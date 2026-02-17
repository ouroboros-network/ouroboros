// src/subchain/registry.rs
//! Subchain Registry with Rent System and Market Discovery
//!
//! Manages subchain lifecycle including:
//! - Registration and state tracking
//! - Rent collection and grace periods
//! - Market discovery (Medium nodes advertise, Light nodes discover)

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

// ─── Rent System ───────────────────────────────────────────────────

/// State of a registered subchain
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SubchainState {
    /// Active and processing transactions
    Active,
    /// Rent overdue, in grace period
    GracePeriod,
    /// Suspended due to non-payment
    Suspended,
    /// Permanently deregistered
    Deregistered,
}

/// Information about a registered subchain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubchainInfo {
    /// Unique subchain identifier
    pub id: String,
    /// Owner/operator address
    pub owner: String,
    /// Current state
    pub state: SubchainState,
    /// Block height when registered
    pub registered_at_block: u64,
    /// Rent paid up to this block height
    pub rent_paid_until_block: u64,
    /// Rent cost per block (in smallest OURO units)
    pub rent_per_block: u64,
    /// Grace period in blocks before suspension
    pub grace_period_blocks: u64,
    /// Total rent collected lifetime
    pub total_rent_paid: u64,
}

impl SubchainInfo {
    /// Create a new subchain registration
    pub fn new(id: String, owner: String, current_block: u64) -> Self {
        Self {
            id,
            owner,
            state: SubchainState::Active,
            registered_at_block: current_block,
            rent_paid_until_block: current_block,
            rent_per_block: 1_000, // 0.00001 OURO per block
            grace_period_blocks: 8_640, // ~1 day at 10s/block
            total_rent_paid: 0,
        }
    }

    /// Check if rent is overdue at the given block height
    pub fn is_rent_overdue(&self, current_block: u64) -> bool {
        current_block > self.rent_paid_until_block
    }

    /// Check if past grace period
    pub fn is_past_grace_period(&self, current_block: u64) -> bool {
        current_block > self.rent_paid_until_block + self.grace_period_blocks
    }
}

// ─── Market Discovery ──────────────────────────────────────────────

/// Advertisement from a Medium node for subchain discovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubchainAdvertisement {
    pub subchain_id: String,
    pub aggregator_node_id: String,
    pub aggregator_addr: String,
    pub app_type: String, // e.g. "gaming", "finance", "storage"
    pub capacity_percent: u8,
    pub last_seen: DateTime<Utc>,
    pub reputation_score: f64,
}

// ─── Registry ──────────────────────────────────────────────────────

/// Combined subchain registry with rent management and market discovery
pub struct SubchainRegistry {
    /// Registered subchains (rent system)
    subchains: Arc<RwLock<HashMap<String, SubchainInfo>>>,
    /// Market advertisements (discovery system)
    pub advertisements: Arc<RwLock<HashMap<String, SubchainAdvertisement>>>,
}

impl SubchainRegistry {
    pub fn new() -> Self {
        Self {
            subchains: Arc::new(RwLock::new(HashMap::new())),
            advertisements: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    // ─── Rent System Methods ───────────────────────────────────────

    /// Register a new subchain
    pub fn register_subchain(
        &self,
        id: String,
        owner: String,
        current_block: u64,
    ) -> Result<SubchainInfo> {
        let mut subchains = self.subchains.write().unwrap();
        if subchains.contains_key(&id) {
            return Err(anyhow::anyhow!("Subchain {} already registered", id));
        }
        let info = SubchainInfo::new(id.clone(), owner, current_block);
        subchains.insert(id, info.clone());
        Ok(info)
    }

    /// Pay rent for a subchain (extends rent_paid_until_block)
    pub fn pay_rent(
        &self,
        subchain_id: &str,
        payment_amount: u64,
    ) -> Result<u64> {
        let mut subchains = self.subchains.write().unwrap();
        let info = subchains
            .get_mut(subchain_id)
            .ok_or_else(|| anyhow::anyhow!("Subchain not found"))?;

        if info.rent_per_block == 0 {
            return Err(anyhow::anyhow!("Rent per block is zero"));
        }

        let blocks_covered = payment_amount / info.rent_per_block;
        info.rent_paid_until_block += blocks_covered;
        info.total_rent_paid += payment_amount;

        // Reactivate if was in grace period or suspended
        if info.state == SubchainState::GracePeriod || info.state == SubchainState::Suspended {
            info.state = SubchainState::Active;
        }

        Ok(info.rent_paid_until_block)
    }

    /// Collect rent for a given block height
    /// Returns total rent collected across all subchains
    pub async fn collect_rent_for_block(&self, current_block: u64) -> Result<u64> {
        let mut subchains = self.subchains.write().unwrap();
        let mut total_collected: u64 = 0;

        for info in subchains.values_mut() {
            match info.state {
                SubchainState::Active => {
                    if info.is_past_grace_period(current_block) {
                        info.state = SubchainState::Suspended;
                        log::warn!(
                            "Subchain {} suspended (rent overdue since block {})",
                            info.id,
                            info.rent_paid_until_block
                        );
                    } else if info.is_rent_overdue(current_block) {
                        info.state = SubchainState::GracePeriod;
                        log::info!(
                            "Subchain {} entered grace period (rent overdue)",
                            info.id
                        );
                    } else {
                        // Rent is current, collect for this block
                        total_collected += info.rent_per_block;
                    }
                }
                SubchainState::GracePeriod => {
                    if info.is_past_grace_period(current_block) {
                        info.state = SubchainState::Suspended;
                        log::warn!("Subchain {} suspended after grace period", info.id);
                    }
                }
                _ => {}
            }
        }

        Ok(total_collected)
    }

    /// Get information about a specific subchain
    pub fn get_subchain(&self, id: &str) -> Option<SubchainInfo> {
        let subchains = self.subchains.read().unwrap();
        subchains.get(id).cloned()
    }

    /// List all registered subchains
    pub fn list_subchains(&self) -> Vec<SubchainInfo> {
        let subchains = self.subchains.read().unwrap();
        subchains.values().cloned().collect()
    }

    /// Get active subchain count
    pub fn active_count(&self) -> usize {
        let subchains = self.subchains.read().unwrap();
        subchains
            .values()
            .filter(|s| s.state == SubchainState::Active)
            .count()
    }

    // ─── Market Discovery Methods ──────────────────────────────────

    /// Register a subchain advertisement (Medium nodes)
    pub fn advertise(&self, ad: SubchainAdvertisement) {
        let mut ads = self.advertisements.write().unwrap();
        ads.insert(ad.subchain_id.clone(), ad);
    }

    /// Discover subchains by application type (Light nodes)
    pub fn discover(&self, app_type: &str) -> Vec<SubchainAdvertisement> {
        let ads = self.advertisements.read().unwrap();
        ads.values()
            .filter(|ad| ad.app_type == app_type)
            .cloned()
            .collect()
    }

    /// Get all advertisements
    pub fn get_all(&self) -> Vec<SubchainAdvertisement> {
        let ads = self.advertisements.read().unwrap();
        ads.values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_subchain() {
        let registry = SubchainRegistry::new();
        let info = registry
            .register_subchain("sub1".into(), "owner1".into(), 100)
            .unwrap();
        assert_eq!(info.state, SubchainState::Active);
        assert_eq!(info.rent_paid_until_block, 100);
    }

    #[test]
    fn test_duplicate_registration_rejected() {
        let registry = SubchainRegistry::new();
        registry
            .register_subchain("sub1".into(), "owner1".into(), 100)
            .unwrap();
        assert!(registry
            .register_subchain("sub1".into(), "owner2".into(), 200)
            .is_err());
    }

    #[test]
    fn test_pay_rent() {
        let registry = SubchainRegistry::new();
        registry
            .register_subchain("sub1".into(), "owner1".into(), 100)
            .unwrap();

        // Pay for 100 blocks (100 * 1000 = 100_000 units)
        let paid_until = registry.pay_rent("sub1", 100_000).unwrap();
        assert_eq!(paid_until, 200); // 100 + 100 blocks

        let info = registry.get_subchain("sub1").unwrap();
        assert_eq!(info.total_rent_paid, 100_000);
    }

    #[test]
    fn test_rent_overdue_detection() {
        let info = SubchainInfo::new("sub1".into(), "owner".into(), 100);
        assert!(!info.is_rent_overdue(100));
        assert!(info.is_rent_overdue(101));
        assert!(!info.is_past_grace_period(101));
        assert!(info.is_past_grace_period(100 + 8_640 + 1));
    }

    #[tokio::test]
    async fn test_collect_rent_suspends_overdue() {
        let registry = SubchainRegistry::new();
        registry
            .register_subchain("sub1".into(), "owner1".into(), 100)
            .unwrap();

        // Collect at block well past grace period
        let _ = registry.collect_rent_for_block(100 + 8_640 + 10).await;

        let info = registry.get_subchain("sub1").unwrap();
        assert_eq!(info.state, SubchainState::Suspended);
    }

    #[test]
    fn test_market_advertise_and_discover() {
        let registry = SubchainRegistry::new();
        registry.advertise(SubchainAdvertisement {
            subchain_id: "sub1".into(),
            aggregator_node_id: "node1".into(),
            aggregator_addr: "localhost:8001".into(),
            app_type: "gaming".into(),
            capacity_percent: 80,
            last_seen: Utc::now(),
            reputation_score: 1.0,
        });

        let gaming = registry.discover("gaming");
        assert_eq!(gaming.len(), 1);
        assert_eq!(gaming[0].aggregator_node_id, "node1");

        let finance = registry.discover("finance");
        assert!(finance.is_empty());
    }
}
