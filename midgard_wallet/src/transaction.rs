use anyhow::Result;
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signer, SigningKey};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub id: String,
    pub sender: String,
    pub recipient: String,
    pub amount: u64,
    pub timestamp: DateTime<Utc>,
    pub parents: Vec<String>,
    pub signature: String,
    pub public_key: String,
    pub fee: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<String>,
    pub chain_id: String,
    pub nonce: u64,
}

impl Transaction {
    /// Create a new unsigned transaction
    pub fn new(
        sender: String,
        recipient: String,
        amount: u64,
        fee: u64,
        nonce: u64,
        public_key: String,
    ) -> Self {
        Transaction {
            id: Uuid::new_v4().to_string(),
            sender,
            recipient,
            amount,
            timestamp: Utc::now(),
            parents: Vec::new(),
            signature: String::new(),
            public_key,
            fee,
            payload: None,
            chain_id: "ouroboros-mainnet-1".to_string(),
            nonce,
        }
    }

    /// Build signing message (must match blockchain's signing logic)
    fn signing_message(&self) -> Vec<u8> {
        let mut msg = Vec::new();

        // Chain ID
        msg.extend_from_slice(self.chain_id.as_bytes());

        // Nonce
        msg.extend_from_slice(&self.nonce.to_le_bytes());

        // Transaction ID
        msg.extend_from_slice(self.id.as_bytes());

        // Sender
        msg.extend_from_slice(self.sender.as_bytes());

        // Recipient
        msg.extend_from_slice(self.recipient.as_bytes());

        // Amount
        msg.extend_from_slice(&self.amount.to_le_bytes());

        // Fee
        msg.extend_from_slice(&self.fee.to_le_bytes());

        // Timestamp
        msg.extend_from_slice(&self.timestamp.timestamp().to_le_bytes());

        // Parents (DAG)
        for parent in &self.parents {
            msg.extend_from_slice(parent.as_bytes());
        }

        // Payload (if any)
        if let Some(ref payload) = self.payload {
            msg.extend_from_slice(payload.as_bytes());
        }

        msg
    }

    /// Sign the transaction with private key
    pub fn sign(&mut self, signing_key: &SigningKey) -> Result<()> {
        let message = self.signing_message();
        let signature = signing_key.sign(&message);
        self.signature = hex::encode(signature.to_bytes());
        Ok(())
    }

    /// Convert transaction to API submission format
    pub fn to_api_format(&self) -> serde_json::Value {
        serde_json::json!({
            "tx_hash": self.id,
            "sender": self.sender,
            "recipient": self.recipient,
            "signature": self.signature,
            "payload": {
                "amount": self.amount,
                "fee": self.fee,
                "public_key": self.public_key
            },
            "nonce": self.nonce
        })
    }
}
