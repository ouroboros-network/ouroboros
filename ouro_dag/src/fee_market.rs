// Dynamic fee market (EIP-1559 style)
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeMarket {
    pub base_fee: u64,
    pub target_gas_per_block: u64,
    pub max_gas_per_block: u64,
    pub elasticity_multiplier: u64,
}

impl FeeMarket {
    pub fn new() -> Self {
        Self {
            base_fee: 10_000, // 0.0001 OURO - ultra cheap for mass adoption
            target_gas_per_block: 15_000_000,
            max_gas_per_block: 30_000_000,
            elasticity_multiplier: 2,
        }
    }

    pub fn update_base_fee(&mut self, gas_used: u64) {
        if gas_used > self.target_gas_per_block {
            let gas_delta = gas_used - self.target_gas_per_block;
            let fee_delta = (self.base_fee * gas_delta) / self.target_gas_per_block / 8;
            self.base_fee += fee_delta.max(1);
        } else if gas_used < self.target_gas_per_block {
            let gas_delta = self.target_gas_per_block - gas_used;
            let fee_delta = (self.base_fee * gas_delta) / self.target_gas_per_block / 8;
            self.base_fee = self.base_fee.saturating_sub(fee_delta);
        }

        self.base_fee = self.base_fee.max(1_000);
    }

    pub fn calculate_fee(&self, gas_used: u64, priority_fee: u64) -> u64 {
        (self.base_fee + priority_fee) * gas_used
    }

    pub fn validate_transaction(&self, max_fee: u64, gas_limit: u64) -> Result<(), String> {
        if gas_limit > self.max_gas_per_block {
            return Err("Gas limit too high".to_string());
        }

        if max_fee < self.base_fee {
            return Err(format!(
                "Max fee {} below base fee {}",
                max_fee, self.base_fee
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub from: String,
    pub to: String,
    pub value: u64,
    pub gas_limit: u64,
    pub max_fee_per_gas: u64,
    pub max_priority_fee: u64,
}

impl Transaction {
    pub fn effective_gas_price(&self, base_fee: u64) -> u64 {
        let max_fee = self.max_fee_per_gas.min(base_fee + self.max_priority_fee);
        max_fee
    }
}

/// Global fee market instance (should be stored in blockchain state)
pub struct FeeMarketManager {
    market: FeeMarket,
    block_gas_used: u64,
}

impl FeeMarketManager {
    pub fn new() -> Self {
        Self {
            market: FeeMarket::new(),
            block_gas_used: 0,
        }
    }

    /// Process transaction and update gas counters
    pub fn process_transaction(&mut self, gas_used: u64) -> Result<(), String> {
        if self.block_gas_used + gas_used > self.market.max_gas_per_block {
            return Err("Block gas limit exceeded".to_string());
        }
        self.block_gas_used += gas_used;
        Ok(())
    }

    /// Finalize block and update base fee
    pub fn finalize_block(&mut self) {
        self.market.update_base_fee(self.block_gas_used);
        self.block_gas_used = 0;
    }

    /// Get current base fee
    pub fn get_base_fee(&self) -> u64 {
        self.market.base_fee
    }

    /// Validate transaction against current market
    pub fn validate_tx(&self, max_fee: u64, gas_limit: u64) -> Result<(), String> {
        self.market.validate_transaction(max_fee, gas_limit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fee_adjustment() {
        let mut market = FeeMarket::new();
        let initial_fee = market.base_fee;

        market.update_base_fee(20_000_000);
        assert!(market.base_fee > initial_fee);

        market.update_base_fee(10_000_000);
        assert!(market.base_fee < 20_000_000);
    }

    #[test]
    fn test_fee_calculation() {
        let market = FeeMarket::new();
        let fee = market.calculate_fee(21_000, 1_000_000);
        assert!(fee > 0);
    }
}
