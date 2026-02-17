// src/config_manager.rs
// Unified configuration manager for Ouroboros Node
// Inspired by Nexus CLI model

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use once_cell::sync::Lazy;
use anyhow::{Result, Context};
use uuid::Uuid;
use chrono::Utc;

pub static CONFIG: Lazy<Arc<RwLock<NodeConfig>>> = Lazy::new(|| {
    Arc::new(RwLock::new(NodeConfig::default()))
});

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum NodeRole {
    Heavy,  // Global Validator / Settlement (Rust)
    Medium, // Subchain Aggregator / Shadow Council (Python/Rust)
    Light,  // Microchain / App Node (Python)
}

impl Default for NodeRole {
    fn default() -> Self {
        NodeRole::Heavy
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NodeConfig {
    #[serde(default)]
    pub role: NodeRole,
    pub network: NetworkConfig,
    pub identity: IdentityConfig,
    pub security: SecurityConfig,
    pub storage: StorageConfig,
    pub adaptive_difficulty: AdaptiveDifficultyConfig,
    pub updates: UpdateConfig,
    pub wallet: Option<WalletLinkConfig>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NetworkConfig {
    pub api_addr: String,
    pub listen_addr: String,
    pub peer_addrs: Vec<String>,
    pub bootstrap_peers: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct IdentityConfig {
    pub node_id: String,
    pub node_number: u64,
    pub public_name: Option<String>,
    pub first_joined: String,
    pub total_uptime_secs: u64,
    pub last_started: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SecurityConfig {
    pub bft_secret_seed: String,
    pub node_keypair_hex: String,
    pub api_keys: Vec<String>,
    pub authorized_peers: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StorageConfig {
    pub db_path: String,
    pub storage_mode: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AdaptiveDifficultyConfig {
    pub current: String,
    pub min_difficulty: Option<String>,
    pub max_difficulty: Option<String>,
    pub last_performance_ms: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UpdateConfig {
    pub auto_update_enabled: bool,
    pub check_interval_hours: u64,
    pub channel: String,
    pub last_check: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WalletLinkConfig {
    pub wallet_address: String,
    pub linked_at: String,
    pub wallet_signature: String,
    pub node_signature: String,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            role: NodeRole::Heavy,
            network: NetworkConfig {
                api_addr: "0.0.0.0:8000".to_string(),
                listen_addr: "0.0.0.0:9000".to_string(),
                peer_addrs: vec![],
                bootstrap_peers: vec![],
            },
            identity: IdentityConfig {
                node_id: Uuid::new_v4().to_string(),
                node_number: 0,
                public_name: None,
                first_joined: Utc::now().to_rfc3339(),
                total_uptime_secs: 0,
                last_started: None,
            },
            security: SecurityConfig {
                bft_secret_seed: "".to_string(),
                node_keypair_hex: "".to_string(),
                api_keys: vec![],
                authorized_peers: vec![],
            },
            storage: StorageConfig {
                db_path: "sled_data".to_string(),
                storage_mode: "rocksdb".to_string(),
            },
            adaptive_difficulty: AdaptiveDifficultyConfig {
                current: "small".to_string(),
                min_difficulty: None,
                max_difficulty: None,
                last_performance_ms: 0,
            },
            updates: UpdateConfig {
                auto_update_enabled: false,
                check_interval_hours: 24,
                channel: "stable".to_string(),
                last_check: None,
            },
            wallet: None,
        }
    }
}

impl NodeConfig {
    pub fn load() -> Result<Self> {
        let config_path = get_config_path();
        
        if config_path.exists() {
            let json = fs::read_to_string(&config_path)?;
            let config: NodeConfig = serde_json::from_str(&json)?;
            Ok(config)
        } else {
            // Attempt migration from old files
            let config = Self::migrate_from_legacy()?;
            config.save()?;
            Ok(config)
        }
    }

    pub fn save(&self) -> Result<()> {
        let config_path = get_config_path();
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        fs::write(&config_path, json)?;
        // Config contains secrets (BFT seed, API keys) — restrict to owner-only
        crate::crypto::set_restrictive_permissions(&config_path);
        Ok(())
    }

    fn migrate_from_legacy() -> Result<Self> {
        let mut config = Self::default();
        
        // 1. Migrate from .env (basic settings)
        dotenvy::dotenv().ok();
        if let Ok(role) = std::env::var("NODE_ROLE") {
            config.role = match role.to_lowercase().as_str() {
                "medium" => NodeRole::Medium,
                "light" => NodeRole::Light,
                _ => NodeRole::Heavy,
            };
        }
        if let Ok(addr) = std::env::var("API_ADDR") { config.network.api_addr = addr; }
        if let Ok(addr) = std::env::var("LISTEN_ADDR") { config.network.listen_addr = addr; }
        if let Ok(peers) = std::env::var("PEER_ADDRS") {
            config.network.peer_addrs = peers.split(',').map(|s| s.trim().to_string()).collect();
        }
        if let Ok(seed) = std::env::var("BFT_SECRET_SEED") { config.security.bft_secret_seed = seed; }
        if let Ok(keys) = std::env::var("API_KEYS") {
            config.security.api_keys = keys.split(',').map(|s| s.trim().to_string()).collect();
        }
        if let Ok(path) = std::env::var("ROCKSDB_PATH") { config.storage.db_path = path; }

        // 2. Migrate identity
        let db_path = &config.storage.db_path;
        let identity_path = Path::new(db_path).join(".node_identity.json");
        if identity_path.exists() {
            if let Ok(json) = fs::read_to_string(identity_path) {
                if let Ok(id) = serde_json::from_str::<serde_json::Value>(&json) {
                    if let Some(id_str) = id["node_id"].as_str() { config.identity.node_id = id_str.to_string(); }
                    if let Some(num) = id["node_number"].as_u64() { config.identity.node_number = num; }
                    if let Some(joined) = id["first_joined"].as_str() { config.identity.first_joined = joined.to_string(); }
                    if let Some(uptime) = id["total_uptime_secs"].as_u64() { config.identity.total_uptime_secs = uptime; }
                }
            }
        }

        // 3. Migrate wallet link
        let wallet_path = Path::new(db_path).join(".wallet_link.json");
        if wallet_path.exists() {
            if let Ok(json) = fs::read_to_string(wallet_path) {
                if let Ok(w) = serde_json::from_str::<serde_json::Value>(&json) {
                    config.wallet = Some(WalletLinkConfig {
                        wallet_address: w["wallet_address"].as_str().unwrap_or_default().to_string(),
                        linked_at: w["linked_at"].as_str().unwrap_or_default().to_string(),
                        wallet_signature: w["wallet_signature"].as_str().unwrap_or_default().to_string(),
                        node_signature: w["node_signature"].as_str().unwrap_or_default().to_string(),
                    });
                }
            }
        }

        Ok(config)
    }
}

pub fn get_config_path() -> PathBuf {
    let home = if cfg!(windows) {
        std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\Users\\Default".to_string())
    } else {
        std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string())
    };
    PathBuf::from(home).join(".ouroboros").join("config.json")
}

pub async fn init_config() -> Result<()> {
    let loaded = NodeConfig::load()?;
    let mut config = CONFIG.write().await;
    *config = loaded;
    Ok(())
}

pub async fn get_node_id() -> String {
    CONFIG.read().await.identity.node_id.clone()
}
