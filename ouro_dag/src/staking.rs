// Staking and delegation system
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stake {
    pub validator: String,
    pub amount: u64,
    pub locked_until: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delegation {
    pub delegator: String,
    pub validator: String,
    pub amount: u64,
    pub rewards: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Validator {
    pub address: String,
    pub self_stake: u64,
    pub delegated_stake: u64,
    pub commission_rate: u16, // basis points (100 = 1%)
    pub total_rewards: u64,
    pub active: bool,
}

pub struct StakingManager {
    validators: HashMap<String, Validator>,
    delegations: HashMap<String, Vec<Delegation>>,
    min_stake: u64,
    unbonding_period: u64,
}

impl StakingManager {
    pub fn new(min_stake: u64, unbonding_period: u64) -> Self {
        Self {
            validators: HashMap::new(),
            delegations: HashMap::new(),
            min_stake,
            unbonding_period,
        }
    }

    pub fn register_validator(&mut self, address: String, stake: u64, commission: u16) -> Result<(), String> {
        if stake < self.min_stake {
            return Err(format!("Minimum stake is {}", self.min_stake));
        }

        self.validators.insert(address.clone(), Validator {
            address,
            self_stake: stake,
            delegated_stake: 0,
            commission_rate: commission,
            total_rewards: 0,
            active: true,
        });

        Ok(())
    }

    pub fn delegate(&mut self, delegator: String, validator: String, amount: u64) -> Result<(), String> {
        let val = self.validators.get_mut(&validator).ok_or("Validator not found")?;
        val.delegated_stake += amount;

        self.delegations.entry(delegator.clone()).or_insert_with(Vec::new).push(Delegation {
            delegator,
            validator,
            amount,
            rewards: 0,
        });

        Ok(())
    }

    pub fn distribute_rewards(&mut self, validator: &str, block_reward: u64) -> Result<(), String> {
        let val = self.validators.get_mut(validator).ok_or("Validator not found")?;

        let commission = (block_reward * val.commission_rate as u64) / 10000;
        let delegator_share = block_reward - commission;

        val.total_rewards += commission;

        let delegations = self.delegations.get_mut(validator).ok_or("No delegations")?;

        for del in delegations.iter_mut() {
            let share = (delegator_share * del.amount) / val.delegated_stake;
            del.rewards += share;
        }

        Ok(())
    }

    pub fn get_total_stake(&self, validator: &str) -> u64 {
        self.validators.get(validator).map(|v| v.self_stake + v.delegated_stake).unwrap_or(0)
    }

    pub fn get_active_validators(&self) -> Vec<String> {
        self.validators.values()
            .filter(|v| v.active && v.self_stake >= self.min_stake)
            .map(|v| v.address.clone())
            .collect()
    }

    /// Sync active validators with BFT validator registry
    pub fn sync_to_validator_registry(&self, registry: &Arc<crate::bft::validator_registry::ValidatorRegistry>) {
        for (addr, validator) in &self.validators {
            let total_stake = self.get_total_stake(addr);
            if validator.active && total_stake >= self.min_stake {
                // Register active validators with stake amount
                // Note: Public key should be stored with validator data
                registry.register_with_stake(addr, vec![], total_stake);
            } else {
                // Remove validators with insufficient stake
                registry.remove(addr);
            }
        }
    }

    /// Get validators sorted by total stake
    pub fn get_validators_by_stake(&self) -> Vec<(String, u64)> {
        let mut validators: Vec<_> = self.validators
            .iter()
            .filter(|(_, v)| v.active)
            .map(|(addr, v)| (addr.clone(), self.get_total_stake(addr)))
            .collect();
        validators.sort_by(|a, b| b.1.cmp(&a.1));
        validators
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_staking() {
        let mut staking = StakingManager::new(1000, 86400);

        staking.register_validator("val1".to_string(), 5000, 500).unwrap();
        staking.delegate("alice".to_string(), "val1".to_string(), 1000).unwrap();

        assert_eq!(staking.get_total_stake("val1"), 6000);
    }
}
