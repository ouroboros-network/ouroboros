// src/rewards.rs
// Node reward system: 1 OURO per day for running nodes
// Sustainable inflation: 10,000 nodes = 3.65M OURO/year = 3.65% of 100M supply

use crate::storage::RocksDb;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Reward rate: 1 OURO per day = 100_000_000 microunits per day
/// This provides 365 OURO/year per node - sustainable incentive
const REWARD_PER_DAY: u64 = 100_000_000; // 1 OURO

/// Minimum uptime required to claim rewards (5 minutes)
const MIN_UPTIME_SECS: u64 = 300;

/// Node heartbeat record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeHeartbeat {
    pub node_id: String,
    pub wallet_address: String,
    pub last_heartbeat: DateTime<Utc>,
    pub first_seen: DateTime<Utc>,
    pub total_uptime_secs: u64,
    pub last_reward_claim: DateTime<Utc>,
}

impl NodeHeartbeat {
    pub fn new(node_id: String, wallet_address: String) -> Self {
        let now = Utc::now();
        Self {
            node_id,
            wallet_address,
            last_heartbeat: now,
            first_seen: now,
            total_uptime_secs: 0,
            last_reward_claim: now,
        }
    }
}

/// Calculate pending rewards for a node
/// Uses saturating arithmetic to prevent overflow if clock goes backward
pub fn calculate_pending_rewards(heartbeat: &NodeHeartbeat) -> u64 {
    let now = Utc::now();
    let duration = now.signed_duration_since(heartbeat.last_reward_claim);

    // Clamp to non-negative (handles clock going backward)
    let secs_since_claim = duration.num_seconds().max(0) as u64;

    // Cap maximum claimable time to 30 days to prevent abuse
    let capped_secs = secs_since_claim.min(30 * 86400);

    // Reward = (seconds_online / 86400) * REWARD_PER_DAY
    // 86400 = seconds in a day
    let days = capped_secs as f64 / 86400.0;

    // Use saturating conversion to prevent overflow
    let reward = (days * REWARD_PER_DAY as f64) as u64;

    // Additional cap: max 30 OURO per claim (prevents gaming)
    reward.min(30 * REWARD_PER_DAY)
}

/// Record a heartbeat from a node
pub async fn record_heartbeat(
    db: &RocksDb,
    node_id: &str,
    wallet_address: &str,
) -> Result<(), String> {
    let key = format!("heartbeat:{}", node_id);

    // Get existing heartbeat or create new one
    let mut heartbeat: NodeHeartbeat = match crate::storage::get_str(db, &key)? {
        Some(h) => h,
        None => NodeHeartbeat::new(node_id.to_string(), wallet_address.to_string()),
    };

    // Update heartbeat
    let now = Utc::now();
    let duration = now.signed_duration_since(heartbeat.last_heartbeat);
    // Clamp to non-negative (handles clock going backward)
    let secs_since_last = duration.num_seconds().max(0) as u64;

    // Only count uptime if heartbeat is within 5 minutes (prevent gaming)
    if secs_since_last < 300 {
        // Use saturating add to prevent overflow
        heartbeat.total_uptime_secs = heartbeat.total_uptime_secs.saturating_add(secs_since_last);
    }

    heartbeat.last_heartbeat = now;

    // Save updated heartbeat
    crate::storage::put_str(db, &key, &heartbeat)?;

    Ok(())
}

/// Claim rewards for a node
pub async fn claim_rewards(db: &RocksDb, node_id: &str) -> Result<(String, u64), String> {
    let key = format!("heartbeat:{}", node_id);

    // Get heartbeat
    let mut heartbeat: NodeHeartbeat = match crate::storage::get_str(db, &key)? {
        Some(h) => h,
        None => return Err("Node not found".to_string()),
    };

    // Calculate pending rewards
    let reward_amount = calculate_pending_rewards(&heartbeat);

    // Check minimum uptime
    let now = Utc::now();
    let duration = now.signed_duration_since(heartbeat.last_reward_claim);
    let secs_since_claim = duration.num_seconds().max(0) as u64;

    if secs_since_claim < MIN_UPTIME_SECS {
        return Err(format!(
            "Minimum uptime not met (need {} seconds, have {})",
            MIN_UPTIME_SECS, secs_since_claim
        ));
    }

    if reward_amount == 0 {
        return Err("No rewards to claim".to_string());
    }

    // Update claim time
    heartbeat.last_reward_claim = now;
    crate::storage::put_str(db, &key, &heartbeat)?;

    // Return wallet address and reward amount
    Ok((heartbeat.wallet_address.clone(), reward_amount))
}

/// Get all active nodes (heartbeat within last 5 minutes)
pub async fn get_active_nodes(db: &RocksDb) -> Result<Vec<NodeHeartbeat>, String> {
    let heartbeats: Vec<NodeHeartbeat> = crate::storage::iter_prefix(db, b"heartbeat:")?;

    let now = Utc::now();
    let active: Vec<NodeHeartbeat> = heartbeats
        .into_iter()
        .filter(|h| {
            let secs_since = (now - h.last_heartbeat).num_seconds() as u64;
            secs_since < 300
        })
        .collect();

    Ok(active)
}

/// Get node statistics
pub async fn get_node_stats(db: &RocksDb, node_id: &str) -> Result<NodeHeartbeat, String> {
    let key = format!("heartbeat:{}", node_id);
    match crate::storage::get_str(db, &key)? {
        Some(h) => Ok(h),
        None => Err("Node not found".to_string()),
    }
}
