// src/light_node_rewards.rs
//! Light Node Rewards System
//!
//! Allows anyone to participate in the network without upfront capital.
//! Earn rewards through:
//! - Reputation (uptime, reliability, good behavior)
//! - Storage (storing and serving historical blockchain data)
//!
//! No stake required - prove your value through actions.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use chrono::{DateTime, Utc, Duration};

/// Reputation score thresholds
pub const REPUTATION_MIN: f64 = 0.0;
pub const REPUTATION_MAX: f64 = 100.0;
pub const REPUTATION_START: f64 = 10.0; // New nodes start here

/// Reputation tiers for reward multipliers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReputationTier {
    /// New node (0-20 rep) - 0.5x rewards
    Newcomer,
    /// Established (20-50 rep) - 1x rewards
    Established,
    /// Trusted (50-80 rep) - 1.5x rewards
    Trusted,
    /// Veteran (80-100 rep) - 2x rewards
    Veteran,
}

impl ReputationTier {
    pub fn from_score(score: f64) -> Self {
        match score {
            s if s < 20.0 => ReputationTier::Newcomer,
            s if s < 50.0 => ReputationTier::Established,
            s if s < 80.0 => ReputationTier::Trusted,
            _ => ReputationTier::Veteran,
        }
    }

    pub fn reward_multiplier(&self) -> f64 {
        match self {
            ReputationTier::Newcomer => 0.5,
            ReputationTier::Established => 1.0,
            ReputationTier::Trusted => 1.5,
            ReputationTier::Veteran => 2.0,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            ReputationTier::Newcomer => "Newcomer",
            ReputationTier::Established => "Established",
            ReputationTier::Trusted => "Trusted",
            ReputationTier::Veteran => "Veteran",
        }
    }
}

/// Light node profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightNodeProfile {
    /// Node identifier (public key or node ID)
    pub node_id: String,

    /// Current reputation score (0-100)
    pub reputation: f64,

    /// When the node first joined
    pub joined_at: DateTime<Utc>,

    /// Total uptime in seconds
    pub total_uptime_secs: u64,

    /// Last seen timestamp
    pub last_seen: DateTime<Utc>,

    /// Consecutive days online
    pub streak_days: u32,

    /// Statistics
    pub stats: LightNodeStats,

    /// Storage contribution (bytes)
    pub storage_contributed_bytes: u64,

    /// Total rewards earned (in smallest OURO units)
    pub total_rewards_earned: u64,

    /// Pending rewards (not yet claimed)
    pub pending_rewards: u64,

    /// Wallet address for payouts (optional - can set later)
    pub reward_address: Option<String>,

    /// Warnings/strikes (too many = reputation penalty)
    pub warnings: u32,
}

/// Node activity statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LightNodeStats {
    /// Transactions relayed to validators
    pub txs_relayed: u64,

    /// Blocks propagated to peers
    pub blocks_propagated: u64,

    /// Data requests served (historical blocks/txs)
    pub data_requests_served: u64,

    /// Bytes served to other nodes
    pub bytes_served: u64,

    /// Peer connections facilitated
    pub peers_helped: u64,

    /// Invalid data served (should be 0)
    pub invalid_data_served: u64,

    /// Times node was unreachable when needed
    pub missed_requests: u64,
}

impl LightNodeProfile {
    /// Create new light node profile
    pub fn new(node_id: String) -> Self {
        let now = Utc::now();
        Self {
            node_id,
            reputation: REPUTATION_START,
            joined_at: now,
            total_uptime_secs: 0,
            last_seen: now,
            streak_days: 0,
            stats: LightNodeStats::default(),
            storage_contributed_bytes: 0,
            total_rewards_earned: 0,
            pending_rewards: 0,
            reward_address: None,
            warnings: 0,
        }
    }

    /// Get current reputation tier
    pub fn tier(&self) -> ReputationTier {
        ReputationTier::from_score(self.reputation)
    }

    /// Get node age in days
    pub fn age_days(&self) -> i64 {
        (Utc::now() - self.joined_at).num_days()
    }

    /// Check if node is currently online (seen in last 5 minutes)
    pub fn is_online(&self) -> bool {
        (Utc::now() - self.last_seen).num_minutes() < 5
    }

    /// Update last seen and uptime
    pub fn heartbeat(&mut self, uptime_delta_secs: u64) {
        let now = Utc::now();
        let was_online = self.is_online();

        self.last_seen = now;
        self.total_uptime_secs += uptime_delta_secs;

        // Update streak
        if !was_online {
            // Was offline, check if streak broken
            let hours_offline = (now - self.last_seen).num_hours();
            if hours_offline > 24 {
                self.streak_days = 0; // Streak broken
            }
        }
    }

