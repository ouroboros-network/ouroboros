// src/ouro_coin/economics.rs
//! OURO Token Economics
//!
//! Defines the economic model for OURO including:
//! - Fee distribution
//! - Token allocation
//! - DEMAND-BASED token release (not time-based vesting)

use serde::{Deserialize, Serialize};

/// Fee distribution percentages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeDistribution {
    /// Percentage to validators (70%)
    pub validators: f64,

    /// Percentage to burn (10%)
    pub burn: f64,

    /// Percentage to treasury (10%)
    pub treasury: f64,

    /// Percentage to app developer (10%)
    pub app_developer: f64,
}

impl Default for FeeDistribution {
    fn default() -> Self {
        Self {
            validators: 0.70,    // 70%
            burn: 0.10,          // 10%
            treasury: 0.10,      // 10%
            app_developer: 0.10, // 10%
        }
    }
}

impl FeeDistribution {
    /// Validate that percentages sum to 100%
    pub fn validate(&self) -> bool {
        let sum = self.validators + self.burn + self.treasury + self.app_developer;
        (sum - 1.0).abs() < 0.0001 // Allow small floating point error
    }

    /// Calculate distribution amounts for a given fee
    pub fn distribute(&self, total_fee: u64) -> FeeAllocation {
        FeeAllocation {
            validators_amount: (total_fee as f64 * self.validators) as u64,
            burn_amount: (total_fee as f64 * self.burn) as u64,
            treasury_amount: (total_fee as f64 * self.treasury) as u64,
            app_developer_amount: (total_fee as f64 * self.app_developer) as u64,
        }
    }
}

/// Fee allocation breakdown
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeAllocation {
    pub validators_amount: u64,
    pub burn_amount: u64,
    pub treasury_amount: u64,
    pub app_developer_amount: u64,
}

impl FeeAllocation {
    /// Get total allocated amount
    pub fn total(&self) -> u64 {
        self.validators_amount + self.burn_amount + self.treasury_amount + self.app_developer_amount
    }
}

// ============================================================================
// DEMAND-BASED TOKEN RELEASE
// ============================================================================
//
// OURO uses a DEMAND-BASED release model, NOT time-based vesting.
//
// Total Supply: 103M OURO
// - Initial Circulating: 13M OURO (immediately available)
// - Reserve Pool: 90M OURO (released based on demand)
//
// How it works:
// - Start with 13M OURO in circulation
// - When circulation drops below threshold OR demand is high, unlock next 10M
// - This continues until all 90M reserve is released
// - Maximum possible circulation: 103M OURO
//
// Tranches (9 x 10M = 90M):
// - Tranche 1: Unlocks when circulating < 10M -> brings to 23M
// - Tranche 2: Unlocks when circulating < 20M -> brings to 33M
// - ... and so on
// ============================================================================

/// Token distribution plan (103M OURO total)
///
/// DEMAND-BASED MODEL:
/// - Initial Circulating: 13M OURO (immediately available)
/// - Reserve Pool: 90M OURO (released based on demand in 10M tranches)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenDistribution {
    /// Initial circulating supply (13M OURO) - immediately available
    pub initial_circulating: u64,

    /// Reserve pool (90M OURO) - released based on demand
    pub reserve_pool: u64,

    /// Breakdown of initial circulating (13M):
    /// - Public launch: 5M OURO
    pub public_launch: u64,
    /// - Initial liquidity: 3M OURO
    pub initial_liquidity: u64,
    /// - Early contributors: 3M OURO
    pub early_contributors: u64,
    /// - Marketing/Airdrops: 2M OURO
    pub marketing: u64,

    /// Breakdown of reserve pool (90M) - ALL goes to ecosystem growth:
    /// - Validator rewards: 50M OURO (released as block rewards when needed)
    pub validator_rewards_reserve: u64,
    /// - Development fund: 25M OURO (for protocol development)
    pub development_reserve: u64,
    /// - Treasury/Ecosystem: 15M OURO (governance controlled)
    pub treasury_reserve: u64,
}

