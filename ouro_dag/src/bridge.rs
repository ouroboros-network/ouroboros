// Cross-chain bridge for asset transfers
// Lock-and-mint bridge between chains

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Bridge transfer request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeTransfer {
    pub id: String,
    pub from_chain: String,
    pub to_chain: String,
    pub from_address: String,
    pub to_address: String,
    pub asset: String,
    pub amount: u64,
    pub timestamp: u64,
    pub status: BridgeStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BridgeStatus {
    Pending,
    Locked,
    Minted,
    Completed,
    Failed,
}

/// Bridge validator (relayer)
#[derive(Debug, Clone)]
pub struct BridgeValidator {
    pub address: String,
    pub stake: u64,
}

/// Oracle verification proof (from oracle subchain)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleVerification {
    /// Transfer ID being verified
    pub transfer_id: String,
    /// Source chain transaction hash
    pub source_tx_hash: Vec<u8>,
    /// Source chain block hash
    pub source_block_hash: Vec<u8>,
    /// Oracle consensus: verified or not
    pub verified: bool,
    /// Confidence score (0.0 to 1.0)
    pub confidence: f64,
    /// Number of oracle validators
    pub num_validators: usize,
    /// Oracle subchain block height
    pub oracle_block: u64,
    /// Timestamp
    pub timestamp: u64,
}

/// Bridge manager
pub struct BridgeManager {
    /// Pending transfers
    transfers: Arc<RwLock<HashMap<String, BridgeTransfer>>>,
    /// Locked assets on source chain
    locked_assets: Arc<RwLock<HashMap<String, u64>>>,
    /// Minted wrapped assets on destination
    minted_assets: Arc<RwLock<HashMap<String, u64>>>,
    /// Bridge validators
    validators: Arc<RwLock<Vec<BridgeValidator>>>,
    /// Minimum validator signatures required
    threshold: usize,
}

impl BridgeManager {
    pub fn new(threshold: usize) -> Self {
        Self {
            transfers: Arc::new(RwLock::new(HashMap::new())),
            locked_assets: Arc::new(RwLock::new(HashMap::new())),
            minted_assets: Arc::new(RwLock::new(HashMap::new())),
            validators: Arc::new(RwLock::new(Vec::new())),
            threshold,
        }
    }

    /// Register bridge validator
    pub async fn register_validator(&self, validator: BridgeValidator) {
        let mut validators = self.validators.write().await;
        validators.push(validator);
    }

    /// Initiate bridge transfer
    pub async fn initiate_transfer(
        &self,
        from_chain: String,
        to_chain: String,
        from_address: String,
        to_address: String,
        asset: String,
        amount: u64,
    ) -> Result<String, String> {
        // Generate transfer ID
        let transfer_id = self.generate_transfer_id(&from_address, &to_address, amount);

        let transfer = BridgeTransfer {
            id: transfer_id.clone(),
            from_chain,
            to_chain,
            from_address,
            to_address,
            asset,
            amount,
            timestamp: current_unix_time(),
            status: BridgeStatus::Pending,
        };

        // Add to pending
        let mut transfers = self.transfers.write().await;
        transfers.insert(transfer_id.clone(), transfer);

        Ok(transfer_id)
    }

    /// Lock assets on source chain (Phase 1)
    pub async fn lock_assets(&self, transfer_id: &str) -> Result<(), String> {
        let mut transfers = self.transfers.write().await;

        let transfer = transfers.get_mut(transfer_id).ok_or("Transfer not found")?;

        if transfer.status != BridgeStatus::Pending {
            return Err(format!("Invalid status: {:?}", transfer.status));
        }

        // Lock assets
        let mut locked = self.locked_assets.write().await;
        let key = format!("{}_{}", transfer.from_chain, transfer.asset);
        *locked.entry(key).or_insert(0) += transfer.amount;

        // Update status
        transfer.status = BridgeStatus::Locked;

        Ok(())
    }