    /// Record a successful action and update reputation
    pub fn record_good_action(&mut self, action: GoodAction) {
        let rep_gain = action.reputation_gain();
        self.reputation = (self.reputation + rep_gain).min(REPUTATION_MAX);

        match action {
            GoodAction::RelayedTransaction => self.stats.txs_relayed += 1,
            GoodAction::PropagatedBlock => self.stats.blocks_propagated += 1,
            GoodAction::ServedDataRequest(bytes) => {
                self.stats.data_requests_served += 1;
                self.stats.bytes_served += bytes;
            }
            GoodAction::HelpedPeerConnect => self.stats.peers_helped += 1,
            GoodAction::DayOnline => self.streak_days += 1,
            GoodAction::StoredHistoricalData(bytes) => {
                self.storage_contributed_bytes += bytes;
            }
        }
    }

    /// Record a bad action and penalize reputation
    pub fn record_bad_action(&mut self, action: BadAction) {
        let rep_loss = action.reputation_loss();
        self.reputation = (self.reputation - rep_loss).max(REPUTATION_MIN);

        match action {
            BadAction::ServedInvalidData => {
                self.stats.invalid_data_served += 1;
                self.warnings += 1;
            }
            BadAction::WasUnreachable => {
                self.stats.missed_requests += 1;
            }
            BadAction::WentOffline => {
                // Just reputation hit, no stat tracking
            }
            BadAction::SpammedNetwork => {
                self.warnings += 1;
            }
        }

        // Too many warnings = extra penalty
        if self.warnings >= 5 {
            self.reputation = (self.reputation - 10.0).max(REPUTATION_MIN);
            log::warn!("Node {} has {} warnings, extra reputation penalty applied",
                self.node_id, self.warnings);
        }
    }

    /// Calculate rewards for a period
    pub fn calculate_period_rewards(&self, base_reward: u64) -> u64 {
        let multiplier = self.tier().reward_multiplier();

        // Base calculation
        let mut reward = (base_reward as f64 * multiplier) as u64;

        // Bonus for streak (up to +50% for 30+ day streak)
        let streak_bonus = (self.streak_days as f64 / 30.0).min(1.0) * 0.5;
        reward = (reward as f64 * (1.0 + streak_bonus)) as u64;

        // Bonus for storage contribution (1 OURO per GB stored per day)
        let storage_gb = self.storage_contributed_bytes as f64 / 1_073_741_824.0;
        let storage_bonus = (storage_gb * 100_000_000.0) as u64; // 1 OURO = 10^8 units
        reward += storage_bonus;

        reward
    }
}

/// Good actions that earn reputation
#[derive(Debug, Clone)]
pub enum GoodAction {
    /// Relayed a transaction to validators
    RelayedTransaction,
    /// Propagated a new block to peers
    PropagatedBlock,
    /// Served a data request (with bytes served)
    ServedDataRequest(u64),
    /// Helped another peer connect to the network
    HelpedPeerConnect,
    /// Completed a full day online
    DayOnline,
    /// Stored historical data (bytes)
    StoredHistoricalData(u64),
}

impl GoodAction {
    pub fn reputation_gain(&self) -> f64 {
        match self {
            GoodAction::RelayedTransaction => 0.001,  // Small, many per day
            GoodAction::PropagatedBlock => 0.01,      // Medium
            GoodAction::ServedDataRequest(_) => 0.05, // Good contribution
            GoodAction::HelpedPeerConnect => 0.1,     // Helpful
            GoodAction::DayOnline => 0.5,             // Consistency rewarded
            GoodAction::StoredHistoricalData(bytes) => {
                // 0.1 rep per GB stored
                (*bytes as f64 / 1_073_741_824.0) * 0.1
            }
        }
    }
}

/// Bad actions that lose reputation
#[derive(Debug, Clone)]
pub enum BadAction {
    /// Served invalid/corrupted data
    ServedInvalidData,
    /// Was unreachable when another node needed data
    WasUnreachable,
    /// Went offline unexpectedly
    WentOffline,
    /// Spammed the network with invalid messages
    SpammedNetwork,
}

impl BadAction {
    pub fn reputation_loss(&self) -> f64 {
        match self {
            BadAction::ServedInvalidData => 5.0,  // Serious
            BadAction::WasUnreachable => 0.5,     // Minor
            BadAction::WentOffline => 0.1,        // Happens
            BadAction::SpammedNetwork => 10.0,    // Very serious
        }
    }
}

