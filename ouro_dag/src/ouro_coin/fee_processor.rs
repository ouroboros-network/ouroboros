// src/ouro_coin/fee_processor.rs
//! Fee processing and distribution
//!
//! Handles the distribution of transaction fees according to the economic model:
//! - 70% to validators
//! - 10% burned (permanently removed from circulation)
//! - 10% to treasury
//! - 10% to app developer

use super::economics::{FeeAllocation, FeeDistribution};
use crate::storage::RocksDb;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Fee processor handles distribution of transaction fees
pub struct FeeProcessor {
    /// Fee distribution configuration
    distribution: FeeDistribution,

    /// Treasury address
    treasury_address: String,

    /// Burned amount tracking (permanently removed from circulation)
    total_burned: u64,

    /// Database handle for executing transfers
    db: Option<Arc<RocksDb>>,
}

impl FeeProcessor {
    /// Create a new fee processor
    pub fn new(treasury_address: String) -> Self {
        Self {
            distribution: FeeDistribution::default(),
            treasury_address,
            total_burned: 0,
            db: None,
        }
    }

    /// Create with database handle for executing transfers
    pub fn with_db(treasury_address: String, db: Arc<RocksDb>) -> Self {
        Self {
            distribution: FeeDistribution::default(),
            treasury_address,
            total_burned: 0,
            db: Some(db),
        }
    }

    /// Create with custom distribution
    pub fn with_distribution(
        treasury_address: String,
        distribution: FeeDistribution,
    ) -> Result<Self> {
        if !distribution.validate() {
            bail!("Invalid fee distribution: percentages must sum to 100%");
        }

        Ok(Self {
            distribution,
            treasury_address,
            total_burned: 0,
            db: None,
        })
    }

    /// Create with custom distribution and database
    pub fn with_distribution_and_db(
        treasury_address: String,
        distribution: FeeDistribution,
        db: Arc<RocksDb>,
    ) -> Result<Self> {
        if !distribution.validate() {
            bail!("Invalid fee distribution: percentages must sum to 100%");
        }

        Ok(Self {
            distribution,
            treasury_address,
            total_burned: 0,
            db: Some(db),
        })
    }

    /// Set database handle
    pub fn set_db(&mut self, db: Arc<RocksDb>) {
        self.db = Some(db);
    }

    /// Process a transaction fee and return distribution instructions
    ///
    /// # Arguments
    /// * `fee_amount` - Total fee amount in smallest units
    /// * `validator_addresses` - List of active validators to distribute to
    /// * `app_developer_address` - Optional address of app developer (subchain/microchain owner)
    ///
    /// # Returns
    /// Distribution instructions for the fee
    pub fn process_fee(
        &mut self,
        fee_amount: u64,
        validator_addresses: &[String],
        app_developer_address: Option<String>,
    ) -> Result<FeeProcessingResult> {
        if fee_amount == 0 {
            return Ok(FeeProcessingResult::default());
        }

        if validator_addresses.is_empty() {
            bail!("No validators available for fee distribution");
        }

        // Calculate allocation
        let allocation = self.distribution.distribute(fee_amount);

        // Distribute to validators (split equally among all active validators)
        let validator_share = if !validator_addresses.is_empty() {
            allocation.validators_amount / validator_addresses.len() as u64
        } else {
            0
        };

        let mut transfers = Vec::new();

        // Add validator transfers
        for validator_addr in validator_addresses {
            transfers.push(FeeTransfer {
                recipient: validator_addr.clone(),
                amount: validator_share,
                purpose: TransferPurpose::ValidatorReward,
            });
        }

        // Add treasury transfer
        transfers.push(FeeTransfer {
            recipient: self.treasury_address.clone(),
            amount: allocation.treasury_amount,
            purpose: TransferPurpose::Treasury,
        });

        // Add app developer transfer (if specified)
        if let Some(dev_addr) = app_developer_address {
            transfers.push(FeeTransfer {
                recipient: dev_addr,
                amount: allocation.app_developer_amount,
                purpose: TransferPurpose::AppDeveloper,
            });
        } else {
            // If no developer specified, add to treasury
            if let Some(treasury_transfer) = transfers
                .iter_mut()
                .find(|t| t.purpose == TransferPurpose::Treasury)
            {
                treasury_transfer.amount += allocation.app_developer_amount;
            }
        }

        // Track burned amount
        let burned_amount = allocation.burn_amount;
        self.total_burned += burned_amount;

        Ok(FeeProcessingResult {
            total_fee: fee_amount,
            allocation,
            transfers,
            burned_amount,
        })
    }