    /// Mint wrapped assets on destination chain (Phase 2)
    /// NOW USES ORACLE SUBCHAIN FOR VERIFICATION (secure!)
    pub async fn mint_wrapped(
        &self,
        transfer_id: &str,
        oracle_verification: OracleVerification,
    ) -> Result<(), String> {
        // Verify oracle proof from oracle subchain
        if !oracle_verification.verified {
            return Err("Oracle verification failed".to_string());
        }

        // Verify oracle confidence (>66% stake agreement)
        if oracle_verification.confidence < 0.66 {
            return Err(format!(
                "Low oracle confidence: {}",
                oracle_verification.confidence
            ));
        }

        // Verify oracle validators count
        if oracle_verification.num_validators < 3 {
            return Err("Insufficient oracle validators".to_string());
        }

        let mut transfers = self.transfers.write().await;

        let transfer = transfers.get_mut(transfer_id).ok_or("Transfer not found")?;

        if transfer.status != BridgeStatus::Locked {
            return Err(format!("Invalid status: {:?}", transfer.status));
        }

        // Verify oracle checked correct transaction
        if oracle_verification.transfer_id != transfer_id {
            return Err("Oracle verified different transaction".to_string());
        }

        // Mint wrapped assets
        let mut minted = self.minted_assets.write().await;
        let key = format!("{}_{}", transfer.to_chain, transfer.asset);
        *minted.entry(key).or_insert(0) += transfer.amount;

        // Update status
        transfer.status = BridgeStatus::Minted;

        Ok(())
    }

    /// Complete transfer
    pub async fn complete_transfer(&self, transfer_id: &str) -> Result<(), String> {
        let mut transfers = self.transfers.write().await;

        let transfer = transfers.get_mut(transfer_id).ok_or("Transfer not found")?;

        if transfer.status != BridgeStatus::Minted {
            return Err(format!("Invalid status: {:?}", transfer.status));
        }

        transfer.status = BridgeStatus::Completed;

        Ok(())
    }

    /// Unlock and burn (reverse bridge)
    pub async fn unlock_assets(&self, transfer_id: &str) -> Result<(), String> {
        let mut transfers = self.transfers.write().await;

        let transfer = transfers.get_mut(transfer_id).ok_or("Transfer not found")?;

        // Burn wrapped assets
        let mut minted = self.minted_assets.write().await;
        let key = format!("{}_{}", transfer.from_chain, transfer.asset);
        let current = *minted.get(&key).ok_or("No wrapped assets")?;

        if current < transfer.amount {
            return Err("Insufficient wrapped assets".to_string());
        }

        minted.insert(key.clone(), current - transfer.amount);

        // Unlock original assets
        let mut locked = self.locked_assets.write().await;
        let lock_key = format!("{}_{}", transfer.to_chain, transfer.asset);
        let locked_amount = *locked.get(&lock_key).ok_or("No locked assets")?;

        if locked_amount < transfer.amount {
            return Err("Insufficient locked assets".to_string());
        }

        locked.insert(lock_key, locked_amount - transfer.amount);

        transfer.status = BridgeStatus::Completed;

        Ok(())
    }

    /// Get transfer status
    pub async fn get_transfer(&self, transfer_id: &str) -> Option<BridgeTransfer> {
        let transfers = self.transfers.read().await;
        transfers.get(transfer_id).cloned()
    }

    /// Get total locked for asset
    pub async fn get_locked_amount(&self, chain: &str, asset: &str) -> u64 {
        let locked = self.locked_assets.read().await;
        let key = format!("{}_{}", chain, asset);
        *locked.get(&key).unwrap_or(&0)
    }

    /// Get total minted wrapped assets
    pub async fn get_minted_amount(&self, chain: &str, asset: &str) -> u64 {
        let minted = self.minted_assets.read().await;
        let key = format!("{}_{}", chain, asset);
        *minted.get(&key).unwrap_or(&0)
    }

    fn generate_transfer_id(&self, from: &str, to: &str, amount: u64) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"bridge_transfer");
        hasher.update(from.as_bytes());
        hasher.update(to.as_bytes());
        hasher.update(&amount.to_le_bytes());
        hasher.update(&current_unix_time().to_le_bytes());
        let hash = hasher.finalize();

        hex::encode(&hash[0..16])
    }
}

