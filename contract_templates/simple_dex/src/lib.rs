// contract_templates/simple_dex/src/lib.rs
//! Simple DEX Contract for OVM
//!
//! A basic automated market maker (AMM) decentralized exchange.
//!
//! # Features
//! - Token pair liquidity pools
//! - Constant product formula (x * y = k)
//! - Add/remove liquidity
//! - Token swapping
//! - Liquidity provider shares

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Integer square root using binary search
/// This is deterministic across all platforms unlike f64::sqrt
fn isqrt(n: u128) -> u128 {
    if n == 0 {
        return 0;
    }

    let mut low = 1u128;
    let mut high = n;
    let mut result = 0u128;

    while low <= high {
        let mid = low + (high - low) / 2;

        // Check for overflow: mid * mid
        if let Some(square) = mid.checked_mul(mid) {
            if square == n {
                return mid;
            } else if square < n {
                result = mid;
                low = mid + 1;
            } else {
                high = mid - 1;
            }
        } else {
            // mid * mid overflowed, so mid is too large
            high = mid - 1;
        }
    }

    result
}

/// DEX contract state
#[derive(Serialize, Deserialize, Default)]
pub struct DEXState {
    /// Liquidity pools: (token_a, token_b) -> Pool
    pub pools: HashMap<(String, String), LiquidityPool>,

    /// LP token balances: user -> (token_a, token_b) -> shares
    /// Uses u128 to support tokens with 18 decimals (standard for ERC-20 compatible tokens)
    pub lp_balances: HashMap<String, HashMap<(String, String), u128>>,

    /// Owner address
    pub owner: String,

    /// Fee basis points (e.g., 30 = 0.3%)
    pub fee_basis_points: u64,
}

/// Liquidity pool for a token pair
/// Uses u128 for reserves to support high-precision tokens (18 decimals)
/// u128 max ≈ 3.4 × 10^38, sufficient for 10^20 tokens with 18 decimals
#[derive(Serialize, Deserialize, Clone)]
pub struct LiquidityPool {
    /// Reserve of token A
    pub reserve_a: u128,

    /// Reserve of token B
    pub reserve_b: u128,

    /// Total LP shares
    pub total_shares: u128,
}

impl LiquidityPool {
    fn new() -> Self {
        Self {
            reserve_a: 0,
            reserve_b: 0,
            total_shares: 0,
        }
    }
}

/// Add liquidity arguments
#[derive(Deserialize)]
pub struct AddLiquidityArgs {
    pub token_a: String,
    pub token_b: String,
    pub amount_a: u128,
    pub amount_b: u128,
}

/// Remove liquidity arguments
#[derive(Deserialize)]
pub struct RemoveLiquidityArgs {
    pub token_a: String,
    pub token_b: String,
    pub shares: u128,
}

/// Swap arguments
#[derive(Deserialize)]
pub struct SwapArgs {
    pub token_in: String,
    pub token_out: String,
    pub amount_in: u128,
    pub min_amount_out: u128, // Slippage protection
}

/// Initialize DEX
pub fn initialize(owner: String, fee_basis_points: u64) -> DEXState {
    DEXState {
        pools: HashMap::new(),
        lp_balances: HashMap::new(),
        owner,
        fee_basis_points, // e.g., 30 = 0.3%
    }
}

/// Get pool for token pair (ensures consistent ordering)
fn get_pool_key(token_a: &str, token_b: &str) -> (String, String) {
    if token_a < token_b {
        (token_a.to_string(), token_b.to_string())
    } else {
        (token_b.to_string(), token_a.to_string())
    }
}

/// Get or create pool
fn get_or_create_pool<'a>(state: &'a mut DEXState, token_a: &str, token_b: &str) -> &'a mut LiquidityPool {
    let key = get_pool_key(token_a, token_b);
    state.pools.entry(key).or_insert_with(LiquidityPool::new)
}

/// Get pool (read-only)
pub fn get_pool<'a>(state: &'a DEXState, token_a: &str, token_b: &str) -> Option<&'a LiquidityPool> {
    let key = get_pool_key(token_a, token_b);
    state.pools.get(&key)
}

/// Get LP shares for user
pub fn get_lp_shares(state: &DEXState, user: &str, token_a: &str, token_b: &str) -> u128 {
    let key = get_pool_key(token_a, token_b);
    state
        .lp_balances
        .get(user)
        .and_then(|pools| pools.get(&key))
        .copied()
        .unwrap_or(0)
}