    /// Execute fee transfers by updating balances in the database
    ///
    /// # Arguments
    /// * `result` - The fee processing result containing transfers to execute
    /// * `fee_payer` - Address that paid the fee (will have balance deducted)
    ///
    /// # Returns
    /// Ok(()) if all transfers executed successfully
    pub fn execute_transfers(&self, result: &FeeProcessingResult, fee_payer: &str) -> Result<()> {
        let db = self
            .db
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Database handle not set - cannot execute transfers"))?;

        // Deduct fee from payer
        let payer_balance = self.get_balance(db, fee_payer)?;
        if payer_balance < result.total_fee {
            bail!(
                "Insufficient balance: {} has {} but fee is {}",
                fee_payer,
                payer_balance,
                result.total_fee
            );
        }
        self.set_balance(db, fee_payer, payer_balance - result.total_fee)?;

        // Execute each transfer (credit recipients)
        for transfer in &result.transfers {
            let recipient_balance = self.get_balance(db, &transfer.recipient)?;
            self.set_balance(db, &transfer.recipient, recipient_balance + transfer.amount)?;

            log::debug!(
                "Fee transfer: {} -> {} ({:?})",
                transfer.amount,
                transfer.recipient,
                transfer.purpose
            );
        }

        // Record burned amount (burns are tracked but no balance update needed - tokens are destroyed)
        if result.burned_amount > 0 {
            let burned_key = "total_burned_fees";
            let current_burned: u64 = crate::storage::get_str::<u64>(db, burned_key)
                .unwrap_or(Some(0))
                .unwrap_or(0);
            crate::storage::put_str(
                db,
                burned_key,
                &(current_burned + result.burned_amount).to_string(),
            )
            .map_err(|e| anyhow::anyhow!("Failed to update burned total: {}", e))?;

            log::info!(
                "Burned {} units from fee (total burned: {})",
                result.burned_amount,
                current_burned + result.burned_amount
            );
        }

        Ok(())
    }

    /// Process fee and immediately execute transfers
    ///
    /// Convenience method that combines process_fee and execute_transfers
    pub fn process_and_execute(
        &mut self,
        fee_amount: u64,
        fee_payer: &str,
        validator_addresses: &[String],
        app_developer_address: Option<String>,
    ) -> Result<FeeProcessingResult> {
        let result = self.process_fee(fee_amount, validator_addresses, app_developer_address)?;
        self.execute_transfers(&result, fee_payer)?;
        Ok(result)
    }

    /// Execute aggregated fees in batch
    ///
    /// More efficient for processing multiple transactions at once
    pub fn execute_aggregated(
        &self,
        aggregated: &AggregatedFees,
        fee_payers: &[(String, u64)],
    ) -> Result<()> {
        let db = self
            .db
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Database handle not set - cannot execute transfers"))?;

        // Deduct fees from payers
        for (payer, amount) in fee_payers {
            let payer_balance = self.get_balance(db, payer)?;
            if payer_balance < *amount {
                bail!(
                    "Insufficient balance: {} has {} but owes {}",
                    payer,
                    payer_balance,
                    amount
                );
            }
            self.set_balance(db, payer, payer_balance - amount)?;
        }

        // Credit validators
        for (validator, amount) in &aggregated.validator_totals {
            let balance = self.get_balance(db, validator)?;
            self.set_balance(db, validator, balance + amount)?;
        }

        // Credit treasury
        let treasury_balance = self.get_balance(db, &self.treasury_address)?;
        self.set_balance(
            db,
            &self.treasury_address,
            treasury_balance + aggregated.treasury_total,
        )?;

        // Credit developers
        for (developer, amount) in &aggregated.developer_totals {
            let balance = self.get_balance(db, developer)?;
            self.set_balance(db, developer, balance + amount)?;
        }

        // Record burned amount
        if aggregated.burned_total > 0 {
            let burned_key = "total_burned_fees";
            let current_burned: u64 = crate::storage::get_str::<u64>(db, burned_key)
                .unwrap_or(Some(0))
                .unwrap_or(0);
            crate::storage::put_str(
                db,
                burned_key,
                &(current_burned + aggregated.burned_total).to_string(),
            )
            .map_err(|e| anyhow::anyhow!("Failed to update burned total: {}", e))?;
        }

        log::info!(
            "Executed aggregated fees: total={}, validators={}, treasury={}, burned={}",
            aggregated.total_fees,
            aggregated.validator_totals.len(),
            aggregated.treasury_total,
            aggregated.burned_total
        );

        Ok(())
    }