/// Storage rewards configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageRewardsConfig {
    /// Reward per GB stored per day (in smallest OURO units)
    pub reward_per_gb_per_day: u64,

    /// Minimum storage to qualify (bytes)
    pub min_storage_bytes: u64,

    /// Maximum storage that earns rewards (bytes) - prevents gaming
    pub max_rewarded_storage_bytes: u64,

    /// How often to verify storage (hours)
    pub verification_interval_hours: u64,
}

impl Default for StorageRewardsConfig {
    fn default() -> Self {
        Self {
            reward_per_gb_per_day: 100_000_000, // 1 OURO per GB per day
            min_storage_bytes: 1_073_741_824,   // 1 GB minimum
            max_rewarded_storage_bytes: 1_099_511_627_776, // 1 TB max
            verification_interval_hours: 24,
        }
    }
}

/// Light Node Rewards Manager
pub struct LightNodeRewardsManager {
    /// All registered light nodes
    profiles: HashMap<String, LightNodeProfile>,

    /// Storage rewards config
    storage_config: StorageRewardsConfig,

    /// Total rewards distributed
    total_distributed: u64,

    /// Rewards pool balance
    rewards_pool: u64,
}

impl LightNodeRewardsManager {
    pub fn new(initial_pool: u64) -> Self {
        Self {
            profiles: HashMap::new(),
            storage_config: StorageRewardsConfig::default(),
            total_distributed: 0,
            rewards_pool: initial_pool,
        }
    }

    /// Register a new light node
    pub fn register_node(&mut self, node_id: String) -> &LightNodeProfile {
        self.profiles.entry(node_id.clone())
            .or_insert_with(|| LightNodeProfile::new(node_id))
    }

    /// Get node profile
    pub fn get_profile(&self, node_id: &str) -> Option<&LightNodeProfile> {
        self.profiles.get(node_id)
    }

    /// Get mutable node profile
    pub fn get_profile_mut(&mut self, node_id: &str) -> Option<&mut LightNodeProfile> {
        self.profiles.get_mut(node_id)
    }

    /// Record heartbeat from node
    pub fn heartbeat(&mut self, node_id: &str, uptime_delta_secs: u64) {
        if let Some(profile) = self.profiles.get_mut(node_id) {
            profile.heartbeat(uptime_delta_secs);
        }
    }

    /// Record good action
    pub fn record_good_action(&mut self, node_id: &str, action: GoodAction) {
        if let Some(profile) = self.profiles.get_mut(node_id) {
            profile.record_good_action(action);
        }
    }

    /// Record bad action
    pub fn record_bad_action(&mut self, node_id: &str, action: BadAction) {
        if let Some(profile) = self.profiles.get_mut(node_id) {
            profile.record_bad_action(action);
        }
    }

    /// Set reward address for a node
    pub fn set_reward_address(&mut self, node_id: &str, address: String) -> Result<(), String> {
        if let Some(profile) = self.profiles.get_mut(node_id) {
            profile.reward_address = Some(address);
            Ok(())
        } else {
            Err("Node not found".to_string())
        }
    }

    /// Calculate and distribute daily rewards
    pub fn distribute_daily_rewards(&mut self, base_reward_per_node: u64) -> Vec<RewardPayout> {
        let mut payouts = Vec::new();

        for (node_id, profile) in self.profiles.iter_mut() {
            // Skip nodes without reward address
            let address = match &profile.reward_address {
                Some(addr) => addr.clone(),
                None => continue,
            };

            // Skip offline nodes
            if !profile.is_online() {
                continue;
            }

            // Calculate reward
            let reward = profile.calculate_period_rewards(base_reward_per_node);

            // Check pool has enough
            if reward > self.rewards_pool {
                log::warn!("Rewards pool depleted, skipping payouts");
                break;
            }

            // Add to pending
            profile.pending_rewards += reward;
            profile.total_rewards_earned += reward;
            self.rewards_pool -= reward;
            self.total_distributed += reward;

            payouts.push(RewardPayout {
                node_id: node_id.clone(),
                address,
                amount: reward,
                tier: profile.tier(),
                reputation: profile.reputation,
            });
        }

        payouts
    }

    /// Get leaderboard (top nodes by reputation)
    pub fn get_leaderboard(&self, limit: usize) -> Vec<LeaderboardEntry> {
        let mut entries: Vec<_> = self.profiles.values()
            .map(|p| LeaderboardEntry {
                node_id: p.node_id.clone(),
                reputation: p.reputation,
                tier: p.tier(),
                total_rewards: p.total_rewards_earned,
                uptime_days: p.total_uptime_secs / 86400,
                storage_gb: p.storage_contributed_bytes as f64 / 1_073_741_824.0,
            })
            .collect();

        entries.sort_by(|a, b| b.reputation.partial_cmp(&a.reputation).unwrap());
        entries.truncate(limit);
        entries
    }

