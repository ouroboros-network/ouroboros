// Staking and delegation system
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
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

/// Thread-safe staking manager using RwLock for concurrent access
/// RwLock allows multiple readers OR one writer, preventing data races
pub struct StakingManager {
    validators: Arc<RwLock<HashMap<String, Validator>>>,
    delegations: Arc<RwLock<HashMap<String, Vec<Delegation>>>>,
    min_stake: u64,
    unbonding_period: u64,
}

impl Clone for StakingManager {
    fn clone(&self) -> Self {
        Self {
            validators: self.validators.clone(),
            delegations: self.delegations.clone(),
            min_stake: self.min_stake,
            unbonding_period: self.unbonding_period,
        }
    }
}

impl StakingManager {
    pub fn new(min_stake: u64, unbonding_period: u64) -> Self {
        Self {
            validators: Arc::new(RwLock::new(HashMap::new())),
            delegations: Arc::new(RwLock::new(HashMap::new())),
            min_stake,
            unbonding_period,
        }
    }

    pub fn register_validator(
        &self,
        address: String,
        stake: u64,
        commission: u16,
    ) -> Result<(), String> {
        if stake < self.min_stake {
            return Err(format!("Minimum stake is {}", self.min_stake));
        }

        let mut validators = self.validators.write();
        validators.insert(
            address.clone(),
            Validator {
                address,
                self_stake: stake,
                delegated_stake: 0,
                commission_rate: commission,
                total_rewards: 0,
                active: true,
            },
        );

        Ok(())
    }

    pub fn delegate(
        &self,
        delegator: String,
        validator: String,
        amount: u64,
    ) -> Result<(), String> {
        {
            let mut validators = self.validators.write();
            let val = validators
                .get_mut(&validator)
                .ok_or("Validator not found")?;
            val.delegated_stake += amount;
        }

        let mut delegations = self.delegations.write();
        delegations
            .entry(delegator.clone())
            .or_insert_with(Vec::new)
            .push(Delegation {
                delegator,
                validator,
                amount,
                rewards: 0,
            });

        Ok(())
    }

    pub fn distribute_rewards(&self, validator: &str, block_reward: u64) -> Result<(), String> {
        let commission;
        let delegator_share;
        let delegated_stake;

        {
            let mut validators = self.validators.write();
            let val = validators
                .get_mut(validator)
                .ok_or("Validator not found")?;

            commission = (block_reward * val.commission_rate as u64) / 10000;
            delegator_share = block_reward - commission;
            delegated_stake = val.delegated_stake;

            val.total_rewards += commission;
        }

        if delegated_stake == 0 {
            return Ok(()); // No delegations to distribute to
        }

        let mut delegations = self.delegations.write();
        let dels = delegations
            .get_mut(validator)
            .ok_or("No delegations")?;

        for del in dels.iter_mut() {
            let share = (delegator_share * del.amount) / delegated_stake;
            del.rewards += share;
        }

        Ok(())
    }

    /// Get validator by address (read-only)
    pub fn get_validator(&self, address: &str) -> Option<Validator> {
        self.validators.read().get(address).cloned()
    }

    /// Get all validators (read-only)
    pub fn get_all_validators(&self) -> Vec<Validator> {
        self.validators.read().values().cloned().collect()
    }

    /// Get validator stake
    pub fn get_validator_stake(&self, address: &str) -> Option<u64> {
        self.validators.read().get(address).map(|v| v.self_stake + v.delegated_stake)
    }

    pub fn get_total_stake(&self, validator: &str) -> u64 {
        self.validators
            .read()
            .get(validator)
            .map(|v| v.self_stake + v.delegated_stake)
            .unwrap_or(0)
    }

    pub fn get_active_validators(&self) -> Vec<String> {
        let validators = self.validators.read();
        validators
            .values()
            .filter(|v| v.active && v.self_stake >= self.min_stake)
            .map(|v| v.address.clone())
            .collect()
    }

    /// Sync active validators with BFT validator registry
    pub fn sync_to_validator_registry(
        &self,
        registry: &Arc<crate::bft::validator_registry::ValidatorRegistry>,
    ) {
        let validators = self.validators.read();
        for (addr, validator) in validators.iter() {
            let total_stake = validator.self_stake + validator.delegated_stake;
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
        let validators = self.validators.read();
        let mut result: Vec<_> = validators
            .iter()
            .filter(|(_, v)| v.active)
            .map(|(addr, v)| (addr.clone(), v.self_stake + v.delegated_stake))
            .collect();
        result.sort_by(|a, b| b.1.cmp(&a.1));
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_staking() {
        let staking = StakingManager::new(1000, 86400);

        staking
            .register_validator("val1".to_string(), 5000, 500)
            .unwrap();
        staking
            .delegate("alice".to_string(), "val1".to_string(), 1000)
            .unwrap();

        assert_eq!(staking.get_total_stake("val1"), 6000);
    }
}
