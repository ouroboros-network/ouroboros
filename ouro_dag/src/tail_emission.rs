// Tail emission for sustainable network security
// Prevents fee-only security issues (like Monero)

use serde::{Deserialize, Serialize};

/// Emission schedule configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmissionConfig {
    /// Initial block reward (in smallest unit)
    pub initial_reward: u64,
    /// Halving interval in blocks (0 = no halving, smooth decay)
    pub halving_blocks: u64,
    /// Minimum perpetual reward (tail emission)
    pub tail_reward: u64,
    /// Total supply cap (0 = infinite with tail emission)
    pub supply_cap: u64,
}

impl Default for EmissionConfig {
    fn default() -> Self {
        Self {
            initial_reward: 50_000_000, // 0.5 OURO per block (~3.15% year 1 inflation)
            halving_blocks: 6_307_200,  // ~1 year at 5 sec blocks (17,280 blocks/day)
            tail_reward: 10_000_000,    // 0.1 OURO perpetual (~0.63% long-term inflation)
            supply_cap: 103_000_000 * 100_000_000, // 103M OURO hard cap
        }
    }
}

/// Calculate block reward for given block height
pub fn calculate_block_reward(height: u64, config: &EmissionConfig) -> u64 {
    if config.halving_blocks == 0 {
        // Smooth exponential decay
        return smooth_decay_reward(height, config);
    }

    // Halving-based emission
    let halvings = height / config.halving_blocks;

    if halvings >= 64 {
        // After 64 halvings, switch to tail emission
        return config.tail_reward;
    }

    let reward = config.initial_reward >> halvings;

    // If reward drops below tail emission, use tail emission
    if reward < config.tail_reward {
        config.tail_reward
    } else {
        reward
    }
}

/// Smooth exponential decay (alternative to halvings)
fn smooth_decay_reward(height: u64, config: &EmissionConfig) -> u64 {
    // Decay factor: 0.999998 per block
    // Reaches ~50% after ~350k blocks
    let decay = 0.999998_f64;
    let blocks = height as f64;

    let reward = (config.initial_reward as f64) * decay.powf(blocks);
    let reward_u64 = reward as u64;

    // Never go below tail emission
    if reward_u64 < config.tail_reward {
        config.tail_reward
    } else {
        reward_u64
    }
}

/// Calculate total supply at given height
pub fn total_supply_at_height(height: u64, config: &EmissionConfig) -> u64 {
    if height == 0 {
        return 0;
    }

    let mut total: u128 = 0;

    if config.halving_blocks > 0 {
        // Sum rewards for each halving period
        let mut current_height = 0;
        let mut current_reward = config.initial_reward;

        while current_height < height {
            let next_halving =
                ((current_height / config.halving_blocks) + 1) * config.halving_blocks;
            let blocks_in_period = if next_halving < height {
                next_halving - current_height
            } else {
                height - current_height
            };

            total += (blocks_in_period as u128) * (current_reward as u128);
            current_height += blocks_in_period;
            current_reward = current_reward / 2;

            if current_reward < config.tail_reward {
                current_reward = config.tail_reward;
            }
        }
    } else {
        // Smooth decay - approximate
        for h in 0..height {
            total += calculate_block_reward(h, config) as u128;
        }
    }

    // Cap at supply limit if set
    if config.supply_cap > 0 && total > config.supply_cap as u128 {
        config.supply_cap
    } else {
        total as u64
    }
}

/// Get emission stats for block height
#[derive(Debug, Serialize, Deserialize)]
pub struct EmissionStats {
    pub block_height: u64,
    pub block_reward: u64,
    pub total_supply: u64,
    pub is_tail_emission: bool,
    pub next_halving: Option<u64>,
}

pub fn get_emission_stats(height: u64, config: &EmissionConfig) -> EmissionStats {
    let reward = calculate_block_reward(height, config);
    let is_tail = reward == config.tail_reward;

    let next_halving = if config.halving_blocks > 0 && !is_tail {
        let current_era = height / config.halving_blocks;
        Some((current_era + 1) * config.halving_blocks)
    } else {
        None
    };

    EmissionStats {
        block_height: height,
        block_reward: reward,
        total_supply: total_supply_at_height(height, config),
        is_tail_emission: is_tail,
        next_halving,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_halving_emission() {
        let config = EmissionConfig::default();

        // Block 0: 0.5 OURO = 50,000,000 units
        assert_eq!(calculate_block_reward(0, &config), 50_000_000);

        // Block 6,307,200 (first halving at ~1 year): 0.25 OURO
        assert_eq!(calculate_block_reward(6_307_200, &config), 25_000_000);

        // Block 12,614,400 (second halving at ~2 years): 0.125 OURO
        assert_eq!(calculate_block_reward(12_614_400, &config), 12_500_000);

        // Block 18,921,600 (third halving): 0.0625 OURO < 0.1 OURO tail, so tail kicks in
        // After 3 halvings: 50M >> 3 = 6,250,000 < 10,000,000 (tail), so returns tail
        assert_eq!(calculate_block_reward(18_921_600, &config), 10_000_000);
    }

    #[test]
    fn test_tail_emission_perpetual() {
        let config = EmissionConfig::default();
        // Tail emission kicks in after 3 halvings (when reward < tail_reward)
        let tail_height = 18_921_600; // After 3 halvings

        // Verify tail emission is constant at 0.1 OURO = 10,000,000 units
        assert_eq!(calculate_block_reward(tail_height, &config), 10_000_000);
        assert_eq!(
            calculate_block_reward(tail_height + 1_000_000, &config),
            10_000_000
        );
        assert_eq!(
            calculate_block_reward(tail_height + 10_000_000, &config),
            10_000_000
        );
    }

    #[test]
    fn test_smooth_decay() {
        // Use custom config for smooth decay test (higher values for visible decay)
        let config = EmissionConfig {
            initial_reward: 50_000_000_000, // Higher starting value for testing
            halving_blocks: 0,              // Smooth decay mode
            tail_reward: 600_000_000,
            supply_cap: 0,
        };

        let reward_0 = calculate_block_reward(0, &config);
        let reward_100k = calculate_block_reward(100_000, &config);
        let reward_1m = calculate_block_reward(1_000_000, &config);

        // Verify smooth decay
        assert!(reward_0 > reward_100k);
        assert!(reward_100k > reward_1m);
        assert!(reward_1m >= config.tail_reward);
    }

    #[test]
    fn test_economic_values() {
        let config = EmissionConfig::default();

        // Verify the economic constants are correct for 103M supply
        assert_eq!(config.initial_reward, 50_000_000); // 0.5 OURO
        assert_eq!(config.tail_reward, 10_000_000); // 0.1 OURO
        assert_eq!(config.halving_blocks, 6_307_200); // ~1 year at 5 sec blocks
        assert_eq!(config.supply_cap, 10_300_000_000_000_000); // 103M OURO in units
    }
}