/// Add liquidity to pool
pub fn add_liquidity(
    state: &mut DEXState,
    caller: &str,
    token_a: &str,
    token_b: &str,
    amount_a: u128,
    amount_b: u128,
) -> Result<u128, String> {
    if amount_a == 0 || amount_b == 0 {
        return Err("Cannot add zero liquidity".to_string());
    }

    let pool = get_or_create_pool(state, token_a, token_b);

    let shares: u128 = if pool.total_shares == 0 {
        // First liquidity provider
        // Shares = sqrt(amount_a * amount_b)
        // SECURITY: Using integer sqrt for deterministic consensus across all platforms
        let product = amount_a
            .checked_mul(amount_b)
            .ok_or_else(|| "Overflow in liquidity calculation".to_string())?;
        isqrt(product)
    } else {
        // Subsequent liquidity providers
        // Shares proportional to existing pool
        let share_a = amount_a
            .checked_mul(pool.total_shares)
            .ok_or_else(|| "Overflow in share_a calculation".to_string())?
            / pool.reserve_a;
        let share_b = amount_b
            .checked_mul(pool.total_shares)
            .ok_or_else(|| "Overflow in share_b calculation".to_string())?
            / pool.reserve_b;
        std::cmp::min(share_a, share_b)
    };

    if shares == 0 {
        return Err("Insufficient liquidity minted".to_string());
    }

    // Update pool reserves with overflow protection
    pool.reserve_a = pool.reserve_a
        .checked_add(amount_a)
        .ok_or_else(|| "Overflow in reserve_a".to_string())?;
    pool.reserve_b = pool.reserve_b
        .checked_add(amount_b)
        .ok_or_else(|| "Overflow in reserve_b".to_string())?;
    pool.total_shares = pool.total_shares
        .checked_add(shares)
        .ok_or_else(|| "Overflow in total_shares".to_string())?;

    // Update user LP balance with overflow protection
    let key = get_pool_key(token_a, token_b);
    let user_shares = state
        .lp_balances
        .entry(caller.to_string())
        .or_insert_with(HashMap::new)
        .entry(key)
        .or_insert(0);
    *user_shares = user_shares
        .checked_add(shares)
        .ok_or_else(|| "Overflow in user shares".to_string())?;

    println!(
        "AddLiquidity: {} added {} {}, {} {} -> {} shares",
        caller, amount_a, token_a, amount_b, token_b, shares
    );

    Ok(shares)
}

/// Remove liquidity from pool
pub fn remove_liquidity(
    state: &mut DEXState,
    caller: &str,
    token_a: &str,
    token_b: &str,
    shares: u128,
) -> Result<(u128, u128), String> {
    if shares == 0 {
        return Err("Cannot remove zero shares".to_string());
    }

    let key = get_pool_key(token_a, token_b);

    // Check user has enough shares
    let user_shares = get_lp_shares(state, caller, token_a, token_b);
    if user_shares < shares {
        return Err(format!("Insufficient shares: {} < {}", user_shares, shares));
    }

    let pool = state
        .pools
        .get_mut(&key)
        .ok_or_else(|| "Pool does not exist".to_string())?;

    // Calculate amounts to return with overflow protection
    let amount_a = shares
        .checked_mul(pool.reserve_a)
        .ok_or_else(|| "Overflow in amount_a calculation".to_string())?
        / pool.total_shares;
    let amount_b = shares
        .checked_mul(pool.reserve_b)
        .ok_or_else(|| "Overflow in amount_b calculation".to_string())?
        / pool.total_shares;

    // Update pool with underflow protection
    pool.reserve_a = pool.reserve_a
        .checked_sub(amount_a)
        .ok_or_else(|| "Underflow in reserve_a".to_string())?;
    pool.reserve_b = pool.reserve_b
        .checked_sub(amount_b)
        .ok_or_else(|| "Underflow in reserve_b".to_string())?;
    pool.total_shares = pool.total_shares
        .checked_sub(shares)
        .ok_or_else(|| "Underflow in total_shares".to_string())?;

    // Update user shares with underflow protection
    let user_balance = state
        .lp_balances
        .get_mut(caller)
        .ok_or_else(|| "User not found".to_string())?
        .get_mut(&key)
        .ok_or_else(|| "Pool balance not found".to_string())?;
    *user_balance = user_balance
        .checked_sub(shares)
        .ok_or_else(|| "Underflow in user shares".to_string())?;

    println!(
        "RemoveLiquidity: {} removed {} shares -> {} {}, {} {}",
        caller, shares, amount_a, token_a, amount_b, token_b
    );

    Ok((amount_a, amount_b))
}

