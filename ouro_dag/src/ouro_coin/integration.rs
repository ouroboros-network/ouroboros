// src/ouro_coin/integration.rs
//! Integration helpers for fee distribution
//!
//! This module provides helper functions to integrate fee distribution
//! into transaction processing workflows.

use super::fee_processor::{FeeProcessingResult, FeeProcessor};
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Process transaction fees and distribute according to economic model
///
/// This should be called during transaction finalization, after validation
/// but before committing to the ledger.
///
/// # Example Integration
///
/// ```rust,ignore
/// // In your transaction processing code:
/// use crate::ouro_coin::integration::process_transaction_fee;
///
/// async fn finalize_transaction(
/// tx: &Transaction,
/// fee_processor: Arc<RwLock<FeeProcessor>>,
/// validator_addresses: &[String],
/// ) -> Result<()> {
/// // 1. Validate transaction (signature, balance, etc.)
/// validate_transaction(tx)?;
///
/// // 2. Process fee distribution
/// let fee_result = process_transaction_fee(
/// fee_processor.clone(),
/// tx.fee,
/// validator_addresses,
/// tx.developer_address.clone(),
/// ).await?;
///
/// // 3. Execute fee transfers
/// for transfer in &fee_result.transfers {
/// execute_balance_transfer(
/// &transfer.recipient,
/// transfer.amount,
/// transfer.purpose,
/// ).await?;
/// }
///
/// // 4. Record burned amount (permanently removed from circulation)
/// record_burned_tokens(fee_result.burned_amount).await?;
///
/// // 5. Complete transaction
/// commit_transaction(tx).await?;
///
/// Ok(())
/// }
/// ```
pub async fn process_transaction_fee(
    fee_processor: Arc<RwLock<FeeProcessor>>,
    fee_amount: u64,
    validator_addresses: &[String],
    developer_address: Option<String>,
) -> Result<FeeProcessingResult> {
    let mut processor = fee_processor.write().await;
    processor.process_fee(fee_amount, validator_addresses, developer_address)
}

/// Process fees for a batch of transactions
///
/// This is more efficient than processing fees individually,
/// as it aggregates all distributions before executing transfers.
///
/// # Example Integration
///
/// ```rust,ignore
/// async fn finalize_block(
/// block: &Block,
/// fee_processor: Arc<RwLock<FeeProcessor>>,
/// validator_addresses: &[String],
/// ) -> Result<()> {
/// // Collect all fee processing results
/// let mut results = Vec::new();
///
/// for tx in &block.transactions {
/// let result = process_transaction_fee(
/// fee_processor.clone(),
/// tx.fee,
/// validator_addresses,
/// tx.developer_address.clone(),
/// ).await?;
/// results.push(result);
/// }
///
/// // Aggregate all fees
/// let aggregated = FeeProcessor::aggregate_fees(&results);
///
/// // Execute aggregated transfers (more efficient)
/// for (validator, amount) in &aggregated.validator_totals {
/// credit_validator_balance(validator, *amount).await?;
/// }
///
/// credit_treasury_balance(&treasury_address, aggregated.treasury_total).await?;
///
/// for (developer, amount) in &aggregated.developer_totals {
/// credit_developer_balance(developer, *amount).await?;
/// }
///
/// // Record total burned for the block
/// record_burned_tokens(aggregated.burned_total).await?;
///
/// Ok(())
/// }
/// ```
pub async fn process_batch_fees(
    fee_processor: Arc<RwLock<FeeProcessor>>,
    transactions: &[(u64, Vec<String>, Option<String>)], // (fee, validators, dev_addr)
) -> Result<Vec<FeeProcessingResult>> {
    let mut processor = fee_processor.write().await;
    let mut results = Vec::new();

    for (fee, validators, dev_addr) in transactions {
        let result = processor.process_fee(*fee, validators, dev_addr.clone())?;
        results.push(result);
    }

    Ok(results)
}

/// Get total burned amount since last reset
pub async fn get_total_burned(fee_processor: Arc<RwLock<FeeProcessor>>) -> u64 {
    let processor = fee_processor.read().await;
    processor.get_total_burned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ouro_coin::FeeProcessor;

    #[tokio::test]
    async fn test_process_transaction_fee_integration() {
        let processor = Arc::new(RwLock::new(FeeProcessor::new("treasury".to_string())));
        let validators = vec!["val1".to_string(), "val2".to_string()];

        let result = process_transaction_fee(
            processor.clone(),
            1_000_000,
            &validators,
            Some("developer".to_string()),
        )
        .await
        .unwrap();

        assert_eq!(result.total_fee, 1_000_000);
        assert_eq!(result.burned_amount, 100_000); // 10%
        assert_eq!(result.transfers.len(), 4); // 2 validators + treasury + developer
    }

    #[tokio::test]
    async fn test_process_batch_fees_integration() {
        let processor = Arc::new(RwLock::new(FeeProcessor::new("treasury".to_string())));
        let validators = vec!["val1".to_string()];

        let transactions = vec![
            (1_000_000u64, validators.clone(), Some("dev1".to_string())),
            (500_000u64, validators.clone(), Some("dev2".to_string())),
        ];

        let results = process_batch_fees(processor.clone(), &transactions)
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].total_fee, 1_000_000);
        assert_eq!(results[1].total_fee, 500_000);
    }

    #[tokio::test]
    async fn test_get_total_burned() {
        let processor = Arc::new(RwLock::new(FeeProcessor::new("treasury".to_string())));
        let validators = vec!["val1".to_string()];

        process_transaction_fee(processor.clone(), 1_000_000, &validators, None)
            .await
            .unwrap();

        let burned = get_total_burned(processor.clone()).await;
        assert_eq!(burned, 100_000); // 10% of 1M
    }
}