    /// Get balance for an address
    fn get_balance(&self, db: &RocksDb, address: &str) -> Result<u64> {
        let key = format!("balance:{}", address);
        Ok(crate::storage::get_str::<u64>(db, &key)
            .map_err(|e| anyhow::anyhow!("Failed to read balance: {}", e))?
            .unwrap_or(0))
    }

    /// Set balance for an address
    fn set_balance(&self, db: &RocksDb, address: &str, balance: u64) -> Result<()> {
        let key = format!("balance:{}", address);
        crate::storage::put_str(db, &key, &balance.to_string())
            .map_err(|e| anyhow::anyhow!("Failed to set balance: {}", e))
    }

    /// Get total burned amount
    pub fn get_total_burned(&self) -> u64 {
        self.total_burned
    }

    /// Reset burned counter (for testing or accounting periods)
    pub fn reset_burned_counter(&mut self) {
        self.total_burned = 0;
    }

    /// Get current fee distribution configuration
    pub fn get_distribution(&self) -> &FeeDistribution {
        &self.distribution
    }

    /// Aggregate multiple fee distributions for batch processing
    ///
    /// This is useful for processing a block of transactions at once
    pub fn aggregate_fees(results: &[FeeProcessingResult]) -> AggregatedFees {
        let mut validator_totals: HashMap<String, u64> = HashMap::new();
        let mut treasury_total = 0u64;
        let mut developer_totals: HashMap<String, u64> = HashMap::new();
        let mut burned_total = 0u64;
        let mut total_fees = 0u64;

        for result in results {
            total_fees += result.total_fee;
            burned_total += result.burned_amount;

            for transfer in &result.transfers {
                match transfer.purpose {
                    TransferPurpose::ValidatorReward => {
                        *validator_totals
                            .entry(transfer.recipient.clone())
                            .or_insert(0) += transfer.amount;
                    }
                    TransferPurpose::Treasury => {
                        treasury_total += transfer.amount;
                    }
                    TransferPurpose::AppDeveloper => {
                        *developer_totals
                            .entry(transfer.recipient.clone())
                            .or_insert(0) += transfer.amount;
                    }
                }
            }
        }

        AggregatedFees {
            total_fees,
            validator_totals,
            treasury_total,
            developer_totals,
            burned_total,
        }
    }
}

/// Result of processing a single transaction fee
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeProcessingResult {
    /// Total fee amount
    pub total_fee: u64,

    /// Fee allocation breakdown
    pub allocation: FeeAllocation,

    /// List of transfers to execute
    pub transfers: Vec<FeeTransfer>,

    /// Amount burned (removed from circulation)
    pub burned_amount: u64,
}

impl Default for FeeProcessingResult {
    fn default() -> Self {
        Self {
            total_fee: 0,
            allocation: FeeAllocation {
                validators_amount: 0,
                burn_amount: 0,
                treasury_amount: 0,
                app_developer_amount: 0,
            },
            transfers: Vec::new(),
            burned_amount: 0,
        }
    }
}

/// Individual fee transfer instruction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeTransfer {
    /// Recipient address
    pub recipient: String,

    /// Amount in smallest units
    pub amount: u64,

    /// Purpose of transfer
    pub purpose: TransferPurpose,
}

/// Purpose of a fee transfer
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferPurpose {
    /// Validator block reward
    ValidatorReward,

    /// Treasury allocation
    Treasury,

    /// App developer fee
    AppDeveloper,
}

