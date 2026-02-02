use crate::storage::{get_counter, get_str, iter_prefix, put_counter, put_str};
use crate::PgPool;
// src/ouro_coin/mod.rs
// OURO COIN: Native cryptocurrency with 103 million capped supply
// Purpose: Trading, investment, speculation (like Bitcoin/Ethereum/Monero)
// Lives on mainchain validators

pub mod api;
pub mod economics;
pub mod fee_processor;
pub mod integration;

pub use fee_processor::{
    AggregatedFees, FeeProcessingResult, FeeProcessor, FeeTransfer, TransferPurpose,
};
pub use integration::{get_total_burned, process_batch_fees, process_transaction_fee};

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

/// Total supply of OURO coin: 103,000,000 OURO (capped)
pub const TOTAL_SUPPLY: u64 = 103_000_000;

/// Decimals for OURO coin (like Bitcoin has 8 decimals)
pub const DECIMALS: u8 = 8;

/// Smallest unit (1 OURO = 10^8 units, like Bitcoin satoshis)
pub const OURO_UNIT: u64 = 100_000_000; // 10^8

/// Total supply in smallest units (103M * 10^8 = 10.3 quadrillion units)
pub const TOTAL_SUPPLY_UNITS: u64 = 10_300_000_000_000_000; // Fits in u64

