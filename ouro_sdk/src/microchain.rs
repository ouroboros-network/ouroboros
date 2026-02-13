use crate::client::OuroClient;
use crate::error::{Result, SdkError};
use crate::transaction::{Transaction, TransactionBuilder};
use crate::types::*;

/// Microchain interface for building dApps
pub struct Microchain {
    /// Microchain ID
    pub id: String,

    /// Client for node communication
    client: OuroClient,

    /// Current nonce for transactions (auto-incremented)
    nonce: u64,
}

impl Microchain {
    /// Connect to an existing microchain
    pub async fn connect(microchain_id: impl Into<String>, node_url: impl Into<String>) -> Result<Self> {
        let id = microchain_id.into();
        let client = OuroClient::new(node_url);

        // Verify microchain exists
        let _state = client.get_microchain_state(&id).await?;

        Ok(Self {
            id,
            client,
            nonce: 0,
        })
    }

    /// Create a new microchain
    pub async fn create(config: MicrochainConfig, node_url: impl Into<String>) -> Result<Self> {
        let client = OuroClient::new(node_url);
        let id = client.create_microchain(&config).await?;

        Ok(Self {
            id,
            client,
            nonce: 0,
        })
    }

    /// Get microchain state
    pub async fn state(&self) -> Result<MicrochainState> {
        self.client.get_microchain_state(&self.id).await
    }

    /// Get balance for an address on this microchain
    pub async fn balance(&self, address: &str) -> Result<u64> {
        self.client.get_microchain_balance(&self.id, address).await
    }

    /// Submit a transaction to this microchain
    pub async fn submit_tx(&mut self, tx: &Transaction) -> Result<String> {
        let url = format!("{}/microchain/{}/tx", self.client.base_url, self.id);
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

    /// Create a transaction builder for this microchain
    pub fn tx(&self) -> TransactionBuilder {
        TransactionBuilder::new().nonce(self.nonce)
    }

    /// Transfer tokens on this microchain
    pub async fn transfer(&mut self, from: &str, to: &str, amount: u64) -> Result<String> {
        let tx = Transaction::new(from, to, amount).with_nonce(self.nonce);
        self.submit_tx(&tx).await
    }

    /// Anchor this microchain to subchain/mainchain
    pub async fn anchor(&self) -> Result<String> {
        self.client.anchor_microchain(&self.id).await
    }

    /// Get transaction history for this microchain
    pub async fn tx_history(&self, from: u64, to: u64) -> Result<Vec<Transaction>> {
        let url = format!("{}/microchain/{}/txs?from={}&to={}",
            self.client.base_url, self.id, from, to);

        let response: TxHistoryResponse = self.client.client.get(&url)
            .send()
            .await?
            .json()
            .await?;

        Ok(response.transactions)
    }

    /// Get latest blocks from this microchain
    pub async fn blocks(&self, limit: u32) -> Result<Vec<BlockHeader>> {
        let url = format!("{}/microchain/{}/blocks?limit={}",
            self.client.base_url, self.id, limit);

        let response: BlocksResponse = self.client.client.get(&url)
            .send()
            .await?
            .json()
            .await?;

        Ok(response.blocks)
    }
}

// Internal response types
#[derive(serde::Deserialize)]
struct TxHistoryResponse {
    transactions: Vec<Transaction>,
}

#[derive(serde::Deserialize)]
struct BlocksResponse {
    blocks: Vec<BlockHeader>,
}

/// Builder for creating microchains
pub struct MicrochainBuilder {
    config: MicrochainConfig,
    node_url: Option<String>,
}

impl MicrochainBuilder {
    pub fn new(name: impl Into<String>, owner: impl Into<String>) -> Self {
        Self {
            config: MicrochainConfig::new(name, owner),
            node_url: None,
        }
    }

    pub fn node(mut self, url: impl Into<String>) -> Self {
        self.node_url = Some(url.into());
        self
    }

    pub fn consensus(mut self, consensus: ConsensusType) -> Self {
        self.config = self.config.with_consensus(consensus);
        self
    }

    pub fn anchor_frequency(mut self, frequency: AnchorFrequency) -> Self {
        self.config = self.config.with_anchor_frequency(frequency);
        self
    }

    pub fn block_time(mut self, seconds: u64) -> Self {
        self.config = self.config.with_block_time(seconds);
        self
    }

    pub async fn build(self) -> Result<Microchain> {
        let node_url = self.node_url.ok_or(
            SdkError::InvalidConfig("Node URL not specified".into())
        )?;

        Microchain::create(self.config, node_url).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_microchain_builder() {
        let builder = MicrochainBuilder::new("TestChain", "ouro1owner")
            .node("http://localhost:8001")
            .block_time(10);

        assert_eq!(builder.config.name, "TestChain");
        assert_eq!(builder.config.block_time_secs, 10);
    }
}
