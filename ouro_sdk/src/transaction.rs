use crate::error::{Result, SdkError};
use ed25519_dalek::{SigningKey, Signer};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Transaction for microchain or mainchain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    /// Unique transaction ID
    pub id: String,

    /// Sender address
    pub from: String,

    /// Recipient address
    pub to: String,

    /// Amount to transfer
    pub amount: u64,

    /// Transaction nonce (prevents replay)
    pub nonce: u64,

    /// Ed25519 signature
    pub signature: String,

    /// Optional custom data (for smart contract calls, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,

    /// Timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

impl Transaction {
    /// Create a new transaction
    pub fn new(from: impl Into<String>, to: impl Into<String>, amount: u64) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            from: from.into(),
            to: to.into(),
            amount,
            nonce: 0,
            signature: String::new(),
            data: None,
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
        }
    }

    /// Set nonce
    pub fn with_nonce(mut self, nonce: u64) -> Self {
        self.nonce = nonce;
        self
    }

    /// Add custom data
    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }

    /// Sign transaction with signing key
    pub fn sign(&mut self, signing_key: &SigningKey) -> Result<()> {
        let message = self.signing_message();
        let signature = signing_key.sign(message.as_bytes());
        self.signature = hex::encode(signature.to_bytes());
        Ok(())
    }

    /// Sign transaction with private key hex
    pub fn sign_with_key(&mut self, private_key_hex: &str) -> Result<()> {
        let private_bytes = hex::decode(private_key_hex)
            .map_err(|_| SdkError::InvalidSignature)?;
        let private_array: [u8; 32] = private_bytes
            .try_into()
            .map_err(|_| SdkError::InvalidSignature)?;
        let signing_key = SigningKey::from_bytes(&private_array);
        self.sign(&signing_key)
    }

    /// Get signing message
    fn signing_message(&self) -> String {
        format!(
            "{}:{}:{}:{}:{}",
            self.id, self.from, self.to, self.amount, self.nonce
        )
    }

    /// Verify transaction signature
    pub fn verify(&self) -> Result<bool> {
        // TODO: Implement signature verification
        // Need public key from 'from' address
        Ok(!self.signature.is_empty())
    }
}

/// Builder for creating transactions
pub struct TransactionBuilder {
    from: Option<String>,
    to: Option<String>,
    amount: Option<u64>,
    nonce: u64,
    data: Option<serde_json::Value>,
}

impl TransactionBuilder {
    pub fn new() -> Self {
        Self {
            from: None,
            to: None,
            amount: None,
            nonce: 0,
            data: None,
        }
    }

    pub fn from(mut self, from: impl Into<String>) -> Self {
        self.from = Some(from.into());
        self
    }

    pub fn to(mut self, to: impl Into<String>) -> Self {
        self.to = Some(to.into());
        self
    }

    pub fn amount(mut self, amount: u64) -> Self {
        self.amount = Some(amount);
        self
    }

    pub fn nonce(mut self, nonce: u64) -> Self {
        self.nonce = nonce;
        self
    }

    pub fn data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }

    pub fn build(self) -> Result<Transaction> {
        let from = self.from.ok_or(SdkError::InvalidConfig("Missing 'from' address".into()))?;
        let to = self.to.ok_or(SdkError::InvalidConfig("Missing 'to' address".into()))?;
        let amount = self.amount.ok_or(SdkError::InvalidConfig("Missing amount".into()))?;

        let mut tx = Transaction::new(from, to, amount).with_nonce(self.nonce);
        if let Some(data) = self.data {
            tx = tx.with_data(data);
        }

        Ok(tx)
    }
}

impl Default for TransactionBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_creation() {
        let tx = Transaction::new("ouro1from", "ouro1to", 1000);
        assert_eq!(tx.from, "ouro1from");
        assert_eq!(tx.to, "ouro1to");
        assert_eq!(tx.amount, 1000);
        assert!(!tx.id.is_empty());
    }

    #[test]
    fn test_transaction_builder() {
        let tx = TransactionBuilder::new()
            .from("ouro1alice")
            .to("ouro1bob")
            .amount(500)
            .nonce(1)
            .build()
            .unwrap();

        assert_eq!(tx.from, "ouro1alice");
        assert_eq!(tx.to, "ouro1bob");
        assert_eq!(tx.amount, 500);
        assert_eq!(tx.nonce, 1);
    }

    #[test]
    fn test_builder_validation() {
        let result = TransactionBuilder::new()
            .from("ouro1alice")
            .amount(100)
            .build();

        assert!(result.is_err()); // Missing 'to' address
    }
}
