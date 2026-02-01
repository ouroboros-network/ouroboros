use anyhow::{anyhow, Result};
use bech32::{Bech32, Hrp};
use bip39::{Language, Mnemonic};
use ed25519_dalek::{SigningKey, VerifyingKey, SECRET_KEY_LENGTH};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use zeroize::{Zeroize, ZeroizeOnDrop};

// Encryption imports
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use pbkdf2::pbkdf2_hmac;
use sha2::Sha256;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

const WALLET_FILE: &str = "midgard_wallet.json";
const PBKDF2_ITERATIONS: u32 = 600_000; // OWASP recommended minimum
const SALT_LENGTH: usize = 32;
const NONCE_LENGTH: usize = 12;

/// Encrypted wallet file format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedWalletFile {
    pub version: u32,
    pub name: String,
    pub address: String,
    pub public_key: String,
    pub created_at: String,
    /// Base64-encoded encrypted private key
    pub encrypted_private_key: String,
    /// Base64-encoded salt for PBKDF2
    pub salt: String,
    /// Base64-encoded nonce for AES-GCM
    pub nonce: String,
}

/// In-memory wallet with sensitive data
#[derive(Debug, Clone, ZeroizeOnDrop)]
pub struct Wallet {
    pub name: String,
    pub address: String,
    pub public_key: String,
    #[zeroize(skip)]
    pub created_at: String,
    /// Private key - zeroized on drop for security
    private_key: Option<SecretKey>,
}

/// Wrapper for secret key data that zeroizes on drop
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
struct SecretKey {
    data: String,
}

impl std::fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[REDACTED]")
    }
}

impl Wallet {
    /// Generate a new wallet with BIP39 mnemonic
    pub fn generate(name: String) -> Result<(Self, String)> {
        // Generate 128 bits (16 bytes) of entropy for 12-word mnemonic
        let mut entropy = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut entropy);

        let mnemonic = Mnemonic::from_entropy_in(Language::English, &entropy)
            .map_err(|e| anyhow!("Failed to generate mnemonic: {}", e))?;
        let seed = mnemonic.to_seed("");

        // Use first 32 bytes as Ed25519 private key
        let private_key_bytes: [u8; SECRET_KEY_LENGTH] = seed[..SECRET_KEY_LENGTH]
            .try_into()
            .map_err(|_| anyhow!("Failed to generate private key"))?;

        let signing_key = SigningKey::from_bytes(&private_key_bytes);
        let verifying_key = signing_key.verifying_key();

        let address = Self::encode_address(&verifying_key)?;
        let public_key = hex::encode(verifying_key.to_bytes());
        let private_key = hex::encode(signing_key.to_keypair_bytes());

        let wallet = Wallet {
            name,
            address,
            public_key,
            private_key: Some(SecretKey { data: private_key }),
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        Ok((wallet, mnemonic.to_string()))
    }