impl Default for TokenDistribution {
    fn default() -> Self {
        Self {
            // Total = 103M OURO
            initial_circulating: 13_000_000,
            reserve_pool: 90_000_000,

            // Initial circulating breakdown (13M)
            public_launch: 5_000_000,
            initial_liquidity: 3_000_000,
            early_contributors: 3_000_000,
            marketing: 2_000_000,

            // Reserve breakdown (90M) - NO TEAM/FOUNDERS ALLOCATION
            validator_rewards_reserve: 50_000_000,
            development_reserve: 25_000_000,
            treasury_reserve: 15_000_000,
        }
    }
}

impl TokenDistribution {
    /// Get total allocated tokens
    pub fn total(&self) -> u64 {
        self.initial_circulating + self.reserve_pool
    }

    /// Validate initial circulating breakdown
    pub fn validate_initial(&self) -> bool {
        self.public_launch + self.initial_liquidity + self.early_contributors + self.marketing
            == self.initial_circulating
    }

    /// Validate reserve breakdown
    pub fn validate_reserve(&self) -> bool {
        self.validator_rewards_reserve + self.development_reserve + self.treasury_reserve
            == self.reserve_pool
    }

    /// Validate that allocation equals total supply
    pub fn validate(&self) -> bool {
        self.total() == super::TOTAL_SUPPLY && self.validate_initial() && self.validate_reserve()
    }
}

// ============================================================================
// DEMAND-BASED RELEASE SYSTEM
// ============================================================================

/// Size of each release tranche (10M OURO)
pub const TRANCHE_SIZE: u64 = 10_000_000;

/// Number of tranches in reserve (90M / 10M = 9 tranches)
pub const TOTAL_TRANCHES: u64 = 9;

/// Threshold percentage - unlock when circulating drops below this % of unlocked supply
/// e.g., if 13M unlocked and threshold is 70%, unlock next tranche when circulating < 9.1M
pub const UNLOCK_THRESHOLD_PERCENT: f64 = 0.70;

/// A single release tranche
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseTranche {
    /// Tranche number (1-9)
    pub number: u8,
    /// Amount in this tranche (10M OURO in smallest units)
    pub amount: u64,
    /// Has this tranche been unlocked?
    pub unlocked: bool,
    /// Timestamp when unlocked (0 if not yet)
    pub unlock_time: u64,
    /// Reason for unlock
    pub unlock_reason: Option<String>,
}

/// Demand-based release manager
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemandBasedRelease {
    /// Genesis timestamp
    pub genesis_time: u64,

    /// Initial circulating supply (13M OURO in units)
    pub initial_supply: u64,

    /// Release tranches (9 x 10M = 90M)
    pub tranches: Vec<ReleaseTranche>,

    /// Total amount unlocked from reserve
    pub total_unlocked: u64,

    /// Total amount distributed from unlocked pool
    pub total_distributed: u64,
}

impl DemandBasedRelease {
    /// Create new demand-based release system
    pub fn new(genesis_time: u64) -> Self {
        let unit = super::OURO_UNIT;
        let tranches: Vec<ReleaseTranche> = (1..=TOTAL_TRANCHES as u8)
            .map(|n| ReleaseTranche {
                number: n,
                amount: TRANCHE_SIZE * unit,
                unlocked: false,
                unlock_time: 0,
                unlock_reason: None,
            })
            .collect();

        Self {
            genesis_time,
            initial_supply: 13_000_000 * unit,
            tranches,
            total_unlocked: 0,
            total_distributed: 0,
        }
    }

    /// Get current unlocked supply (initial + unlocked tranches)
    pub fn unlocked_supply(&self) -> u64 {
        self.initial_supply + self.total_unlocked
    }

    /// Get available supply (unlocked - distributed)
    pub fn available_supply(&self) -> u64 {
        self.unlocked_supply()
            .saturating_sub(self.total_distributed)
    }

