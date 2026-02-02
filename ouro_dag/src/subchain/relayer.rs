// src/subchain/relayer.rs
use crate::subchain::messages::MicroAnchorLeaf;
use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid;

/// Relayer watches microchains and forwards provisional claims.
/// Minimal example: it POSTs ProvisionalClaim to destination microchain endpoint.
/// Production: implement robust fetch, proof bundling, retries and multi-relayer.
#[derive(Serialize, Deserialize, Debug)]
pub struct CrossIntent {
    pub microchain_id: Uuid,
    pub block_id: Uuid,
    pub leaf_hash: Vec<u8>,
    pub recipient_microchain: Uuid,
    pub recipient_account: String,
    pub amount: u64,
    pub nonce: u64,
    pub sig_sender: Vec<u8>,
}

pub struct Relayer {
    client: Client,
    pub id: String,
}

impl Relayer {
    pub fn new(id: &str) -> Self {
        Self {
            client: Client::new(),
            id: id.to_string(),
        }
    }

    /// Send a provisional claim to destination microchain API.
    pub async fn send_provisional_claim(&self, dest_api: &str, intent: &CrossIntent) -> Result<()> {
        let url = format!("{}/provisional_claim", dest_api.trim_end_matches('/'));
        let resp = self
            .client
            .post(&url)
            .json(intent)
            .timeout(Duration::from_secs(8))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            anyhow::bail!("provisional claim failed: {} - {}", status, txt);
        }
        Ok(())
    }
}
