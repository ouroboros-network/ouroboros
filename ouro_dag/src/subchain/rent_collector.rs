// src/subchain/rent_collector.rs
//! Background task for automatic rent collection
//!
//! Periodically charges rent from subchains based on block height.
//! Handles grace periods and automatic suspension of unpaid subchains.

use super::registry::SubchainRegistry;
use anyhow::Result;
use std::sync::Arc;
use tokio::time::{interval, Duration};

/// Background rent collection configuration
#[derive(Debug, Clone)]
pub struct RentCollectorConfig {
    /// Interval between rent collection checks (in seconds)
    pub check_interval_secs: u64,

    /// Enable automatic rent collection
    pub enabled: bool,
}

impl Default for RentCollectorConfig {
    fn default() -> Self {
        Self {
            check_interval_secs: 10, // Check every 10 seconds (adjust based on block time)
            enabled: true,
        }
    }
}

/// Background rent collector task
pub struct RentCollector {
    /// Subchain registry
    registry: Arc<SubchainRegistry>,

    /// Configuration
    config: RentCollectorConfig,

    /// Current block height (updated externally)
    current_block: Arc<tokio::sync::RwLock<u64>>,
}

impl RentCollector {
    /// Create a new rent collector
    pub fn new(
        registry: Arc<SubchainRegistry>,
        config: RentCollectorConfig,
        current_block: Arc<tokio::sync::RwLock<u64>>,
    ) -> Self {
        Self {
            registry,
            config,
            current_block,
        }
    }

    /// Start the background rent collection task
    ///
    /// Returns a handle to the spawned task
    pub fn start(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            if !self.config.enabled {
                log::info!("BLOCKED Rent collection disabled");
                return;
            }

            log::info!(
                "BANK Starting rent collection task (check interval: {}s)",
                self.config.check_interval_secs
            );

            let mut interval = interval(Duration::from_secs(self.config.check_interval_secs));

            loop {
                interval.tick().await;

                let block_height = *self.current_block.read().await;

                if block_height == 0 {
                    // Skip if no blocks yet
                    continue;
                }

                match self.collect_rent(block_height).await {
                    Ok(stats) => {
                        if stats.subchains_charged > 0 || stats.subchains_suspended > 0 {
                            log::info!(
 "REWARD Rent collected: {} charged, {} suspended, {} OURO collected (block {})",
 stats.subchains_charged,
 stats.subchains_suspended,
 stats.total_rent_collected as f64 / 100_000_000.0, // Convert to OURO
 block_height
 );
                        }
                    }
                    Err(e) => {
                        log::error!(
                            "ERROR Rent collection failed at block {}: {}",
                            block_height,
                            e
                        );
                    }
                }
            }
        })
    }

    /// Collect rent for the current block
    async fn collect_rent(&self, block_height: u64) -> Result<RentCollectionStats> {
        let result = self.registry.collect_rent_for_block(block_height).await;

        match result {
            Ok(_total_collected) => {
                // Query to get statistics
                let stats = self.get_collection_stats(block_height).await?;
                Ok(stats)
            }
            Err(e) => {
                log::error!("Error collecting rent: {}", e);
                Err(e)
            }
        }
    }

    /// Get collection statistics for logging
    async fn get_collection_stats(&self, _block_height: u64) -> Result<RentCollectionStats> {
        // This is a simplified version - in production you'd query actual stats
        Ok(RentCollectionStats {
            subchains_charged: 0,
            subchains_suspended: 0,
            total_rent_collected: 0,
        })
    }
}

/// Statistics from a rent collection run
#[derive(Debug, Clone, Default)]
pub struct RentCollectionStats {
    /// Number of subchains charged rent
    pub subchains_charged: u64,

    /// Number of subchains suspended due to non-payment
    pub subchains_suspended: u64,

    /// Total rent collected (in smallest units)
    pub total_rent_collected: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rent_collector_config_default() {
        let config = RentCollectorConfig::default();
        assert_eq!(config.check_interval_secs, 10);
        assert!(config.enabled);
    }

    #[test]
    fn test_rent_collector_config_custom() {
        let config = RentCollectorConfig {
            check_interval_secs: 30,
            enabled: false,
        };

        assert_eq!(config.check_interval_secs, 30);
        assert!(!config.enabled);
    }
}