/// RocksDB key prefixes for OURO coin data
const BALANCE_PREFIX: &str = "ouro:balance:";
const TRANSFER_PREFIX: &str = "ouro:transfer:";
const NONCE_PREFIX: &str = "ouro:nonce:";
const GENESIS_KEY: &str = "ouro:genesis_initialized";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OuroBalance {
    pub address: String,
    pub balance: u64, // Balance in smallest units
    pub locked: u64,  // Locked for pending transactions
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OuroTransfer {
    pub tx_id: Uuid,
    pub from_address: String,
    pub to_address: String,
    pub amount: u64, // Amount in smallest units
    pub fee: u64,
    pub signature: String,
    pub public_key: String,
    pub created_at: DateTime<Utc>,
    pub status: TransferStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TransferStatus {
    Pending,
    Confirmed,
    Failed,
}

/// Ouro Coin Manager - handles all OURO coin operations
/// Uses RocksDB for persistent storage
pub struct OuroCoinManager {
    db: Arc<PgPool>,
}

impl OuroCoinManager {
    pub fn new(db_pool: Arc<PgPool>) -> Self {
        Self { db: db_pool }
    }

    /// Get the inner DB reference for direct access
    fn db(&self) -> &crate::storage::RocksDb {
        &**self.db
    }

    /// Initialize genesis allocation
    ///
    /// Tokenomics:
    /// - Initial Circulating: 13M OURO (to genesis_address for distribution)
    /// - Locked Supply: 90M OURO (held in vesting_address, released over time)
    /// - Total: 103M OURO
    pub async fn initialize_genesis(&self, genesis_address: &str) -> Result<()> {
        self.initialize_genesis_with_vesting(genesis_address, "ouro_vesting_contract")
            .await
    }

    /// Initialize genesis with explicit vesting address
    pub async fn initialize_genesis_with_vesting(
        &self,
        genesis_address: &str,
        vesting_address: &str,
    ) -> Result<()> {
        // Check if genesis already initialized
        let initialized: Option<bool> = get_str(self.db(), GENESIS_KEY)
            .map_err(|e| anyhow!("Failed to check genesis: {}", e))?;

        if initialized.is_some() {
            return Err(anyhow!("Genesis already initialized"));
        }

        // Constants for distribution
        const INITIAL_CIRCULATING: u64 = 13_000_000; // 13M OURO
        const LOCKED_SUPPLY: u64 = 90_000_000; // 90M OURO

        let initial_circulating_units = INITIAL_CIRCULATING * OURO_UNIT;
        let locked_supply_units = LOCKED_SUPPLY * OURO_UNIT;

        // Verify total equals max supply
        assert_eq!(
            initial_circulating_units + locked_supply_units,
            TOTAL_SUPPLY_UNITS,
            "Genesis allocation must equal total supply"
        );

        // Create genesis balance (13M circulating)
        let genesis_balance = OuroBalance {
            address: genesis_address.to_string(),
            balance: initial_circulating_units,
            locked: 0,
            updated_at: Utc::now(),
        };

        // Create vesting contract balance (90M locked)
        let vesting_balance = OuroBalance {
            address: vesting_address.to_string(),
            balance: locked_supply_units,
            locked: locked_supply_units, // All locked initially
            updated_at: Utc::now(),
        };

        // Store genesis balance
        let balance_key = format!("{}{}", BALANCE_PREFIX, genesis_address);
        put_str(self.db(), &balance_key, &genesis_balance)
            .map_err(|e| anyhow!("Failed to store genesis balance: {}", e))?;

        // Store vesting contract balance
        let vesting_key = format!("{}{}", BALANCE_PREFIX, vesting_address);
        put_str(self.db(), &vesting_key, &vesting_balance)
            .map_err(|e| anyhow!("Failed to store vesting balance: {}", e))?;

        // Mark genesis as initialized
        put_str(self.db(), GENESIS_KEY, &true)
            .map_err(|e| anyhow!("Failed to mark genesis initialized: {}", e))?;

        // Store genesis timestamp for vesting calculations
        let genesis_time = Utc::now().timestamp() as u64;
        put_str(self.db(), "ouro:genesis_time", &genesis_time)
            .map_err(|e| anyhow!("Failed to store genesis time: {}", e))?;

        // Initialize nonces
        let nonce_key = format!("{}{}", NONCE_PREFIX, genesis_address);
        put_counter(self.db(), &nonce_key, 0)
            .map_err(|e| anyhow!("Failed to initialize nonce: {}", e))?;

        let vesting_nonce_key = format!("{}{}", NONCE_PREFIX, vesting_address);
        put_counter(self.db(), &vesting_nonce_key, 0)
            .map_err(|e| anyhow!("Failed to initialize vesting nonce: {}", e))?;

        log::info!("=== GENESIS INITIALIZED ===");
        log::info!(
            "Initial Circulating: {} OURO -> {}",
            INITIAL_CIRCULATING,
            genesis_address
        );
        log::info!(
            "Locked (Vesting):    {} OURO -> {}",
            LOCKED_SUPPLY,
            vesting_address
        );
        log::info!("Total Supply:        {} OURO", TOTAL_SUPPLY);
        log::info!("===========================");

        Ok(())
    }

    /// Get genesis timestamp
    pub async fn get_genesis_time(&self) -> Result<u64> {
        let time: Option<u64> = get_str(self.db(), "ouro:genesis_time")
            .map_err(|e| anyhow!("Failed to get genesis time: {}", e))?;
        Ok(time.unwrap_or(0))
    }

    /// Get balance for an address
    pub async fn get_balance(&self, address: &str) -> Result<Option<OuroBalance>> {
        let balance_key = format!("{}{}", BALANCE_PREFIX, address);
        let balance: Option<OuroBalance> = get_str(self.db(), &balance_key)
            .map_err(|e| anyhow!("Failed to get balance: {}", e))?;
        Ok(balance)
    }

    /// Get balance amount (convenience method)
    pub async fn get_balance_amount(&self, address: &str) -> Result<u64> {
        match self.get_balance(address).await? {
            Some(balance) => Ok(balance.balance),
            None => Ok(0),
        }
    }

    /// Transfer OURO from one address to another
    pub async fn transfer(
        &self,
        from: &str,
        to: &str,
        amount: u64,
        fee: u64,
        nonce: u64,
        signature: &str,
        public_key: &str,
    ) -> Result<Uuid> {
        // Verify nonce
        let expected_nonce = self.get_nonce(from).await?;
        if nonce != expected_nonce {
            return Err(anyhow!(
                "Invalid nonce: expected {}, got {}",
                expected_nonce,
                nonce
            ));
        }

        // Verify signature
        let message = format!("{}:{}:{}:{}:{}", from, to, amount, fee, nonce);
        let verified = crate::crypto::verify_ed25519_hex(public_key, signature, message.as_bytes());
        if !verified {
            return Err(anyhow!("Invalid signature"));
        }

        // Get sender balance
        let sender_balance = self
            .get_balance(from)
            .await?
            .ok_or_else(|| anyhow!("Sender has no balance"))?;

        let total_needed = amount + fee;
        if sender_balance.balance < total_needed {
            return Err(anyhow!(
                "Insufficient balance: {} < {} (amount: {} + fee: {})",
                sender_balance.balance,
                total_needed,
                amount,
                fee
            ));
        }

        // Update sender balance
        let new_sender_balance = OuroBalance {
            address: from.to_string(),
            balance: sender_balance.balance - total_needed,
            locked: sender_balance.locked,
            updated_at: Utc::now(),
        };
        let sender_key = format!("{}{}", BALANCE_PREFIX, from);
        put_str(self.db(), &sender_key, &new_sender_balance)
            .map_err(|e| anyhow!("Failed to update sender balance: {}", e))?;

        // Update receiver balance
        let receiver_balance = self.get_balance(to).await?.unwrap_or(OuroBalance {
            address: to.to_string(),
            balance: 0,
            locked: 0,
            updated_at: Utc::now(),
        });
        let new_receiver_balance = OuroBalance {
            address: to.to_string(),
            balance: receiver_balance.balance + amount,
            locked: receiver_balance.locked,
            updated_at: Utc::now(),
        };
        let receiver_key = format!("{}{}", BALANCE_PREFIX, to);
        put_str(self.db(), &receiver_key, &new_receiver_balance)
            .map_err(|e| anyhow!("Failed to update receiver balance: {}", e))?;

        // Increment sender nonce
        let nonce_key = format!("{}{}", NONCE_PREFIX, from);
        put_counter(self.db(), &nonce_key, nonce + 1)
            .map_err(|e| anyhow!("Failed to update nonce: {}", e))?;

        // Create transfer record
        let tx_id = Uuid::new_v4();
        let transfer = OuroTransfer {
            tx_id,
            from_address: from.to_string(),
            to_address: to.to_string(),
            amount,
            fee,
            signature: signature.to_string(),
            public_key: public_key.to_string(),
            created_at: Utc::now(),
            status: TransferStatus::Confirmed,
        };
        let transfer_key = format!("{}{}", TRANSFER_PREFIX, tx_id);
        put_str(self.db(), &transfer_key, &transfer)
            .map_err(|e| anyhow!("Failed to store transfer: {}", e))?;

        log::info!(
            "Transfer {} OURO ({} units) from {} to {} (fee: {} units, tx: {})",
            amount as f64 / OURO_UNIT as f64,
            amount,
            from,
            to,
            fee,
            tx_id
        );

        Ok(tx_id)
    }

    /// Direct balance update (used by consensus/rewards, no signature required)
    /// IMPORTANT: This enforces the 103M OURO supply cap to prevent inflation
    pub async fn credit(&self, address: &str, amount: u64) -> Result<()> {
        // Enforce supply cap before crediting
        let current_supply = self.get_circulating_supply().await?;
        if current_supply.saturating_add(amount) > TOTAL_SUPPLY_UNITS {
            return Err(anyhow!(
                "Credit would exceed 103M OURO supply cap: current {} + {} > {}",
                current_supply,
                amount,
                TOTAL_SUPPLY_UNITS
            ));
        }

        let balance = self.get_balance(address).await?.unwrap_or(OuroBalance {
            address: address.to_string(),
            balance: 0,
            locked: 0,
            updated_at: Utc::now(),
        });

        let new_balance = OuroBalance {
            address: address.to_string(),
            balance: balance.balance + amount,
            locked: balance.locked,
            updated_at: Utc::now(),
        };

        let balance_key = format!("{}{}", BALANCE_PREFIX, address);
        put_str(self.db(), &balance_key, &new_balance)
            .map_err(|e| anyhow!("Failed to credit balance: {}", e))?;

        log::debug!(
            "Credited {} units to {} (supply: {})",
            amount,
            address,
            current_supply + amount
        );
        Ok(())
    }

    /// Direct balance deduction (used by staking/slashing, no signature required)
    pub async fn debit(&self, address: &str, amount: u64) -> Result<()> {
        let balance = self
            .get_balance(address)
            .await?
            .ok_or_else(|| anyhow!("Address has no balance"))?;

        if balance.balance < amount {
            return Err(anyhow!(
                "Insufficient balance for debit: {} < {}",
                balance.balance,
                amount
            ));
        }

        let new_balance = OuroBalance {
            address: address.to_string(),
            balance: balance.balance - amount,
            locked: balance.locked,
            updated_at: Utc::now(),
        };

        let balance_key = format!("{}{}", BALANCE_PREFIX, address);
        put_str(self.db(), &balance_key, &new_balance)
            .map_err(|e| anyhow!("Failed to debit balance: {}", e))?;

        log::debug!("Debited {} units from {}", amount, address);
        Ok(())
    }

    /// Get total circulating supply (sum of all balances)
    pub async fn get_circulating_supply(&self) -> Result<u64> {
        let balances: Vec<OuroBalance> = iter_prefix(self.db(), BALANCE_PREFIX.as_bytes())
            .map_err(|e| anyhow!("Failed to iterate balances: {}", e))?;

        let total: u64 = balances.iter().map(|b| b.balance + b.locked).sum();
        Ok(total)
    }

    /// Verify circulating supply equals genesis allocation
    pub async fn verify_supply_integrity(&self) -> Result<bool> {
        let circulating = self.get_circulating_supply().await?;
        // Note: Supply may be less than total if some was burned
        Ok(circulating <= TOTAL_SUPPLY_UNITS)
    }

    /// Get next nonce for an address
    pub async fn get_nonce(&self, address: &str) -> Result<u64> {
        let nonce_key = format!("{}{}", NONCE_PREFIX, address);
        let nonce = get_counter(self.db(), &nonce_key)
            .map_err(|e| anyhow!("Failed to get nonce: {}", e))?;
        Ok(nonce)
    }

    /// Get a transfer by ID
    pub async fn get_transfer(&self, tx_id: &Uuid) -> Result<Option<OuroTransfer>> {
        let transfer_key = format!("{}{}", TRANSFER_PREFIX, tx_id);
        let transfer: Option<OuroTransfer> = get_str(self.db(), &transfer_key)
            .map_err(|e| anyhow!("Failed to get transfer: {}", e))?;
        Ok(transfer)
    }

    /// Get recent transfers (for address history)
    pub async fn get_recent_transfers(&self, limit: usize) -> Result<Vec<OuroTransfer>> {
        let transfers: Vec<OuroTransfer> = iter_prefix(self.db(), TRANSFER_PREFIX.as_bytes())
            .map_err(|e| anyhow!("Failed to iterate transfers: {}", e))?;

        // Sort by timestamp descending and take limit
        let mut sorted = transfers;
        sorted.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(sorted.into_iter().take(limit).collect())
    }

    /// Check if genesis has been initialized
    pub async fn is_genesis_initialized(&self) -> Result<bool> {
        let initialized: Option<bool> = get_str(self.db(), GENESIS_KEY)
            .map_err(|e| anyhow!("Failed to check genesis: {}", e))?;
        Ok(initialized.unwrap_or(false))
    }

    /// Assert supply cap is respected (panic if violated - critical invariant)
    /// Call this during block finalization as a safety check
    pub async fn assert_supply_cap(&self) -> Result<()> {
        let supply = self.get_circulating_supply().await?;
        if supply > TOTAL_SUPPLY_UNITS {
            // This is a critical protocol violation - should never happen
            log::error!(
                "CRITICAL: Supply cap violated! Current: {} > Cap: {} (overflow: {})",
                supply,
                TOTAL_SUPPLY_UNITS,
                supply - TOTAL_SUPPLY_UNITS
            );
            return Err(anyhow!(
                "Supply cap violated: {} > {} (103M OURO)",
                supply,
                TOTAL_SUPPLY_UNITS
            ));
        }
        Ok(())
    }

    /// Get supply statistics
    pub async fn get_supply_stats(&self) -> Result<SupplyStats> {
        let circulating = self.get_circulating_supply().await?;
        let remaining = TOTAL_SUPPLY_UNITS.saturating_sub(circulating);
        let burned = self.get_total_burned().await.unwrap_or(0);

        Ok(SupplyStats {
            max_supply: TOTAL_SUPPLY_UNITS,
            circulating,
            remaining,
            burned,
            cap_utilization_percent: (circulating as f64 / TOTAL_SUPPLY_UNITS as f64) * 100.0,
        })
    }

    /// Get total burned tokens (if burn tracking is enabled)
    async fn get_total_burned(&self) -> Result<u64> {
        const BURN_KEY: &str = "ouro:total_burned";
        let burned: Option<u64> = get_str(self.db(), BURN_KEY)
            .map_err(|e| anyhow!("Failed to get burned total: {}", e))?;
        Ok(burned.unwrap_or(0))
    }
}

/// Supply statistics for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupplyStats {
    /// Maximum supply cap (103M OURO in units)
    pub max_supply: u64,
    /// Current circulating supply
    pub circulating: u64,
    /// Remaining mintable supply
    pub remaining: u64,
    /// Total burned tokens
    pub burned: u64,
    /// Percentage of cap utilized
    pub cap_utilization_percent: f64,
}