    /// Get remaining reserve (not yet unlocked)
    pub fn remaining_reserve(&self) -> u64 {
        let total_reserve = 90_000_000 * super::OURO_UNIT;
        total_reserve.saturating_sub(self.total_unlocked)
    }

    /// Count unlocked tranches
    pub fn unlocked_count(&self) -> usize {
        self.tranches.iter().filter(|t| t.unlocked).count()
    }

    /// Check if next tranche should be unlocked based on demand
    /// Returns true if circulation has dropped below threshold
    pub fn should_unlock_next(&self, current_circulating: u64) -> bool {
        // If all tranches unlocked, nothing to do
        if self.unlocked_count() >= TOTAL_TRANCHES as usize {
            return false;
        }

        // Calculate threshold: unlock when circulating < 70% of unlocked supply
        let threshold = (self.unlocked_supply() as f64 * UNLOCK_THRESHOLD_PERCENT) as u64;

        current_circulating < threshold
    }

    /// Unlock the next tranche
    /// Returns the amount unlocked, or error if no tranches available
    pub fn unlock_next_tranche(&mut self, current_time: u64, reason: &str) -> Result<u64, String> {
        // Find next locked tranche
        let next_tranche = self.tranches.iter_mut().find(|t| !t.unlocked);

        match next_tranche {
            Some(tranche) => {
                tranche.unlocked = true;
                tranche.unlock_time = current_time;
                tranche.unlock_reason = Some(reason.to_string());

                self.total_unlocked += tranche.amount;

                log::info!(
                    "Tranche {} unlocked: {} OURO (reason: {})",
                    tranche.number,
                    tranche.amount / super::OURO_UNIT,
                    reason
                );

                Ok(tranche.amount)
            }
            None => Err("All tranches already unlocked".to_string()),
        }
    }

    /// Record distribution from the unlocked pool
    pub fn record_distribution(&mut self, amount: u64) -> Result<(), String> {
        if amount > self.available_supply() {
            return Err(format!(
                "Cannot distribute {} - only {} available",
                amount,
                self.available_supply()
            ));
        }

        self.total_distributed += amount;
        Ok(())
    }

    /// Check and unlock if needed based on current circulation
    /// This should be called periodically (e.g., every block)
    pub fn check_and_unlock(&mut self, current_circulating: u64, current_time: u64) -> Option<u64> {
        if self.should_unlock_next(current_circulating) {
            let threshold_pct = (UNLOCK_THRESHOLD_PERCENT * 100.0) as u64;
            let reason = format!(
                "Circulation dropped below {}% threshold ({} < {})",
                threshold_pct,
                current_circulating / super::OURO_UNIT,
                (self.unlocked_supply() as f64 * UNLOCK_THRESHOLD_PERCENT) as u64
                    / super::OURO_UNIT
            );

            match self.unlock_next_tranche(current_time, &reason) {
                Ok(amount) => Some(amount),
                Err(_) => None,
            }
        } else {
            None
        }
    }

    /// Get supply statistics
    pub fn get_supply_info(&self, current_circulating: u64, burned: u64) -> SupplyInfo {
        let max_supply = super::TOTAL_SUPPLY_UNITS;

        SupplyInfo {
            max_supply,
            initial_circulating: self.initial_supply,
            current_circulating,
            total_unlocked: self.total_unlocked,
            total_locked: self.remaining_reserve(),
            total_distributed: self.total_distributed,
            total_burned: burned,
            tranches_unlocked: self.unlocked_count() as u64,
            tranches_remaining: TOTAL_TRANCHES - self.unlocked_count() as u64,
            circulation_percent: (current_circulating as f64 / max_supply as f64) * 100.0,
            unlock_threshold: (self.unlocked_supply() as f64 * UNLOCK_THRESHOLD_PERCENT) as u64,
        }
    }

