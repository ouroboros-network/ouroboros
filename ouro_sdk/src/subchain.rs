use crate::client::OuroClient;
use crate::error::{Result, SdkError};
use crate::transaction::{Transaction, TransactionBuilder};
use serde::{Deserialize, Serialize};

/// Minimum deposit required to create a subchain (5,000 OURO)
pub const MIN_SUBCHAIN_DEPOSIT: u64 = 500_000_000_000;

/// Rent rate per block (0.0001 OURO)
pub const RENT_RATE_PER_BLOCK: u64 = 10_000;

/// Subchain state
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SubchainState {
    /// Active and operational
    Active,
    /// In grace period (rent depleted)
    GracePeriod,
    /// Terminated
    Terminated,
}

/// Validator configuration for subchain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorConfig {
    /// Validator public key
    pub pubkey: String,
    /// Validator stake amount
    pub stake: u64,
    /// Validator endpoint
    pub endpoint: Option<String>,
}

/// Subchain configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubchainConfig {
    /// Subchain name
    pub name: String,
    /// Owner address
    pub owner: String,
    /// Initial deposit amount (must be >= MIN_SUBCHAIN_DEPOSIT)
    pub deposit: u64,
    /// Anchor frequency (blocks)
    pub anchor_frequency: u64,
    /// RPC endpoint for the subchain
    pub rpc_endpoint: Option<String>,
    /// Validators (for BFT consensus)
    pub validators: Vec<ValidatorConfig>,
}

impl SubchainConfig {
    pub fn new(name: impl Into<String>, owner: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            owner: owner.into(),
            deposit: MIN_SUBCHAIN_DEPOSIT,
            anchor_frequency: 100,
            rpc_endpoint: None,
            validators: vec![],
        }
    }

    pub fn with_deposit(mut self, deposit: u64) -> Self {
        self.deposit = deposit;
        self
    }

    pub fn with_anchor_frequency(mut self, frequency: u64) -> Self {
        self.anchor_frequency = frequency;
        self
    }

    pub fn with_rpc_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.rpc_endpoint = Some(endpoint.into());
        self
    }

    pub fn with_validator(mut self, pubkey: impl Into<String>, stake: u64) -> Self {
        self.validators.push(ValidatorConfig {
            pubkey: pubkey.into(),
            stake,
            endpoint: None,
        });
        self
    }

    pub fn validate(&self) -> Result<()> {
        if self.name.is_empty() || self.name.len() > 64 {
            return Err(SdkError::InvalidConfig("Name must be 1-64 characters".into()));
        }
        if self.deposit < MIN_SUBCHAIN_DEPOSIT {
            return Err(SdkError::InvalidConfig(format!(
                "Deposit must be at least {} OURO",
                MIN_SUBCHAIN_DEPOSIT / 100_000_000
            )));
        }
        Ok(())
    }
}

/// Subchain status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubchainStatus {
    /// Subchain ID
    pub id: String,
    /// Subchain name
    pub name: String,
    /// Owner address
    pub owner: String,
    /// Current state
    pub state: SubchainState,
    /// Current deposit balance
    pub deposit_balance: u64,
    /// Estimated blocks remaining before rent runs out
    pub blocks_remaining: u64,
    /// Current block height
    pub block_height: u64,
    /// Total transactions processed
    pub tx_count: u64,
    /// Last anchor to mainchain
    pub last_anchor_height: Option<u64>,
    /// Number of active validators
    pub validator_count: usize,
}

/// Subchain interface for building high-scale applications
pub struct Subchain {
    /// Subchain ID
    pub id: String,
    /// Client for node communication
    client: OuroClient,
    /// Current nonce for transactions
    nonce: u64,
}

impl Subchain {
    /// Connect to an existing subchain
    pub async fn connect(subchain_id: impl Into<String>, node_url: impl Into<String>) -> Result<Self> {
        let id = subchain_id.into();
        let client = OuroClient::new(node_url);

        // Verify subchain exists
        let _status = client.get_subchain_status(&id).await?;

        Ok(Self {
            id,
            client,
            nonce: 0,
        })
    }

    /// Register a new subchain
    pub async fn register(config: SubchainConfig, node_url: impl Into<String>) -> Result<Self> {
        config.validate()?;

        let client = OuroClient::new(node_url);
        let id = client.register_subchain(&config).await?;

        Ok(Self {
            id,
            client,
            nonce: 0,
        })
    }

    /// Get subchain status
    pub async fn status(&self) -> Result<SubchainStatus> {
        self.client.get_subchain_status(&self.id).await
    }

    /// Get current deposit balance
    pub async fn deposit_balance(&self) -> Result<u64> {
        let status = self.status().await?;
        Ok(status.deposit_balance)
    }

    /// Get estimated blocks remaining
    pub async fn blocks_remaining(&self) -> Result<u64> {
        let status = self.status().await?;
        Ok(status.blocks_remaining)
    }

    /// Top up rent deposit
    pub async fn top_up_rent(&self, amount: u64) -> Result<String> {
        self.client.top_up_subchain_rent(&self.id, amount).await
    }