/// Calculate output amount for swap (with fee)
/// Uses u128 for all calculations to support 18-decimal tokens
pub fn get_amount_out(
    reserve_in: u128,
    reserve_out: u128,
    amount_in: u128,
    fee_basis_points: u64,
) -> u128 {
    if amount_in == 0 || reserve_in == 0 || reserve_out == 0 {
        return 0;
    }

    // Apply fee: amount_in_with_fee = amount_in * (10000 - fee) / 10000
    let fee_multiplier = 10000u128 - fee_basis_points as u128;
    let amount_in_with_fee = (amount_in * fee_multiplier) / 10000;

    // Constant product formula: (x + dx) * (y - dy) = x * y
    // dy = (y * dx) / (x + dx)
    let numerator = amount_in_with_fee * reserve_out;
    let denominator = reserve_in + amount_in_with_fee;

    numerator / denominator
}

/// Swap tokens
pub fn swap(
    state: &mut DEXState,
    caller: &str,
    token_in: &str,
    token_out: &str,
    amount_in: u128,
    min_amount_out: u128,
) -> Result<u128, String> {
    if amount_in == 0 {
        return Err("Cannot swap zero amount".to_string());
    }

    let key = get_pool_key(token_in, token_out);
    let pool = state
        .pools
        .get_mut(&key)
        .ok_or_else(|| "Pool does not exist".to_string())?;

    // Determine which reserve is which
    let (reserve_in, reserve_out) = if token_in < token_out {
        (pool.reserve_a, pool.reserve_b)
    } else {
        (pool.reserve_b, pool.reserve_a)
    };

    // Calculate output amount
    let amount_out = get_amount_out(reserve_in, reserve_out, amount_in, state.fee_basis_points);

    // Slippage check
    if amount_out < min_amount_out {
        return Err(format!(
            "Slippage too high: {} < {}",
            amount_out, min_amount_out
        ));
    }

    // Update reserves with overflow/underflow protection
    if token_in < token_out {
        pool.reserve_a = pool.reserve_a
            .checked_add(amount_in)
            .ok_or_else(|| "Overflow in reserve_a".to_string())?;
        pool.reserve_b = pool.reserve_b
            .checked_sub(amount_out)
            .ok_or_else(|| "Underflow in reserve_b".to_string())?;
    } else {
        pool.reserve_b = pool.reserve_b
            .checked_add(amount_in)
            .ok_or_else(|| "Overflow in reserve_b".to_string())?;
        pool.reserve_a = pool.reserve_a
            .checked_sub(amount_out)
            .ok_or_else(|| "Underflow in reserve_a".to_string())?;
    }

    println!(
        "Swap: {} swapped {} {} for {} {}",
        caller, amount_in, token_in, amount_out, token_out
    );

    Ok(amount_out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_isqrt() {
        // Test integer square root for deterministic consensus
        assert_eq!(isqrt(0), 0);
        assert_eq!(isqrt(1), 1);
        assert_eq!(isqrt(4), 2);
        assert_eq!(isqrt(9), 3);
        assert_eq!(isqrt(100), 10);
        assert_eq!(isqrt(1000000), 1000);
        // Test non-perfect squares (floor)
        assert_eq!(isqrt(2), 1);
        assert_eq!(isqrt(10), 3);
        assert_eq!(isqrt(99), 9);
        // Test large numbers (important for 18-decimal tokens)
        assert_eq!(isqrt(1_000_000_000_000u128), 1_000_000);
        // Test with 18-decimal token amounts (10^18 * 10^18 = 10^36)
        assert_eq!(isqrt(1_000_000_000_000_000_000_000_000_000_000_000_000u128), 1_000_000_000_000_000_000u128);
    }

    #[test]
    fn test_initialize() {
        let state = initialize("owner".to_string(), 30); // 0.3% fee
        assert_eq!(state.owner, "owner");
        assert_eq!(state.fee_basis_points, 30);
    }

    #[test]
    fn test_add_liquidity_initial() {
        let mut state = initialize("owner".to_string(), 30);

        // Test with 18-decimal token amounts
        // Using 10^18 (1 token) to avoid overflow in sqrt(a*b)
        // Max safe: sqrt(u128::MAX) ≈ 1.8 × 10^19, so each amount < 1.8 × 10^19
        let amount = 1_000_000_000_000_000_000u128; // 1 token with 18 decimals
        let shares = add_liquidity(&mut state, "alice", "TOKEN_A", "TOKEN_B", amount, amount).unwrap();

        assert!(shares > 0);
        assert_eq!(get_lp_shares(&state, "alice", "TOKEN_A", "TOKEN_B"), shares);

        let pool = get_pool(&state, "TOKEN_A", "TOKEN_B").unwrap();
        assert_eq!(pool.reserve_a, amount);
        assert_eq!(pool.reserve_b, amount);
    }

    #[test]
    fn test_add_and_remove_liquidity() {
        let mut state = initialize("owner".to_string(), 30);

        let shares = add_liquidity(&mut state, "alice", "TOKEN_A", "TOKEN_B", 1000, 2000).unwrap();

        let (amount_a, amount_b) =
            remove_liquidity(&mut state, "alice", "TOKEN_A", "TOKEN_B", shares).unwrap();

        assert_eq!(amount_a, 1000);
        assert_eq!(amount_b, 2000);
    }

    #[test]
    fn test_swap() {
        let mut state = initialize("owner".to_string(), 30);

        // Add initial liquidity
        add_liquidity(&mut state, "alice", "TOKEN_A", "TOKEN_B", 1000, 1000).unwrap();

        // Swap 100 TOKEN_A for TOKEN_B
        let amount_out = swap(&mut state, "bob", "TOKEN_A", "TOKEN_B", 100, 80).unwrap();

        assert!(amount_out >= 80); // Meets minimum
        assert!(amount_out < 100); // Due to slippage and fees

        let pool = get_pool(&state, "TOKEN_A", "TOKEN_B").unwrap();
        assert_eq!(pool.reserve_a, 1100); // 1000 + 100
        assert_eq!(pool.reserve_b, 1000 - amount_out);
    }

    #[test]
    fn test_get_amount_out() {
        // 1% fee
        let amount_out = get_amount_out(1000, 1000, 100, 100);

        // With 1% fee, effective input = 99
        // Output = (1000 * 99) / (1000 + 99) ≈ 90
        assert!(amount_out < 100);
        assert!(amount_out > 0);
    }

    #[test]
    fn test_slippage_protection() {
        let mut state = initialize("owner".to_string(), 30);

        add_liquidity(&mut state, "alice", "TOKEN_A", "TOKEN_B", 1000, 1000).unwrap();

        // Try to swap with unrealistic min_amount_out
        let result = swap(&mut state, "bob", "TOKEN_A", "TOKEN_B", 100, 200);

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Slippage"));
    }

    #[test]
    fn test_large_token_amounts() {
        // Test with realistic DeFi amounts
        // Note: Initial liquidity sqrt(a*b) must fit in u128
        // Max per token for equal amounts: sqrt(u128::MAX) ≈ 1.8 × 10^19
        let mut state = initialize("owner".to_string(), 30);

        // 10 billion tokens with 18 decimals = 10^28 (safe: 10^28 * 10^28 = 10^56 overflows)
        // Use 10^19 each (max safe for equal amounts)
        let large_amount = 10_000_000_000_000_000_000u128; // 10 tokens with 18 decimals

        let shares = add_liquidity(&mut state, "whale", "USDC", "ETH", large_amount, large_amount).unwrap();
        assert!(shares > 0);

        // Swap 1 token (10^18)
        let swap_amount = 1_000_000_000_000_000_000u128;
        let result = swap(&mut state, "trader", "USDC", "ETH", swap_amount, 1);
        assert!(result.is_ok());
    }

    #[test]
    fn test_asymmetric_liquidity() {
        // Test with different amounts (USDC/ETH typical ratio)
        let mut state = initialize("owner".to_string(), 30);

        // Use smaller amounts to avoid overflow in sqrt(a*b)
        // sqrt(u128::MAX) ≈ 1.8 * 10^19, so a*b must be < 3.4 * 10^38
        // 100 * 10^18 * 1 * 10^18 = 10^38 (just within limit)
        let usdc_amount = 100_000_000_000_000_000_000u128; // 100 * 10^18
        let eth_amount = 1_000_000_000_000_000_000u128;    // 1 * 10^18

        let shares = add_liquidity(&mut state, "lp", "USDC", "ETH", usdc_amount, eth_amount).unwrap();
        assert!(shares > 0);
        // shares = sqrt(100 * 10^18 * 1 * 10^18) = sqrt(10^38) = 10^19
        assert!(shares >= 10_000_000_000_000_000_000u128);
    }
}