    /// Get next unlock threshold (circulating must drop below this)
    pub fn next_unlock_threshold(&self) -> Option<u64> {
        if self.unlocked_count() >= TOTAL_TRANCHES as usize {
            return None;
        }
        Some((self.unlocked_supply() as f64 * UNLOCK_THRESHOLD_PERCENT) as u64)
    }
}

/// Supply statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupplyInfo {
    /// Maximum supply (103M OURO)
    pub max_supply: u64,
    /// Initial circulating at genesis (13M OURO)
    pub initial_circulating: u64,
    /// Current circulating supply
    pub current_circulating: u64,
    /// Total unlocked from reserve
    pub total_unlocked: u64,
    /// Total still locked in reserve
    pub total_locked: u64,
    /// Total distributed from unlocked pool
    pub total_distributed: u64,
    /// Total burned (deflationary)
    pub total_burned: u64,
    /// Number of tranches unlocked
    pub tranches_unlocked: u64,
    /// Number of tranches remaining
    pub tranches_remaining: u64,
    /// Percentage of max supply circulating
    pub circulation_percent: f64,
    /// Current unlock threshold (circulating must drop below this)
    pub unlock_threshold: u64,
}

/// Validator economics configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorEconomics {
    /// Minimum stake required (100 OURO in smallest units)
    pub min_stake: u64,

    /// Expected annual percentage yield (estimated)
    pub estimated_apy: f64,
}

