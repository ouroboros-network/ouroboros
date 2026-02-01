use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs;
use std::path::Path;
use chrono::Utc;
use ed25519_dalek::{SigningKey, VerifyingKey, Signature, Signer, Verifier};

/// Represents a link between a node and a wallet address
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WalletLink {
 /// OURO wallet address (bech32 format: ouro1...)
 pub wallet_address: String,

 /// Timestamp when link was created
 pub linked_at: String,

 /// Node's public key (for signature verification)
 pub node_public_key: String,

 /// Signature proving node ownership
 pub node_signature: String,

 /// Wallet's signature confirming the link
 pub wallet_signature: String,

 /// Whether this link has been verified on-chain
 #[serde(default)]
 pub verified_onchain: bool,
}

impl WalletLink {
 /// Load existing wallet link from file
 pub fn load(path: &Path) -> Result<Option<Self>, Box<dyn Error>> {
 if !path.exists() {
 return Ok(None);
 }

 let json = fs::read_to_string(path)?;
 let link: WalletLink = serde_json::from_str(&json)?;
 Ok(Some(link))
 }

 /// Save wallet link to disk
 pub fn save(&self, path: &Path) -> Result<(), Box<dyn Error>> {
 // Ensure parent directory exists
 if let Some(parent) = path.parent() {
 fs::create_dir_all(parent)?;
 }

 let json = serde_json::to_string_pretty(self)?;
 fs::write(path, json)?;
 Ok(())
 }

 /// Create a new wallet link with signatures
 pub fn create(
 wallet_address: String,
 node_keypair: &SigningKey,
 wallet_signature: String,
 ) -> Result<Self, Box<dyn Error>> {
 // Get timestamp once to ensure consistency
 let linked_at = Utc::now().to_rfc3339();
 let node_public_key = hex::encode(node_keypair.verifying_key().as_bytes());

 // Generate link message for signing
 let message = format!(
 "Link node {} to wallet {} at {}",
 node_public_key,
 wallet_address,
 linked_at
 );

 // Sign with node's private key
 let node_signature = node_keypair.sign(message.as_bytes());

 Ok(Self {
 wallet_address,
 linked_at,
 node_public_key,
 node_signature: hex::encode(node_signature.to_bytes()),
 wallet_signature,
 verified_onchain: false,
 })
 }

 /// Verify the node signature
 pub fn verify_node_signature(&self) -> Result<bool, Box<dyn Error>> {
 let public_key_bytes = hex::decode(&self.node_public_key)?;
 let public_key_array: [u8; 32] = public_key_bytes.as_slice().try_into()
 .map_err(|_| "Invalid public key length")?;
 let public_key = VerifyingKey::from_bytes(&public_key_array)?;

 let signature_bytes = hex::decode(&self.node_signature)?;
 let signature_array: [u8; 64] = signature_bytes.as_slice().try_into()
 .map_err(|_| "Invalid signature length")?;
 let signature = Signature::from_bytes(&signature_array);

 let message = format!(
 "Link node {} to wallet {} at {}",
 self.node_public_key,
 self.wallet_address,
 self.linked_at
 );

 match public_key.verify(message.as_bytes(), &signature) {
 Ok(_) => Ok(true),
 Err(_) => Ok(false),
 }
 }

 /// Unlink wallet (delete the link file)
 pub fn unlink(path: &Path) -> Result<(), Box<dyn Error>> {
 if path.exists() {
 fs::remove_file(path)?;
 }
 Ok(())
 }
}

/// Request payload for linking a wallet
#[derive(Deserialize)]
pub struct LinkWalletRequest {
 pub wallet_address: String,
 pub wallet_signature: String,
}

/// Response after successful wallet link
#[derive(Serialize)]
pub struct LinkWalletResponse {
 pub success: bool,
 pub message: String,
 pub link: Option<WalletLink>,
}

#[cfg(test)]
mod tests {
 use super::*;
 use rand::rngs::OsRng;
 use tempfile::tempdir;

 #[test]
 fn test_create_wallet_link() {
 let node_keypair = SigningKey::generate(&mut OsRng);
 let wallet_addr = "ouro1abc123xyz789".to_string();
 let wallet_sig = "fake_wallet_signature".to_string();

 let link = WalletLink::create(
 wallet_addr.clone(),
 &node_keypair,
 wallet_sig,
 ).unwrap();

 assert_eq!(link.wallet_address, wallet_addr);
 assert!(!link.node_public_key.is_empty());
 assert!(!link.node_signature.is_empty());
 assert_eq!(link.verified_onchain, false);
 }

 #[test]
 fn test_save_and_load_wallet_link() {
 let dir = tempdir().unwrap();
 let path = dir.path().join("wallet_link.json");

 let node_keypair = SigningKey::generate(&mut OsRng);

 let link = WalletLink::create(
 "ouro1test".to_string(),
 &node_keypair,
 "sig".to_string(),
 ).unwrap();

 // Save
 link.save(&path).unwrap();
 assert!(path.exists());

 // Load
 let loaded = WalletLink::load(&path).unwrap();
 assert!(loaded.is_some());

 let loaded_link = loaded.unwrap();
 assert_eq!(loaded_link.wallet_address, "ouro1test");
 assert_eq!(loaded_link.node_public_key, link.node_public_key);
 }

 #[test]
 fn test_verify_node_signature() {
 let node_keypair = SigningKey::generate(&mut OsRng);

 let link = WalletLink::create(
 "ouro1test".to_string(),
 &node_keypair,
 "wallet_sig".to_string(),
 ).unwrap();

 // Signature should be valid
 assert!(link.verify_node_signature().unwrap());
 }

 #[test]
 fn test_unlink() {
 let dir = tempdir().unwrap();
 let path = dir.path().join("wallet_link.json");

 let node_keypair = SigningKey::generate(&mut OsRng);

 let link = WalletLink::create(
 "ouro1test".to_string(),
 &node_keypair,
 "sig".to_string(),
 ).unwrap();

 link.save(&path).unwrap();
 assert!(path.exists());

 // Unlink
 WalletLink::unlink(&path).unwrap();
 assert!(!path.exists());
 }
}
