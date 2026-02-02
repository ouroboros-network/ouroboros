// src/vm/gas.rs
//! Gas metering for OVM contract execution
//!
//! Tracks gas consumption to prevent infinite loops and resource exhaustion.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// Gas costs for OVM operations (Ouroboros-specific gas schedule)
///
/// These values are calibrated for OVM/WASM execution and can be adjusted
/// based on real-world benchmarks and network performance.
#[derive(Debug, Clone)]
pub struct GasCosts {
    // Basic operations
    pub step: u64,            // 1 - Basic computation step
    pub stop: u64,            // 0 - Free
    pub arithmetic: u64,      // 3 - Addition, subtraction, etc.
    pub comparison: u64,      // 3 - LT, GT, EQ, etc.
    pub bitwise: u64,         // 3 - AND, OR, XOR, etc.
    pub sha256_base: u64,     // 60 - Base cost for SHA256
    pub sha256_per_word: u64, // 12 - Per 32 bytes

    // Memory operations
    pub memory_base: u64,     // 3 - Memory access base cost
    pub memory_per_byte: u64, // 1 - Per byte copied

    // Storage operations
    pub storage_set: u64,   // 20000 - Store value
    pub storage_get: u64,   // 200 - Load value
    pub storage_clear: u64, // 5000 - Clear storage slot

    // Contract operations
    pub create_contract: u64, // 32000 - Deploy new contract
    pub call_contract: u64,   // 700 - Call another contract
    pub transfer: u64,        // 9000 - Transfer tokens

    // Cryptography precompiles
    pub ed25519_verify: u64,     // 3000 - Ed25519 signature verification
    pub ecdsa_verify: u64,       // 3000 - ECDSA signature verification
    pub keccak256_base: u64,     // 30 - Keccak256 base
    pub keccak256_per_word: u64, // 6 - Keccak256 per word
    pub blake2_base: u64,        // 30 - Blake2 base
    pub blake2_per_word: u64,    // 6 - Blake2 per word

    // Big integer math
    pub modexp_base: u64,     // 200 - Modular exponentiation base
    pub modexp_per_byte: u64, // 1 - Per byte of input
}

impl Default for GasCosts {
    fn default() -> Self {
        Self {
            step: 1,
            stop: 0,
            arithmetic: 3,
            comparison: 3,
            bitwise: 3,
            sha256_base: 60,
            sha256_per_word: 12,
            memory_base: 3,
            memory_per_byte: 1,
            storage_set: 20_000,
            storage_get: 200,
            storage_clear: 5_000,
            create_contract: 32_000,
            call_contract: 700,
            transfer: 9_000,
            ed25519_verify: 3_000,
            ecdsa_verify: 3_000,
            keccak256_base: 30,
            keccak256_per_word: 6,
            blake2_base: 30,
            blake2_per_word: 6,
            modexp_base: 200,
            modexp_per_byte: 1,
        }
    }
}

/// Gas metering state for contract execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasState {
    /// Gas limit for this execution
    pub gas_limit: u64,

    /// Gas consumed so far
    pub gas_used: u64,

    /// Gas costs schedule
    #[serde(skip)]
    pub costs: GasCosts,

    /// Contract address being executed
    pub contract_address: Option<String>,

    /// Caller address
    pub caller: Option<String>,
}

impl GasState {
    /// Create new gas state with limit
    pub fn new(gas_limit: u64) -> Self {
        Self {
            gas_limit,
            gas_used: 0,
            costs: GasCosts::default(),
            contract_address: None,
            caller: None,
        }
    }

    /// Create with custom costs
    pub fn with_costs(gas_limit: u64, costs: GasCosts) -> Self {
        Self {
            gas_limit,
            gas_used: 0,
            costs,
            contract_address: None,
            caller: None,
        }
    }

    /// Consume gas for an operation
    pub fn consume_gas(&mut self, amount: u64) -> Result<()> {
        let new_total = self
            .gas_used
            .checked_add(amount)
            .ok_or_else(|| anyhow::anyhow!("Gas overflow"))?;

        if new_total > self.gas_limit {
            bail!(
                "Out of gas: used {}, limit {}, tried to add {}",
                self.gas_used,
                self.gas_limit,
                amount
            );
        }

        self.gas_used = new_total;
        Ok(())
    }

    /// Check if enough gas is available
    pub fn check_gas(&self, required: u64) -> Result<()> {
        let available = self.gas_limit.saturating_sub(self.gas_used);
        if available < required {
            bail!("Insufficient gas: need {}, have {}", required, available);
        }
        Ok(())
    }