impl Default for ValidatorEconomics {
    fn default() -> Self {
        Self {
            min_stake: 100 * super::OURO_UNIT, // 100 OURO
            estimated_apy: 0.50,               // 50% APY (conservative estimate)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ouro_coin::{OURO_UNIT, TOTAL_SUPPLY_UNITS};

    #[test]
    fn test_fee_distribution_validates() {
        let dist = FeeDistribution::default();
        assert!(dist.validate(), "Fee distribution should sum to 100%");
    }

    #[test]
    fn test_fee_allocation() {
        let dist = FeeDistribution::default();
        let total_fee = 1_000_000; // 0.01 OURO in smallest units

        let allocation = dist.distribute(total_fee);

        assert_eq!(allocation.validators_amount, 700_000); // 70%
        assert_eq!(allocation.burn_amount, 100_000); // 10%
        assert_eq!(allocation.treasury_amount, 100_000); // 10%
        assert_eq!(allocation.app_developer_amount, 100_000); // 10%
    }

    #[test]
    fn test_token_distribution_validates() {
        let dist = TokenDistribution::default();
        assert!(
            dist.validate(),
            "Token distribution should equal total supply"
        );
        assert_eq!(dist.total(), 103_000_000); // 103M total
    }

    #[test]
    fn test_token_distribution_no_team_allocation() {
        let dist = TokenDistribution::default();

        // Initial circulating: 13M
        assert_eq!(dist.initial_circulating, 13_000_000);

        // Reserve: 90M (NO team allocation)
        assert_eq!(dist.reserve_pool, 90_000_000);
        assert_eq!(dist.validator_rewards_reserve, 50_000_000);
        assert_eq!(dist.development_reserve, 25_000_000);
        assert_eq!(dist.treasury_reserve, 15_000_000);
        // No team_locked field exists!
    }

    #[test]
    fn test_demand_based_release_initial() {
        let genesis_time = 1704067200; // Jan 1, 2024
        let release = DemandBasedRelease::new(genesis_time);

        // Initial state
        assert_eq!(release.unlocked_supply(), 13_000_000 * OURO_UNIT);
        assert_eq!(release.total_unlocked, 0);
        assert_eq!(release.unlocked_count(), 0);
        assert_eq!(release.remaining_reserve(), 90_000_000 * OURO_UNIT);
    }

    #[test]
    fn test_unlock_threshold() {
        let genesis_time = 1704067200;
        let release = DemandBasedRelease::new(genesis_time);

        // With 13M unlocked and 70% threshold, unlock triggers when < 9.1M circulating
        let threshold = release.next_unlock_threshold().unwrap();
        let expected = (13_000_000_f64 * OURO_UNIT as f64 * 0.70) as u64;
        assert_eq!(threshold, expected);
    }

    #[test]
    fn test_should_unlock_based_on_demand() {
        let genesis_time = 1704067200;
        let release = DemandBasedRelease::new(genesis_time);

        // Full circulation: should NOT unlock
        let full_circulation = 13_000_000 * OURO_UNIT;
        assert!(!release.should_unlock_next(full_circulation));

        // 80% circulation: should NOT unlock (above 70% threshold)
        let high_circulation = (13_000_000_f64 * OURO_UNIT as f64 * 0.80) as u64;
        assert!(!release.should_unlock_next(high_circulation));

        // 50% circulation: SHOULD unlock (below 70% threshold)
        let low_circulation = (13_000_000_f64 * OURO_UNIT as f64 * 0.50) as u64;
        assert!(release.should_unlock_next(low_circulation));
    }

    #[test]
    fn test_unlock_tranche() {
        let genesis_time = 1704067200;
        let mut release = DemandBasedRelease::new(genesis_time);

        // Unlock first tranche
        let amount = release
            .unlock_next_tranche(genesis_time + 1000, "Test unlock")
            .unwrap();
        assert_eq!(amount, 10_000_000 * OURO_UNIT);
        assert_eq!(release.unlocked_count(), 1);
        assert_eq!(release.total_unlocked, 10_000_000 * OURO_UNIT);

        // New unlocked supply: 13M + 10M = 23M
        assert_eq!(release.unlocked_supply(), 23_000_000 * OURO_UNIT);
    }

    #[test]
    fn test_full_unlock_sequence() {
        let genesis_time = 1704067200;
        let mut release = DemandBasedRelease::new(genesis_time);

        // Unlock all 9 tranches
        for i in 1..=9 {
            let amount = release
                .unlock_next_tranche(genesis_time + i * 1000, "Test")
                .unwrap();
            assert_eq!(amount, 10_000_000 * OURO_UNIT);
        }

        // All tranches unlocked
        assert_eq!(release.unlocked_count(), 9);
        assert_eq!(release.total_unlocked, 90_000_000 * OURO_UNIT);
        assert_eq!(release.remaining_reserve(), 0);

        // Total unlocked supply: 13M + 90M = 103M
        assert_eq!(release.unlocked_supply(), 103_000_000 * OURO_UNIT);

        // Can't unlock more
        assert!(release
            .unlock_next_tranche(genesis_time + 10000, "Test")
            .is_err());
    }

    #[test]
    fn test_check_and_unlock() {
        let genesis_time = 1704067200;
        let mut release = DemandBasedRelease::new(genesis_time);

        // High circulation: no unlock
        let high = 12_000_000 * OURO_UNIT;
        assert!(release
            .check_and_unlock(high, genesis_time + 1000)
            .is_none());

        // Low circulation: triggers unlock
        let low = 5_000_000 * OURO_UNIT;
        let unlocked = release.check_and_unlock(low, genesis_time + 2000);
        assert!(unlocked.is_some());
        assert_eq!(unlocked.unwrap(), 10_000_000 * OURO_UNIT);
    }

    #[test]
    fn test_supply_info() {
        let genesis_time = 1704067200;
        let release = DemandBasedRelease::new(genesis_time);

        let info = release.get_supply_info(13_000_000 * OURO_UNIT, 0);

        assert_eq!(info.max_supply, TOTAL_SUPPLY_UNITS);
        assert_eq!(info.initial_circulating, 13_000_000 * OURO_UNIT);
        assert_eq!(info.total_locked, 90_000_000 * OURO_UNIT);
        assert_eq!(info.tranches_unlocked, 0);
        assert_eq!(info.tranches_remaining, 9);
        assert!(info.circulation_percent > 12.0 && info.circulation_percent < 13.0);
    }
}
