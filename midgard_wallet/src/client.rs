use anyhow::{anyhow, Result};
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::Value;

const DEFAULT_API_URL: &str = "http://localhost:8001";
const DEFAULT_API_KEY: &str = "default_api_key";

#[derive(Debug, Deserialize)]
pub struct BalanceResponse {
    pub balance: u64,
}

#[derive(Debug, Deserialize)]
pub struct TransactionResponse {
    pub tx_id: String,
}

#[derive(Debug, Deserialize)]
pub struct StatusResponse {
    pub block_height: u64,
}

#[derive(Debug, Deserialize)]
pub struct NonceResponse {
    pub nonce: u64,
}

#[derive(Debug, Deserialize)]
pub struct NodeInfoResponse {
    pub node_id: Option<String>,
    pub version: Option<String>,
    pub block_height: Option<u64>,
    pub peer_count: Option<u32>,
    pub sync_status: Option<String>,
    pub mempool_size: Option<u32>,
    pub uptime_secs: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct TransactionHistoryItem {
    pub tx_id: String,
    pub from: String,
    pub to: String,
    pub amount: u64,
    pub fee: Option<u64>,
    pub status: Option<String>,
    pub timestamp: Option<String>,
    pub block_height: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct TransactionHistoryResponse {
    pub transactions: Vec<TransactionHistoryItem>,
    pub total: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct PeerInfo {
    pub peer_id: String,
    pub address: String,
    pub latency_ms: Option<u32>,
    pub connected_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PeersResponse {
    pub peers: Vec<PeerInfo>,
    pub total: u32,
}

#[derive(Debug, Deserialize)]
pub struct MicrochainInfo {
    pub id: String,
    pub name: Option<String>,
    pub owner: Option<String>,
    pub block_height: Option<u64>,
    pub created_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MicrochainsResponse {
    pub microchains: Vec<MicrochainInfo>,
}

pub struct OuroClient {
    client: Client,
    base_url: String,
    api_key: String,
}

impl OuroClient {
    /// Create new client with custom URL
    pub fn new(url: Option<String>) -> Self {
        OuroClient {
            client: Client::new(),
            base_url: url.unwrap_or_else(|| DEFAULT_API_URL.to_string()),
            api_key: DEFAULT_API_KEY.to_string(),
        }
    }

    /// Get balance for an address
    pub fn get_balance(&self, address: &str) -> Result<u64> {
        let url = format!("{}/ouro/balance/{}", self.base_url, address);

        let response = self.client
            .get(&url)
            .send()
            .map_err(|e| anyhow!("Failed to fetch balance: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().unwrap_or_default();
            return Err(anyhow!("API error {}: {}", status, error_text));
        }

        let balance_response: BalanceResponse = response
            .json()
            .map_err(|e| anyhow!("Failed to parse balance response: {}", e))?;

        Ok(balance_response.balance)
    }

    /// Submit a transaction
    pub fn submit_transaction(&self, tx_json: Value) -> Result<String> {
        let url = format!("{}/tx/submit", self.base_url);

        let response = self.client
            .post(&url)
            .header("X-API-Key", &self.api_key)
            .json(&tx_json)
            .send()
            .map_err(|e| anyhow!("Failed to submit transaction: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().unwrap_or_default();
            return Err(anyhow!("Transaction submission failed {}: {}", status, error_text));
        }

        let tx_response: TransactionResponse = response
            .json()
            .map_err(|e| anyhow!("Failed to parse transaction response: {}", e))?;

        Ok(tx_response.tx_id)
    }

    /// Get current block height
    pub fn get_status(&self) -> Result<u64> {
        let url = format!("{}/status", self.base_url);

        let response = self.client
            .get(&url)
            .send()
            .map_err(|e| anyhow!("Failed to fetch status: {}", e))?;

        if !response.status().is_success() {
            return Err(anyhow!("Failed to get status: {}", response.status()));
        }

        let status_response: StatusResponse = response
            .json()
            .map_err(|e| anyhow!("Failed to parse status response: {}", e))?;

        Ok(status_response.block_height)
    }

    /// Health check
    pub fn health_check(&self) -> Result<bool> {
        let url = format!("{}/health", self.base_url);

        let response = self.client
            .get(&url)
            .send()
            .map_err(|e| anyhow!("Failed to connect to node: {}", e))?;

        Ok(response.status().is_success())
    }

    /// Get nonce for an address
    pub fn get_nonce(&self, address: &str) -> Result<u64> {
        let url = format!("{}/ouro/nonce/{}", self.base_url, address);

        let response = self.client
            .get(&url)
            .send()
            .map_err(|e| anyhow!("Failed to fetch nonce: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().unwrap_or_default();
            return Err(anyhow!("API error {}: {}", status, error_text));
        }

        let nonce_response: NonceResponse = response
            .json()
            .map_err(|e| anyhow!("Failed to parse nonce response: {}", e))?;

        Ok(nonce_response.nonce)
    }

    /// Get detailed node info
    pub fn get_node_info(&self) -> Result<NodeInfoResponse> {
        let url = format!("{}/", self.base_url);

        let response = self.client
            .get(&url)
            .send()
            .map_err(|e| anyhow!("Failed to fetch node info: {}", e))?;

        if !response.status().is_success() {
            return Err(anyhow!("Failed to get node info: {}", response.status()));
        }

        let info: NodeInfoResponse = response
            .json()
            .map_err(|e| anyhow!("Failed to parse node info: {}", e))?;

        Ok(info)
    }

    /// Get connected peers
    pub fn get_peers(&self) -> Result<PeersResponse> {
        let url = format!("{}/network/peers", self.base_url);

        let response = self.client
            .get(&url)
            .send()
            .map_err(|e| anyhow!("Failed to fetch peers: {}", e))?;

        if !response.status().is_success() {
            return Err(anyhow!("Failed to get peers: {}", response.status()));
        }

        let peers: PeersResponse = response
            .json()
            .map_err(|e| anyhow!("Failed to parse peers response: {}", e))?;

        Ok(peers)
    }

    /// Get transaction history for an address
    pub fn get_transaction_history(&self, address: &str, limit: u32) -> Result<TransactionHistoryResponse> {
        let url = format!("{}/ouro/transactions/{}?limit={}", self.base_url, address, limit);

        let response = self.client
            .get(&url)
            .send()
            .map_err(|e| anyhow!("Failed to fetch transactions: {}", e))?;

        if !response.status().is_success() {
            // Return empty list if endpoint not available
            return Ok(TransactionHistoryResponse {
                transactions: vec![],
                total: Some(0),
            });
        }

        let history: TransactionHistoryResponse = response
            .json()
            .map_err(|e| anyhow!("Failed to parse transactions: {}", e))?;

        Ok(history)
    }

    /// List microchains
    pub fn list_microchains(&self) -> Result<MicrochainsResponse> {
        let url = format!("{}/api/microchains", self.base_url);

        let response = self.client
            .get(&url)
            .send()
            .map_err(|e| anyhow!("Failed to fetch microchains: {}", e))?;

        if !response.status().is_success() {
            return Ok(MicrochainsResponse { microchains: vec![] });
        }

        let microchains: MicrochainsResponse = response
            .json()
            .map_err(|e| anyhow!("Failed to parse microchains: {}", e))?;

        Ok(microchains)
    }

    /// Get microchain balance
    pub fn get_microchain_balance(&self, microchain_id: &str, address: &str) -> Result<u64> {
        let url = format!("{}/api/microchains/{}/balance/{}", self.base_url, microchain_id, address);

        let response = self.client
            .get(&url)
            .send()
            .map_err(|e| anyhow!("Failed to fetch microchain balance: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().unwrap_or_default();
            return Err(anyhow!("API error {}: {}", status, error_text));
        }

        let balance_response: BalanceResponse = response
            .json()
            .map_err(|e| anyhow!("Failed to parse balance response: {}", e))?;

        Ok(balance_response.balance)
    }
}