/// Aggregated fees from multiple transactions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedFees {
    /// Total fees collected
    pub total_fees: u64,

    /// Total per validator (address -> amount)
    pub validator_totals: HashMap<String, u64>,

    /// Total to treasury
    pub treasury_total: u64,

    /// Total per developer (address -> amount)
    pub developer_totals: HashMap<String, u64>,

    /// Total burned
    pub burned_total: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fee_processing_basic() {
        let mut processor = FeeProcessor::new("treasury_addr".to_string());

        let validators = vec!["val1".to_string(), "val2".to_string()];
        let result = processor
            .process_fee(1_000_000, &validators, Some("dev_addr".to_string()))
            .unwrap();

        // Verify allocation
        assert_eq!(result.allocation.validators_amount, 700_000); // 70%
        assert_eq!(result.allocation.burn_amount, 100_000); // 10%
        assert_eq!(result.allocation.treasury_amount, 100_000); // 10%
        assert_eq!(result.allocation.app_developer_amount, 100_000); // 10%

        // Verify transfers
        assert_eq!(result.transfers.len(), 4); // 2 validators + treasury + developer

        // Each validator gets half of 70%
        let validator_transfers: Vec<_> = result
            .transfers
            .iter()
            .filter(|t| t.purpose == TransferPurpose::ValidatorReward)
            .collect();
        assert_eq!(validator_transfers.len(), 2);
        for transfer in validator_transfers {
            assert_eq!(transfer.amount, 350_000); // 700_000 / 2
        }

        // Verify burned amount tracking
        assert_eq!(processor.get_total_burned(), 100_000);
    }

    #[test]
    fn test_fee_processing_no_developer() {
        let mut processor = FeeProcessor::new("treasury_addr".to_string());

        let validators = vec!["val1".to_string()];
        let result = processor.process_fee(1_000_000, &validators, None).unwrap();

        // Developer share should go to treasury
        let treasury_transfer = result
            .transfers
            .iter()
            .find(|t| t.purpose == TransferPurpose::Treasury)
            .unwrap();

        // Treasury gets 10% + 10% (from developer) = 20%
        assert_eq!(treasury_transfer.amount, 200_000);
    }

    #[test]
    fn test_aggregated_fees() {
        let mut processor = FeeProcessor::new("treasury_addr".to_string());

        let validators = vec!["val1".to_string(), "val2".to_string()];

        // Process 3 transactions
        let results: Vec<_> = (0..3)
            .map(|_| {
                processor
                    .process_fee(1_000_000, &validators, Some("dev_addr".to_string()))
                    .unwrap()
            })
            .collect();

        let aggregated = FeeProcessor::aggregate_fees(&results);

        assert_eq!(aggregated.total_fees, 3_000_000);
        assert_eq!(aggregated.burned_total, 300_000); // 10% * 3
        assert_eq!(aggregated.treasury_total, 300_000); // 10% * 3

        // Each validator gets 350k per tx * 3 txs
        assert_eq!(aggregated.validator_totals.get("val1"), Some(&1_050_000));
        assert_eq!(aggregated.validator_totals.get("val2"), Some(&1_050_000));

        // Developer gets 10% * 3
        assert_eq!(aggregated.developer_totals.get("dev_addr"), Some(&300_000));
    }

    #[test]
    fn test_fee_processing_zero_fee() {
        let mut processor = FeeProcessor::new("treasury_addr".to_string());

        let validators = vec!["val1".to_string()];
        let result = processor.process_fee(0, &validators, None).unwrap();

        assert_eq!(result.total_fee, 0);
        assert_eq!(result.transfers.len(), 0);
        assert_eq!(result.burned_amount, 0);
    }

    #[test]
    fn test_invalid_distribution() {
        let bad_distribution = FeeDistribution {
            validators: 0.50,
            burn: 0.20,
            treasury: 0.20,
            app_developer: 0.20, // Sum is 110%, not 100%
        };

        let result = FeeProcessor::with_distribution("treasury_addr".to_string(), bad_distribution);
        assert!(result.is_err());
    }
}