    /// Import wallet from mnemonic phrase
    pub fn from_mnemonic(mnemonic_phrase: &str, name: String) -> Result<Self> {
        let mnemonic = Mnemonic::parse_in_normalized(Language::English, mnemonic_phrase)
            .map_err(|e| anyhow!("Invalid mnemonic: {}", e))?;

        let seed = mnemonic.to_seed("");
        let private_key_bytes: [u8; SECRET_KEY_LENGTH] = seed[..SECRET_KEY_LENGTH]
            .try_into()
            .map_err(|_| anyhow!("Failed to generate private key"))?;

        let signing_key = SigningKey::from_bytes(&private_key_bytes);
        let verifying_key = signing_key.verifying_key();

        let address = Self::encode_address(&verifying_key)?;
        let public_key = hex::encode(verifying_key.to_bytes());
        let private_key = hex::encode(signing_key.to_keypair_bytes());

        Ok(Wallet {
            name,
            address,
            public_key,
            private_key: Some(SecretKey { data: private_key }),
            created_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Import wallet from private key hex
    pub fn from_private_key(private_key_hex: &str, name: String) -> Result<Self> {
        let key_bytes = hex::decode(private_key_hex)
            .map_err(|_| anyhow!("Invalid hex private key"))?;

        if key_bytes.len() != 64 {
            return Err(anyhow!("Private key must be 64 bytes (keypair)"));
        }

        let signing_key = SigningKey::from_keypair_bytes(&key_bytes.try_into().unwrap())
            .map_err(|e| anyhow!("Invalid private key: {}", e))?;

        let verifying_key = signing_key.verifying_key();

        let address = Self::encode_address(&verifying_key)?;
        let public_key = hex::encode(verifying_key.to_bytes());

        Ok(Wallet {
            name,
            address,
            public_key,
            private_key: Some(SecretKey { data: private_key_hex.to_string() }),
            created_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Encode public key to Bech32 address with "ouro" prefix
    fn encode_address(verifying_key: &VerifyingKey) -> Result<String> {
        let pubkey_bytes = verifying_key.to_bytes();

        // Try Bech32 encoding
        let hrp = Hrp::parse("ouro").map_err(|e| anyhow!("Invalid HRP: {}", e))?;

        match bech32::encode::<Bech32>(hrp, &pubkey_bytes) {
            Ok(addr) => Ok(addr),
            Err(_) => {
                // Fallback: ouro1 + first 20 bytes of pubkey in hex
                let short_addr = hex::encode(&pubkey_bytes[..20]);
                Ok(format!("ouro1{}", short_addr))
            }
        }
    }

    /// Get signing key from private key
    pub fn get_signing_key(&self) -> Result<SigningKey> {
        let private_key = self.private_key
            .as_ref()
            .ok_or_else(|| anyhow!("No private key available"))?;

        let key_bytes = hex::decode(&private_key.data)
            .map_err(|_| anyhow!("Invalid hex private key"))?;

        if key_bytes.len() != 64 {
            return Err(anyhow!("Private key must be 64 bytes"));
        }

        SigningKey::from_keypair_bytes(&key_bytes.try_into().unwrap())
            .map_err(|e| anyhow!("Invalid signing key: {}", e))
    }

    /// Get the private key hex (for export purposes only)
    pub fn get_private_key_hex(&self) -> Result<&str> {
        self.private_key
            .as_ref()
            .map(|k| k.data.as_str())
            .ok_or_else(|| anyhow!("No private key available"))
    }

    /// Derive encryption key from password using PBKDF2
    fn derive_key(password: &str, salt: &[u8]) -> [u8; 32] {
        let mut key = [0u8; 32];
        pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, PBKDF2_ITERATIONS, &mut key);
        key
    }

    /// Encrypt private key with AES-256-GCM
    fn encrypt_private_key(private_key: &str, password: &str) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>)> {
        // Generate random salt and nonce
        let mut salt = [0u8; SALT_LENGTH];
        let mut nonce_bytes = [0u8; NONCE_LENGTH];
        rand::thread_rng().fill_bytes(&mut salt);
        rand::thread_rng().fill_bytes(&mut nonce_bytes);

        // Derive key from password
        let key = Self::derive_key(password, &salt);

        // Encrypt
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|e| anyhow!("Failed to create cipher: {}", e))?;
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, private_key.as_bytes())
            .map_err(|e| anyhow!("Encryption failed: {}", e))?;

        Ok((ciphertext, salt.to_vec(), nonce_bytes.to_vec()))
    }

    /// Decrypt private key with AES-256-GCM
    fn decrypt_private_key(ciphertext: &[u8], password: &str, salt: &[u8], nonce: &[u8]) -> Result<String> {
        // Derive key from password
        let key = Self::derive_key(password, salt);

        // Decrypt
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|e| anyhow!("Failed to create cipher: {}", e))?;
        let nonce = Nonce::from_slice(nonce);

        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| anyhow!("Decryption failed - wrong password or corrupted data"))?;

        String::from_utf8(plaintext)
            .map_err(|_| anyhow!("Invalid UTF-8 in decrypted data"))
    }

    /// Save wallet to encrypted file
    ///
    /// SECURITY: Private key is encrypted with AES-256-GCM using a password-derived key
    pub fn save_encrypted(&self, password: &str) -> Result<()> {
        if password.len() < 8 {
            return Err(anyhow!("Password must be at least 8 characters"));
        }

        let private_key = self.private_key
            .as_ref()
            .ok_or_else(|| anyhow!("No private key to save"))?;

        let (ciphertext, salt, nonce) = Self::encrypt_private_key(&private_key.data, password)?;

        let encrypted_file = EncryptedWalletFile {
            version: 1,
            name: self.name.clone(),
            address: self.address.clone(),
            public_key: self.public_key.clone(),
            created_at: self.created_at.clone(),
            encrypted_private_key: BASE64.encode(&ciphertext),
            salt: BASE64.encode(&salt),
            nonce: BASE64.encode(&nonce),
        };

        let wallet_path = Self::get_wallet_path()?;
        let json = serde_json::to_string_pretty(&encrypted_file)?;
        fs::write(&wallet_path, json)?;

        // Set restrictive file permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            fs::set_permissions(&wallet_path, perms)?;
        }