    /// Get balance for an address on this subchain
    pub async fn balance(&self, address: &str) -> Result<u64> {
        self.client.get_subchain_balance(&self.id, address).await
    }

    /// Submit a transaction to this subchain
    pub async fn submit_tx(&mut self, tx: &Transaction) -> Result<String> {
        let url = format!("{}/subchain/{}/tx", self.client.base_url, self.id);
        let response: serde_json::Value = self.client.client.post(&url)
            .json(tx)
            .send()
            .await?
            .json()
            .await?;

        if response["success"].as_bool().unwrap_or(false) {
            self.nonce += 1;
            Ok(response["tx_id"].as_str().unwrap_or("").to_string())
        } else {
            Err(SdkError::TransactionFailed(
                response["message"].as_str().unwrap_or("Unknown error").to_string()
            ))
        }
    }

    /// Create a transaction builder for this subchain
    pub fn tx(&self) -> TransactionBuilder {
        TransactionBuilder::new().nonce(self.nonce)
    }

    /// Transfer tokens on this subchain
    pub async fn transfer(&mut self, from: &str, to: &str, amount: u64) -> Result<String> {
        let tx = Transaction::new(from, to, amount).with_nonce(self.nonce);
        self.submit_tx(&tx).await
    }

    /// Anchor this subchain to mainchain
    pub async fn anchor(&self) -> Result<String> {
        self.client.anchor_subchain(&self.id).await
    }

    /// Get transaction history
    pub async fn tx_history(&self, from: u64, to: u64) -> Result<Vec<Transaction>> {
        let url = format!("{}/subchain/{}/txs?from={}&to={}",
            self.client.base_url, self.id, from, to);

        let response: TxHistoryResponse = self.client.client.get(&url)
            .send()
            .await?
            .json()
            .await?;

        Ok(response.transactions)
    }

    /// Add a validator to the subchain
    pub async fn add_validator(&self, validator: ValidatorConfig) -> Result<String> {
        self.client.add_subchain_validator(&self.id, &validator).await
    }

    /// Remove a validator from the subchain
    pub async fn remove_validator(&self, pubkey: &str) -> Result<String> {
        self.client.remove_subchain_validator(&self.id, pubkey).await
    }

    /// Get list of validators
    pub async fn validators(&self) -> Result<Vec<ValidatorConfig>> {
        self.client.get_subchain_validators(&self.id).await
    }

    /// Withdraw deposit (only after termination)
    pub async fn withdraw_deposit(&self) -> Result<String> {
        self.client.withdraw_subchain_deposit(&self.id).await
    }
}

#[derive(Deserialize)]
struct TxHistoryResponse {
    transactions: Vec<Transaction>,
}

/// Builder for creating subchains
pub struct SubchainBuilder {
    config: SubchainConfig,
    node_url: Option<String>,
}

impl SubchainBuilder {
    pub fn new(name: impl Into<String>, owner: impl Into<String>) -> Self {
        Self {
            config: SubchainConfig::new(name, owner),
            node_url: None,
        }
    }

    pub fn node(mut self, url: impl Into<String>) -> Self {
        self.node_url = Some(url.into());
        self
    }

    pub fn deposit(mut self, amount: u64) -> Self {
        self.config = self.config.with_deposit(amount);
        self
    }

    pub fn anchor_frequency(mut self, frequency: u64) -> Self {
        self.config = self.config.with_anchor_frequency(frequency);
        self
    }

    pub fn rpc_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.config = self.config.with_rpc_endpoint(endpoint);
        self
    }

    pub fn validator(mut self, pubkey: impl Into<String>, stake: u64) -> Self {
        self.config = self.config.with_validator(pubkey, stake);
        self
    }

    pub async fn build(self) -> Result<Subchain> {
        let node_url = self.node_url.ok_or(
            SdkError::InvalidConfig("Node URL not specified".into())
        )?;

        Subchain::register(self.config, node_url).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subchain_config() {
        let config = SubchainConfig::new("Hermes", "ouro1owner...")
            .with_deposit(1_000_000_000_000) // 10,000 OURO
            .with_anchor_frequency(50)
            .with_validator("validator1_pubkey", 100_000_000_000);

        assert_eq!(config.name, "Hermes");
        assert_eq!(config.deposit, 1_000_000_000_000);
        assert_eq!(config.anchor_frequency, 50);
        assert_eq!(config.validators.len(), 1);
    }

    #[test]
    fn test_config_validation() {
        let valid = SubchainConfig::new("Test", "owner")
            .with_deposit(MIN_SUBCHAIN_DEPOSIT);
        assert!(valid.validate().is_ok());

        let invalid = SubchainConfig::new("Test", "owner")
            .with_deposit(100); // Too low
        assert!(invalid.validate().is_err());

        let empty_name = SubchainConfig::new("", "owner");
        assert!(empty_name.validate().is_err());
    }

    #[test]
    fn test_subchain_builder() {
        let builder = SubchainBuilder::new("Hermes", "ouro1owner")
            .node("http://localhost:8001")
            .deposit(1_000_000_000_000)
            .validator("val1", 500_000_000_000);

        assert_eq!(builder.config.name, "Hermes");
        assert_eq!(builder.config.deposit, 1_000_000_000_000);
    }
}