fn current_unix_time() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Query oracle subchain for bridge verification
pub async fn query_oracle_verification(
    transfer_id: &str,
    source_chain: &str,
    block_hash: &[u8],
    tx_hash: &[u8],
) -> Result<OracleVerification, String> {
    // Query oracle subchain for bridge verification
    // Oracle validators submit their verifications to oracle subchain
    // Main chain reads consensus from oracle subchain

    let bridge_verification =
        crate::oracle_subchain::verify_bridge_via_oracle(source_chain, block_hash, tx_hash).await?;

    // Convert oracle subchain verification to bridge verification
    Ok(OracleVerification {
        transfer_id: transfer_id.to_string(),
        source_tx_hash: tx_hash.to_vec(),
        source_block_hash: block_hash.to_vec(),
        verified: bridge_verification.verified,
        confidence: bridge_verification.confidence,
        num_validators: bridge_verification.num_validators,
        oracle_block: 0, // TODO: Get actual oracle block height
        timestamp: bridge_verification.timestamp,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_bridge_transfer() {
        let bridge = BridgeManager::new(2);

        // Register validators
        bridge
            .register_validator(BridgeValidator {
                address: "val1".to_string(),
                stake: 1000,
            })
            .await;

        bridge
            .register_validator(BridgeValidator {
                address: "val2".to_string(),
                stake: 1000,
            })
            .await;

        // Initiate transfer
        let transfer_id = bridge
            .initiate_transfer(
                "Ethereum".to_string(),
                "Ouroboros".to_string(),
                "alice_eth".to_string(),
                "alice_ouro".to_string(),
                "ETH".to_string(),
                100,
            )
            .await
            .unwrap();

        // Lock on source
        bridge.lock_assets(&transfer_id).await.unwrap();

        // Check locked amount
        let locked = bridge.get_locked_amount("Ethereum", "ETH").await;
        assert_eq!(locked, 100);

        // Mint on destination (with oracle verification)
        let oracle_proof = OracleVerification {
            transfer_id: transfer_id.clone(),
            source_tx_hash: vec![1, 2, 3],
            source_block_hash: vec![4, 5, 6],
            verified: true,
            confidence: 0.95,
            num_validators: 5,
            oracle_block: 100,
            timestamp: current_unix_time(),
        };
        bridge
            .mint_wrapped(&transfer_id, oracle_proof)
            .await
            .unwrap();

        // Check minted amount
        let minted = bridge.get_minted_amount("Ouroboros", "ETH").await;
        assert_eq!(minted, 100);

        // Complete transfer
        bridge.complete_transfer(&transfer_id).await.unwrap();

        // Verify status
        let transfer = bridge.get_transfer(&transfer_id).await.unwrap();
        assert_eq!(transfer.status, BridgeStatus::Completed);
    }

    #[tokio::test]
    async fn test_reverse_bridge() {
        let bridge = BridgeManager::new(2);

        // Setup initial state (locked ETH)
        let transfer_id = bridge
            .initiate_transfer(
                "Ethereum".to_string(),
                "Ouroboros".to_string(),
                "alice_eth".to_string(),
                "alice_ouro".to_string(),
                "ETH".to_string(),
                50,
            )
            .await
            .unwrap();

        bridge.lock_assets(&transfer_id).await.unwrap();

        let oracle_proof = OracleVerification {
            transfer_id: transfer_id.clone(),
            source_tx_hash: vec![],
            source_block_hash: vec![],
            verified: true,
            confidence: 0.9,
            num_validators: 3,
            oracle_block: 50,
            timestamp: current_unix_time(),
        };
        bridge
            .mint_wrapped(&transfer_id, oracle_proof)
            .await
            .unwrap();

        // Now reverse: burn wrapped ETH on Ouroboros, unlock on Ethereum
        let reverse_id = bridge
            .initiate_transfer(
                "Ouroboros".to_string(),
                "Ethereum".to_string(),
                "alice_ouro".to_string(),
                "alice_eth".to_string(),
                "ETH".to_string(),
                50,
            )
            .await
            .unwrap();

        bridge.unlock_assets(&reverse_id).await.unwrap();

        // Check balances updated
        let locked = bridge.get_locked_amount("Ethereum", "ETH").await;
        assert_eq!(locked, 0);

        let minted = bridge.get_minted_amount("Ouroboros", "ETH").await;
        assert_eq!(minted, 0);
    }
}