        Ok(())
    }

    /// Load wallet from encrypted file
    pub fn load_encrypted(password: &str) -> Result<Self> {
        let wallet_path = Self::get_wallet_path()?;
        if !wallet_path.exists() {
            return Err(anyhow!("No wallet found. Create one with 'midgard-wallet create'"));
        }

        let json = fs::read_to_string(&wallet_path)?;
        let encrypted_file: EncryptedWalletFile = serde_json::from_str(&json)?;

        // Check version
        if encrypted_file.version != 1 {
            return Err(anyhow!("Unsupported wallet file version: {}", encrypted_file.version));
        }

        // Decode Base64 fields
        let ciphertext = BASE64.decode(&encrypted_file.encrypted_private_key)
            .map_err(|_| anyhow!("Invalid encrypted data"))?;
        let salt = BASE64.decode(&encrypted_file.salt)
            .map_err(|_| anyhow!("Invalid salt"))?;
        let nonce = BASE64.decode(&encrypted_file.nonce)
            .map_err(|_| anyhow!("Invalid nonce"))?;

        // Decrypt private key
        let private_key = Self::decrypt_private_key(&ciphertext, password, &salt, &nonce)?;

        Ok(Wallet {
            name: encrypted_file.name,
            address: encrypted_file.address,
            public_key: encrypted_file.public_key,
            created_at: encrypted_file.created_at,
            private_key: Some(SecretKey { data: private_key }),
        })
    }

    /// Save wallet (calls save_encrypted with password prompt)
    pub fn save(&self) -> Result<()> {
        let password = rpassword::prompt_password("Enter wallet password: ")
            .map_err(|e| anyhow!("Failed to read password: {}", e))?;
        self.save_encrypted(&password)
    }

    /// Load wallet (calls load_encrypted with password prompt)
    pub fn load() -> Result<Self> {
        let wallet_path = Self::get_wallet_path()?;
        if !wallet_path.exists() {
            return Err(anyhow!("No wallet found. Create one with 'midgard-wallet create'"));
        }

        // Check if file is old unencrypted format
        let json = fs::read_to_string(&wallet_path)?;
        if json.contains("\"private_key\":") && !json.contains("\"encrypted_private_key\":") {
            return Err(anyhow!(
                "Found legacy unencrypted wallet. Please backup your private key and create a new encrypted wallet."
            ));
        }

        let password = rpassword::prompt_password("Enter wallet password: ")
            .map_err(|e| anyhow!("Failed to read password: {}", e))?;
        Self::load_encrypted(&password)
    }

    /// Get wallet file path
    fn get_wallet_path() -> Result<PathBuf> {
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow!("Could not find home directory"))?;
        Ok(home.join(WALLET_FILE))
    }

    /// Check if wallet exists
    pub fn exists() -> bool {
        Self::get_wallet_path()
            .map(|p| p.exists())
            .unwrap_or(false)
    }

    /// Get wallet info without private key (for display)
    pub fn get_public_info(&self) -> WalletInfo {
        WalletInfo {
            name: self.name.clone(),
            address: self.address.clone(),
            public_key: self.public_key.clone(),
            created_at: self.created_at.clone(),
        }
    }
}

/// Public wallet information (safe to display)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletInfo {
    pub name: String,
    pub address: String,
    pub public_key: String,
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wallet_encryption_roundtrip() {
        let (wallet, _mnemonic) = Wallet::generate("test_wallet".to_string()).unwrap();
        let password = "test_password_123";

        // Encrypt
        let (ciphertext, salt, nonce) = Wallet::encrypt_private_key(
            wallet.get_private_key_hex().unwrap(),
            password,
        ).unwrap();

        // Decrypt
        let decrypted = Wallet::decrypt_private_key(&ciphertext, password, &salt, &nonce).unwrap();

        assert_eq!(decrypted, wallet.get_private_key_hex().unwrap());
    }

    #[test]
    fn test_wrong_password_fails() {
        let (wallet, _mnemonic) = Wallet::generate("test_wallet".to_string()).unwrap();

        let (ciphertext, salt, nonce) = Wallet::encrypt_private_key(
            wallet.get_private_key_hex().unwrap(),
            "correct_password",
        ).unwrap();

        let result = Wallet::decrypt_private_key(&ciphertext, "wrong_password", &salt, &nonce);
        assert!(result.is_err());
    }
}