    /// Get remaining gas
    pub fn gas_remaining(&self) -> u64 {
        self.gas_limit.saturating_sub(self.gas_used)
    }

    /// Get gas usage percentage
    pub fn gas_usage_percent(&self) -> f64 {
        if self.gas_limit == 0 {
            return 0.0;
        }
        (self.gas_used as f64 / self.gas_limit as f64) * 100.0
    }

    /// Reset gas used (for testing)
    pub fn reset(&mut self) {
        self.gas_used = 0;
    }
}

/// Calculate gas cost for memory expansion
pub fn memory_gas_cost(current_size: u64, new_size: u64) -> u64 {
    if new_size <= current_size {
        return 0;
    }

    let expansion = new_size - current_size;
    let word_expansion = (expansion + 31) / 32; // Round up to words

    // Quadratic cost: word_expansion * 3 + word_expansion^2 / 512
    let linear = word_expansion * 3;
    let quadratic = (word_expansion * word_expansion) / 512;

    linear + quadratic
}

/// Calculate gas cost for data copy
pub fn copy_gas_cost(num_bytes: u64) -> u64 {
    let words = (num_bytes + 31) / 32; // Round up to words
    words * 3
}

/// Calculate gas cost for hash operations
pub fn hash_gas_cost(base_cost: u64, per_word_cost: u64, data_len: u64) -> u64 {
    let words = (data_len + 31) / 32; // Round up to words
    base_cost + (words * per_word_cost)
}

/// Calculate gas cost for modular exponentiation
/// Based on complexity: base^exp mod modulus
pub fn modexp_gas_cost(base_len: u64, exp_len: u64, mod_len: u64) -> u64 {
    let max_len = base_len.max(mod_len);
    let words = (max_len + 7) / 8;

    // Simplified calculation (Ethereum uses more complex formula)
    let complexity = words * words; // O(n^2) for multiplication
    let iterations = exp_len * 8; // Number of bits in exponent

    200 + (complexity * iterations) / 100
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gas_consumption() {
        let mut gas = GasState::new(1000);

        assert!(gas.consume_gas(100).is_ok());
        assert_eq!(gas.gas_used, 100);
        assert_eq!(gas.gas_remaining(), 900);

        assert!(gas.consume_gas(500).is_ok());
        assert_eq!(gas.gas_used, 600);

        // Should fail - exceeds limit
        assert!(gas.consume_gas(500).is_err());
        assert_eq!(gas.gas_used, 600); // Unchanged
    }

    #[test]
    fn test_gas_check() {
        let gas = GasState::new(1000);

        assert!(gas.check_gas(500).is_ok());
        assert!(gas.check_gas(1000).is_ok());
        assert!(gas.check_gas(1001).is_err());
    }

    #[test]
    fn test_gas_usage_percent() {
        let mut gas = GasState::new(1000);

        assert_eq!(gas.gas_usage_percent(), 0.0);

        gas.consume_gas(500).unwrap();
        assert_eq!(gas.gas_usage_percent(), 50.0);

        gas.consume_gas(250).unwrap();
        assert_eq!(gas.gas_usage_percent(), 75.0);
    }

    #[test]
    fn test_memory_gas_cost() {
        // No expansion
        assert_eq!(memory_gas_cost(100, 50), 0);
        assert_eq!(memory_gas_cost(100, 100), 0);

        // Small expansion (1 word = 32 bytes)
        assert_eq!(memory_gas_cost(0, 32), 3); // 1 word * 3

        // Larger expansion
        let cost = memory_gas_cost(0, 1024); // 32 words
        assert!(cost > 0);
    }

    #[test]
    fn test_copy_gas_cost() {
        assert_eq!(copy_gas_cost(0), 0);
        assert_eq!(copy_gas_cost(32), 3); // 1 word
        assert_eq!(copy_gas_cost(64), 6); // 2 words
        assert_eq!(copy_gas_cost(33), 6); // 2 words (rounds up)
    }

    #[test]
    fn test_hash_gas_cost() {
        // SHA256: 60 base + 12 per word
        assert_eq!(hash_gas_cost(60, 12, 0), 60);
        assert_eq!(hash_gas_cost(60, 12, 32), 72); // 1 word
        assert_eq!(hash_gas_cost(60, 12, 64), 84); // 2 words
    }
}