    /// Get network statistics
    pub fn get_stats(&self) -> LightNodeNetworkStats {
        let online_count = self.profiles.values().filter(|p| p.is_online()).count();
        let total_storage: u64 = self.profiles.values()
            .map(|p| p.storage_contributed_bytes)
            .sum();
        let total_txs_relayed: u64 = self.profiles.values()
            .map(|p| p.stats.txs_relayed)
            .sum();

        let avg_reputation = if self.profiles.is_empty() {
            0.0
        } else {
            self.profiles.values().map(|p| p.reputation).sum::<f64>()
                / self.profiles.len() as f64
        };

        LightNodeNetworkStats {
            total_nodes: self.profiles.len(),
            online_nodes: online_count,
            total_storage_bytes: total_storage,
            total_txs_relayed,
            avg_reputation,
            total_rewards_distributed: self.total_distributed,
            rewards_pool_remaining: self.rewards_pool,
        }
    }
}

/// Reward payout record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardPayout {
    pub node_id: String,
    pub address: String,
    pub amount: u64,
    pub tier: ReputationTier,
    pub reputation: f64,
}

/// Leaderboard entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderboardEntry {
    pub node_id: String,
    pub reputation: f64,
    pub tier: ReputationTier,
    pub total_rewards: u64,
    pub uptime_days: u64,
    pub storage_gb: f64,
}

/// Network-wide statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightNodeNetworkStats {
    pub total_nodes: usize,
    pub online_nodes: usize,
    pub total_storage_bytes: u64,
    pub total_txs_relayed: u64,
    pub avg_reputation: f64,
    pub total_rewards_distributed: u64,
    pub rewards_pool_remaining: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reputation_tiers() {
        assert_eq!(ReputationTier::from_score(5.0), ReputationTier::Newcomer);
        assert_eq!(ReputationTier::from_score(25.0), ReputationTier::Established);
        assert_eq!(ReputationTier::from_score(60.0), ReputationTier::Trusted);
        assert_eq!(ReputationTier::from_score(90.0), ReputationTier::Veteran);
    }

    #[test]
    fn test_new_node_starts_as_newcomer() {
        let profile = LightNodeProfile::new("test_node".to_string());
        assert_eq!(profile.reputation, REPUTATION_START);
        assert_eq!(profile.tier(), ReputationTier::Newcomer);
    }

    #[test]
    fn test_good_actions_increase_reputation() {
        let mut profile = LightNodeProfile::new("test_node".to_string());
        let initial_rep = profile.reputation;

        profile.record_good_action(GoodAction::RelayedTransaction);
        assert!(profile.reputation > initial_rep);

        profile.record_good_action(GoodAction::DayOnline);
        assert!(profile.reputation > initial_rep + 0.001);
    }

    #[test]
    fn test_bad_actions_decrease_reputation() {
        let mut profile = LightNodeProfile::new("test_node".to_string());
        profile.reputation = 50.0; // Start higher

        profile.record_bad_action(BadAction::ServedInvalidData);
        assert!(profile.reputation < 50.0);
        assert_eq!(profile.warnings, 1);
    }

    #[test]
    fn test_reputation_bounds() {
        let mut profile = LightNodeProfile::new("test_node".to_string());

        // Can't go above max
        profile.reputation = 99.0;
        profile.record_good_action(GoodAction::DayOnline);
        profile.record_good_action(GoodAction::DayOnline);
        profile.record_good_action(GoodAction::DayOnline);
        assert!(profile.reputation <= REPUTATION_MAX);

        // Can't go below min
        profile.reputation = 1.0;
        profile.record_bad_action(BadAction::SpammedNetwork);
        assert!(profile.reputation >= REPUTATION_MIN);
    }

    #[test]
    fn test_reward_multipliers() {
        let mut profile = LightNodeProfile::new("test_node".to_string());
        let base_reward = 1_000_000u64;

        // Newcomer: 0.5x
        profile.reputation = 10.0;
        let newcomer_reward = profile.calculate_period_rewards(base_reward);

        // Veteran: 2x
        profile.reputation = 90.0;
        let veteran_reward = profile.calculate_period_rewards(base_reward);

        assert!(veteran_reward > newcomer_reward * 3); // Should be ~4x difference
    }

    #[test]
    fn test_manager_register_and_track() {
        let mut manager = LightNodeRewardsManager::new(1_000_000_000);

        manager.register_node("node1".to_string());
        manager.record_good_action("node1", GoodAction::RelayedTransaction);

        let profile = manager.get_profile("node1").unwrap();
        assert_eq!(profile.stats.txs_relayed, 1);
    }
}
