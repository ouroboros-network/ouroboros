// L3 fix: Removed blanket #![allow(dead_code/unused_imports/unused_variables)]
// Use targeted #[allow(...)] on specific items if needed during development.

// RocksDB type alias for database operations
// Replaces PostgreSQL PgPool throughout the codebase
pub type PgPool = std::sync::Arc<storage::RocksDb>;

pub mod account_abstraction;
pub mod alerts;
pub mod api;
pub mod batch_writer;
pub mod bft;
pub mod bridge;
pub mod config;
pub mod config_manager;
pub mod crypto;
pub mod dag;
pub mod fee_market;
pub mod indexer;
pub mod keys;
pub mod mempool;
pub mod merkle;
pub mod mev_protection;
pub mod microchain;
pub mod network;
pub mod oracle;
pub mod oracle_fetchers;
pub mod oracle_node;
pub mod oracle_subchain;
pub mod rewards;
pub mod ring_signatures;
pub mod simple_metrics;
pub mod stealth_addresses;
pub mod tail_emission;
pub mod vrf;
pub mod zk_proofs;
pub mod reconciliation;
pub mod storage;
pub mod anchor_service;
pub mod controller;
pub mod mainchain;
pub mod ouro_coin;
pub mod subchain;
pub mod token_bucket;
pub mod cli_dashboard;
pub mod multisig;
pub mod node_identity;
pub mod tor;
pub mod validator_registration;
pub mod vm;
pub mod cross_chain;
pub mod data_availability;
pub mod fraud_detection;
pub mod governance;
pub mod native_contracts;

use crate::reconciliation::finalize_block;

use crate::crypto::verify_ed25519_hex;

use crate::storage::{batch_put, open_db, put, RocksDb};
use crate::crypto::pq::DilithiumKeypair;
use axum::{
    routing::{delete, get, post},
    Router,
};
use bft::consensus::{BFTNode, HotStuff, HotStuffConfig};
use bft::state::BFTState;
use bft::validator_registry::ValidatorRegistry;
use chrono::Utc;
use clap::{Parser, Subcommand};
use dag::dag::DAG;
use dag::transaction::Transaction;
use dotenvy;
use hex;
use mempool::Mempool;
use network::bft_msg::{start_bft_server, BroadcastHandle};
use network::{start_network, TxBroadcast};
use rustls_pemfile::{certs, pkcs8_private_keys};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use std::error::Error;
use std::fs;
use std::io::BufReader;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio_rustls::rustls;
use tokio_rustls::rustls::{Certificate, PrivateKey};
use uuid::Uuid;

#[derive(Deserialize)]
pub struct IncomingFileTxn {
    sender: String,
    recipient: String,
    amount: u64,
    public_key: String,
    signature: String,
}

/// Lightweight verification stub kept for optional fallback (length checks).
/// Key validation result
pub struct KeyStatus {
    pub warnings: Vec<String>,
}

/// Check required keys on startup
pub fn check_required_keys() -> anyhow::Result<KeyStatus> {
    let mut warnings = Vec::new();
    let mut errors = Vec::new();

    // 1. BFT_SECRET_SEED - REQUIRED for validators
    match std::env::var("BFT_SECRET_SEED") {
        Ok(seed_hex) => match hex::decode(&seed_hex) {
            Ok(bytes) => {
                if bytes.len() != 32 {
                    errors.push(format!(
                        "BFT_SECRET_SEED must be 64 hex characters (32 bytes), got {} bytes",
                        bytes.len()
                    ));
                }
                if bytes.iter().all(|&b| b == 0) {
                    errors.push(
                            "BFT_SECRET_SEED is all zeros - NOT SECURE! Generate with: openssl rand -hex 32".to_string()
                        );
                }
                // M3 fix: Detect common weak patterns (all-same-byte, sequential)
                if bytes.len() == 32 && bytes.iter().all(|&b| b == bytes[0]) {
                    errors.push(
                        "BFT_SECRET_SEED uses a repeating byte pattern - NOT SECURE! Generate with: openssl rand -hex 32".to_string()
                    );
                }
            }
            Err(_) => {
                errors.push("BFT_SECRET_SEED is not valid hex".to_string());
            }
        },
        Err(_) => {
            errors.push(
                "BFT_SECRET_SEED not set - REQUIRED for production. Generate with: openssl rand -hex 32".to_string()
            );
        }
    }

    // 2. TLS Certificate - warn if missing (optional but recommended)
    if std::env::var("TLS_CERT_PATH").is_err() {
        warnings.push(
            "TLS_CERT_PATH not set - running HTTP only (insecure for production)".to_string(),
        );
        warnings.push(
            "Generate self-signed cert with: openssl req -x509 -newkey rsa:4096 -keyout key.pem -out cert.pem -days 365 -nodes".to_string()
        );
    }

    // 3. ANCHOR_PRIVATE_KEY - info if missing (optional feature)
    if std::env::var("ANCHOR_PRIVATE_KEY").is_err() {
        println!("INFO: ANCHOR_PRIVATE_KEY not set - anchor operations disabled");
    }

    // 4. NODE_WALLET_ADDRESS - warn if missing (needed for rewards)
    if std::env::var("NODE_WALLET_ADDRESS").is_err() {
        warnings
            .push("NODE_WALLET_ADDRESS not set - node will not receive block rewards".to_string());
    }

    // If there are critical errors, fail startup
    if !errors.is_empty() {
        eprintln!("\n=== CRITICAL: Missing Required Keys ===");
        for error in &errors {
            eprintln!("  [ERROR] {}", error);
        }
        eprintln!("===================================\n");
        return Err(anyhow::anyhow!(
            "Missing required keys: {}",
            errors.join("; ")
        ));
    }

    Ok(KeyStatus { warnings })
}

// M5 fix: verify_signature_stub removed — it only checked lengths, not cryptographic validity.
// Use crate::crypto::keys::verify_bytes() for real signature verification.

/// Handle file-based transaction submission (dag_txn.json)
pub async fn handle_incoming_file(
    path: &Path,
    _dag: &mut DAG,
    mempool: &Arc<Mempool>,
    bcast: &TxBroadcast,
) {
    if !path.exists() {
        return;
    }
    let data = match tokio::fs::read_to_string(path).await {
        Ok(d) => d,
        Err(e) => {
            println!("read file error: {}", e);
            return;
        }
    };
    let parsed: IncomingFileTxn = match serde_json::from_str(&data) {
        Ok(p) => p,
        Err(e) => {
            println!("parse file txn error: {}", e);
            return;
        }
    };

    let message = format!("{}:{}:{}", parsed.sender, parsed.recipient, parsed.amount);

    // Strict verification — require real ed25519 verification (no fallback)
    let verified = verify_ed25519_hex(&parsed.public_key, &parsed.signature, message.as_bytes());
    if !verified {
        println!(" Signature validation failed. Transaction rejected.");
        return;
    }

    let txn = Transaction {
        id: Uuid::new_v4(),
        sender: parsed.sender.clone(),
        recipient: parsed.recipient.clone(),
        amount: parsed.amount,
        timestamp: Utc::now(),
        parents: vec![],
        signature: parsed.signature.clone(),
        public_key: parsed.public_key.clone(),
        fee: 0, // Default fee (can be extended to read from file)
        payload: None,
        chain_id: "ouroboros-mainnet-1".to_string(), // Phase 6: replay protection
        nonce: 0, // Phase 6: transaction ordering (should be queried from sender's last nonce)
    };

    if let Err(e) = mempool.add_tx(&txn) {
        println!("mempool add err: {}", e);
    } else {
        // broadcast and remove file
        let _ = bcast.send(txn.clone()).await;
        let _ = fs::remove_file(path);
        println!(" Verified & added transaction.");
    }
}

/// Load TLS configuration from environment variables.
/// Returns None if TLS is not configured (allows fallback to HTTP).
///
/// Environment variables:
/// - `TLS_CERT_PATH`: Path to TLS certificate file (PEM format)
/// - `TLS_KEY_PATH`: Path to TLS private key file (PKCS8 PEM format)
pub fn load_tls_config() -> Option<axum_server::tls_rustls::RustlsConfig> {
    let cert_path = match std::env::var("TLS_CERT_PATH") {
        Ok(path) if !path.is_empty() => path,
        _ => {
            println!("  TLS_CERT_PATH not set - running without TLS (HTTP only)");
            return None;
        }
    };

    let key_path = match std::env::var("TLS_KEY_PATH") {
        Ok(path) if !path.is_empty() => path,
        _ => {
            println!("  TLS_CERT_PATH set but TLS_KEY_PATH missing - running without TLS");
            return None;
        }
    };

    // Load certificate
    let cert_file = match std::fs::File::open(&cert_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!(
                "  Failed to open TLS cert '{}': {} - running without TLS",
                cert_path, e
            );
            return None;
        }
    };
    let mut cert_reader = BufReader::new(cert_file);
    let cert_chain = match certs(&mut cert_reader) {
        Ok(certs) => certs.into_iter().map(|c| Certificate(c)).collect(),
        Err(e) => {
            eprintln!(
                "  Failed to parse TLS cert '{}': {} - running without TLS",
                cert_path, e
            );
            return None;
        }
    };

    // Load private key
    let key_file = match std::fs::File::open(&key_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!(
                "  Failed to open TLS key '{}': {} - running without TLS",
                key_path, e
            );
            return None;
        }
    };
    let mut key_reader = BufReader::new(key_file);
    let mut keys = match pkcs8_private_keys(&mut key_reader) {
        Ok(keys) => keys,
        Err(e) => {
            eprintln!(
                "  Failed to parse TLS key '{}': {} - running without TLS",
                key_path, e
            );
            return None;
        }
    };

    if keys.is_empty() {
        eprintln!(
            "  No private keys found in '{}' - running without TLS",
            key_path
        );
        return None;
    }

    let key = keys.remove(0);

    // Build rustls config
    let mut server_config = match rustls::ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_single_cert(cert_chain, PrivateKey(key))
    {
        Ok(config) => config,
        Err(e) => {
            eprintln!("  Failed to build TLS config: {} - running without TLS", e);
            return None;
        }
    };

    server_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    let rustls_config = axum_server::tls_rustls::RustlsConfig::from_config(Arc::new(server_config));

    println!(" TLS enabled: cert='{}', key='{}'", cert_path, key_path);
    Some(rustls_config)
}

/// Load TLS configuration for P2P network connections.
/// Returns None if TLS is not configured (allows fallback to plain TCP).
///
/// Environment variables:
/// - `P2P_TLS_CERT_PATH`: Path to P2P TLS certificate file (PEM format)
/// - `P2P_TLS_KEY_PATH`: Path to P2P TLS private key file (PKCS8 PEM format)
///
/// If not set, falls back to using the same certs as the API (TLS_CERT_PATH/TLS_KEY_PATH)
pub fn load_p2p_tls_config() -> Option<Arc<rustls::ServerConfig>> {
    // Try P2P-specific certs first, then fall back to API certs
    let cert_path = std::env::var("P2P_TLS_CERT_PATH")
        .or_else(|_| std::env::var("TLS_CERT_PATH"))
        .ok()
        .filter(|p| !p.is_empty());

    let key_path = std::env::var("P2P_TLS_KEY_PATH")
        .or_else(|_| std::env::var("TLS_KEY_PATH"))
        .ok()
        .filter(|p| !p.is_empty());

    let (cert_path, key_path) = match (cert_path, key_path) {
        (Some(c), Some(k)) => (c, k),
        _ => {
            println!("  P2P TLS not configured - using plain TCP for peer connections");
            return None;
        }
    };

    // Load certificate
    let cert_file = match std::fs::File::open(&cert_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!(
                "  Failed to open P2P TLS cert '{}': {} - using plain TCP",
                cert_path, e
            );
            return None;
        }
    };
    let mut cert_reader = BufReader::new(cert_file);
    let cert_chain = match certs(&mut cert_reader) {
        Ok(certs) => certs.into_iter().map(|c| Certificate(c)).collect(),
        Err(e) => {
            eprintln!(
                "  Failed to parse P2P TLS cert '{}': {} - using plain TCP",
                cert_path, e
            );
            return None;
        }
    };

    // Load private key
    let key_file = match std::fs::File::open(&key_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!(
                "  Failed to open P2P TLS key '{}': {} - using plain TCP",
                key_path, e
            );
            return None;
        }
    };
    let mut key_reader = BufReader::new(key_file);
    let mut keys = match pkcs8_private_keys(&mut key_reader) {
        Ok(keys) => keys,
        Err(e) => {
            eprintln!(
                "  Failed to parse P2P TLS key '{}': {} - using plain TCP",
                key_path, e
            );
            return None;
        }
    };

    if keys.is_empty() {
        eprintln!(
            "  No private keys found in '{}' - using plain TCP",
            key_path
        );
        return None;
    }

    let key = keys.remove(0);

    // Build rustls config for P2P (no client auth required by default)
    let server_config = match rustls::ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_single_cert(cert_chain, PrivateKey(key))
    {
        Ok(config) => config,
        Err(e) => {
            eprintln!("  Failed to build P2P TLS config: {} - using plain TCP", e);
            return None;
        }
    };

    println!(" P2P TLS enabled: cert='{}', key='{}'", cert_path, key_path);
    Some(Arc::new(server_config))
}

/// Load TLS client configuration for outbound P2P connections.
/// Returns None if TLS is not configured (allows fallback to plain TCP).
///
/// Environment variables:
/// - `P2P_TLS_CA_PATH`: Path to CA certificate file for verifying peers (optional)
/// - `P2P_TLS_SKIP_VERIFY`: Set to "true" to skip certificate verification (INSECURE - testing only)
pub fn load_p2p_client_tls_config() -> Option<Arc<rustls::ClientConfig>> {
    // Check if TLS is configured at all
    let has_cert = std::env::var("P2P_TLS_CERT_PATH")
        .or_else(|_| std::env::var("TLS_CERT_PATH"))
        .ok()
        .filter(|p| !p.is_empty())
        .is_some();

    if !has_cert {
        // Server TLS not configured, don't use TLS for client either
        return None;
    }

    let skip_verify = std::env::var("P2P_TLS_SKIP_VERIFY")
        .map(|v| v.to_lowercase() == "true")
        .unwrap_or(false);

    if skip_verify {
        // INSECURE: Skip certificate verification (for testing only)
        println!("  WARNING: P2P TLS certificate verification disabled (INSECURE)");

        // Create a config that accepts any certificate
        let root_store = rustls::RootCertStore::empty();

        // Use a custom verifier that accepts all certs
        let config = rustls::ClientConfig::builder()
            .with_safe_defaults()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        // Note: This still validates but has no trusted roots, so it will fail on real certs
        // For truly insecure testing, users should use the dangerous_configuration feature
        return Some(Arc::new(config));
    }

    // Try to load CA certificates
    let ca_path = std::env::var("P2P_TLS_CA_PATH")
        .ok()
        .filter(|p| !p.is_empty());

    let mut root_store = rustls::RootCertStore::empty();

    if let Some(ca_path) = ca_path {
        // Load custom CA
        let ca_file = match std::fs::File::open(&ca_path) {
            Ok(f) => f,
            Err(e) => {
                eprintln!(
                    "  Failed to open P2P TLS CA '{}': {} - using plain TCP for outbound",
                    ca_path, e
                );
                return None;
            }
        };
        let mut ca_reader = BufReader::new(ca_file);
        match certs(&mut ca_reader) {
            Ok(ca_certs) => {
                for cert in ca_certs {
                    if let Err(e) = root_store.add(&Certificate(cert)) {
                        eprintln!("  Warning: Failed to add CA cert: {}", e);
                    }
                }
            }
            Err(e) => {
                eprintln!(
                    "  Failed to parse P2P TLS CA '{}': {} - using plain TCP for outbound",
                    ca_path, e
                );
                return None;
            }
        }
        println!(" P2P TLS client enabled with custom CA: '{}'", ca_path);
    } else {
        // Use webpki roots for public certificates
        root_store.add_trust_anchors(webpki_roots::TLS_SERVER_ROOTS.iter().map(|ta| {
            rustls::OwnedTrustAnchor::from_subject_spki_name_constraints(
                ta.subject.as_ref(),
                ta.spki.as_ref(),
                ta.name_constraints
                    .as_ref()
                    .map(|nc| -> &[u8] { nc.as_ref() }),
            )
        }));
        println!(" P2P TLS client enabled with system roots");
    }

    let config = rustls::ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    Some(Arc::new(config))
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Starts the ouroboros node
    Start {
        /// Run in headless mode (no interactive dashboard)
        #[arg(long)]
        headless: bool,
        /// Node role (heavy, medium, light). 
        /// Use 'ouro roles' to see requirements and rewards for each role.
        #[arg(long, default_value = "heavy")]
        role: String,
        /// Minimum difficulty (override)
        #[arg(long)]
        min_difficulty: Option<String>,
        /// Maximum difficulty (override)
        #[arg(long)]
        max_difficulty: Option<String>,
    },
    /// Joins an existing network
    Join {
        #[arg(long)]
        peer: Option<String>,
        #[arg(long)]
        bootstrap_url: Option<String>,
        #[arg(long, default_value_t = 8000)]
        api_port: u16,
        #[arg(long, default_value_t = 9000)]
        p2p_port: u16,
        #[arg(long, default_value = "rocksdb")]
        storage: String,
        #[arg(long)]
        rocksdb_path: Option<String>,
        /// Node role (heavy, medium, light). 
        /// Use 'ouro roles' to see requirements and rewards for each role.
        #[arg(long, default_value = "heavy")]
        role: String,
        /// Run in headless mode (no interactive dashboard)
        #[arg(long)]
        headless: bool,
        /// Minimum difficulty (override)
        #[arg(long)]
        min_difficulty: Option<String>,
        /// Maximum difficulty (override)
        #[arg(long)]
        max_difficulty: Option<String>,
    },
    /// Run hardware benchmark to set optimal difficulty
    Benchmark {
        /// Number of cycles to run
        #[arg(long, default_value_t = 10)]
        cycles: u32,
    },
    /// Show node status dashboard (live-updating by default)
    Status {
        /// Print once and exit (instead of live updating)
        #[arg(long)]
        once: bool,
        /// API endpoint to query (default: http://localhost:8000)
        #[arg(long, default_value = "http://localhost:8000")]
        api: String,
    },
    /// List connected peers
    Peers {
        /// API endpoint to query
        #[arg(long, default_value = "http://localhost:8000")]
        api: String,
    },
    /// Show consensus status
    Consensus {
        /// API endpoint to query
        #[arg(long, default_value = "http://localhost:8000")]
        api: String,
    },
    /// Show mempool status
    Mempool {
        /// API endpoint to query
        #[arg(long, default_value = "http://localhost:8000")]
        api: String,
    },
    /// Show resource usage
    Resources {
        /// API endpoint to query
        #[arg(long, default_value = "http://localhost:8000")]
        api: String,
    },
    /// Tail node logs
    Logs {
        /// Number of lines to show
        #[arg(long, short, default_value_t = 50)]
        lines: u32,
        /// Follow log output (like tail -f)
        #[arg(long, short)]
        follow: bool,
        /// Export logs to file
        #[arg(long)]
        export: Option<String>,
    },
    /// Stop the running node
    Stop {
        /// API endpoint to send stop command
        #[arg(long, default_value = "http://localhost:8000")]
        api: String,
    },
    /// Restart the running node
    Restart {
        /// API endpoint to send restart command
        #[arg(long, default_value = "http://localhost:8000")]
        api: String,
    },
    /// Run diagnostic checks
    Diagnose {
        /// Export diagnostic report to file
        #[arg(long)]
        export: Option<String>,
    },
    /// Wallet management
    Wallet {
        #[command(subcommand)]
        command: WalletCommands,
    },
    /// Account management (keys, balance)
    Account {
        #[command(subcommand)]
        command: AccountCommands,
    },
    /// Transaction management
    Tx {
        #[command(subcommand)]
        command: TxCommands,
    },
    /// Resync node from network
    Resync {
        /// API endpoint
        #[arg(long, default_value = "http://localhost:8000")]
        api: String,
    },
    /// Backup database
    Backup {
        /// Output path for backup
        #[arg(long)]
        output: Option<String>,
    },
    /// Register a user wallet (Nexus style)
    RegisterUser {
        /// Wallet address to link
        #[arg(long)]
        wallet_address: String,
    },
    /// Register a node ID (Nexus style)
    RegisterNode {
        /// Custom node ID
        #[arg(long)]
        node_id: Option<String>,
    },
    /// Clear all credentials and logout
    Logout {},
    /// Show information about node roles (Heavy, Medium, Light)
    Roles {},
    /// Run as oracle node
    Oracle {
        #[arg(long)]
        peer: Option<String>,
        #[arg(long)]
        config: Option<String>,
        #[arg(long, default_value = "rocksdb")]
        storage: String,
        #[arg(long)]
        rocksdb_path: Option<String>,
        #[arg(long, default_value_t = 8002)]
        api_port: u16,
    },
}

#[derive(Subcommand, Debug)]
enum WalletCommands {
    /// Show wallet status
    Status {
        #[arg(long, default_value = "http://localhost:8000")]
        api: String,
    },
    /// Link a wallet address
    Link {
        /// Wallet address to link
        address: String,
        /// Wallet signature for verification
        #[arg(long)]
        signature: String,
        #[arg(long, default_value = "http://localhost:8000")]
        api: String,
    },
    /// Unlink current wallet
    Unlink {
        #[arg(long, default_value = "http://localhost:8000")]
        api: String,
    },
}

#[derive(Subcommand, Debug)]
enum AccountCommands {
    /// Generate a new keypair
    New,
    /// Get account balance
    Balance {
        address: Option<String>,
        #[arg(long, default_value = "http://localhost:8000")]
        api: String,
    },
}

#[derive(Subcommand, Debug)]
enum TxCommands {
    /// Send a transaction
    Send {
        /// Recipient address
        #[arg(long)]
        to: String,
        /// Amount to send (in OURO)
        #[arg(long)]
        amount: f64,
        /// API endpoint
        #[arg(long, default_value = "http://localhost:8000")]
        api: String,
    },
}

pub async fn run() -> std::io::Result<()> {
    let cli = Cli::parse();

    // Initialize unified configuration
    if let Err(e) = config_manager::init_config().await {
        eprintln!(" Failed to initialize configuration: {}", e);
        std::process::exit(1);
    }

    // Wire Join --peer and --bootstrap_url into the environment before shared startup
    if let Commands::Join { peer, bootstrap_url, .. } = &cli.command {
        if let Some(peer_addr) = peer {
            let existing = std::env::var("PEER_ADDRS").unwrap_or_default();
            let combined = if existing.is_empty() {
                peer_addr.clone()
            } else {
                format!("{},{}", existing, peer_addr)
            };
            std::env::set_var("PEER_ADDRS", &combined);
        }
        if let Some(url) = bootstrap_url {
            std::env::set_var("BOOTSTRAP_URL", url);
        }
    }

    match &cli.command {
        Commands::Start {
            headless,
            role,
            min_difficulty,
            max_difficulty,
        }
        | Commands::Join {
            headless,
            role,
            min_difficulty,
            max_difficulty,
            ..
        } => {
            let is_headless = *headless;
            // load .env for local development (if present)
            dotenvy::dotenv().ok();

            // Apply role override
            let current_role = {
                let mut config = config_manager::CONFIG.write().await;
                let r = match role.to_lowercase().as_str() {
                    "medium" => crate::config_manager::NodeRole::Medium,
                    "light" => crate::config_manager::NodeRole::Light,
                    _ => crate::config_manager::NodeRole::Heavy,
                };
                config.role = r.clone();
                r
            };

            // MASTER DISPATCHER: If not Heavy, hand off to Python managed process
            if current_role != crate::config_manager::NodeRole::Heavy {
                return handoff_to_python_node(current_role, &cli.command).await;
            }

            println!(" Node starting as role: HEAVY (Rust Settlement Engine)");

            // Apply difficulty overrides
            {
                let mut config = config_manager::CONFIG.write().await;
                if let Some(min) = min_difficulty {
                    config.adaptive_difficulty.min_difficulty = Some(min.clone());
                    println!(" Min difficulty override: {}", min);
                }
                if let Some(max) = max_difficulty {
                    config.adaptive_difficulty.max_difficulty = Some(max.clone());
                    println!(" Max difficulty override: {}", max);
                }
                // Save config immediately
                if let Err(e) = config.save() {
                    eprintln!(" Failed to save config overrides: {}", e);
                }
            }

            // Validate configuration before starting
            let config_validation = crate::config::validate_config();
            config_validation.print_summary();

            // Fail startup if configuration is invalid
            if !config_validation.valid {
                eprintln!("\n Configuration validation failed! Cannot start node.");
                eprintln!("   Fix the errors above and try again.\n");
                std::process::exit(1);
            }

            // Validate required keys
            match check_required_keys() {
                Ok(key_status) => {
                    if !key_status.warnings.is_empty() {
                        println!("\n=== Key Configuration Warnings ===");
                        for warning in &key_status.warnings {
                            println!("  [WARN] {}", warning);
                        }
                        println!("==================================\n");
                    }
                }
                Err(e) => {
                    eprintln!("\n Key validation failed: {}", e);
                    eprintln!("   Cannot start node without required keys.\n");
                    std::process::exit(1);
                }
            }

            // Get configuration from unified manager
            let config = config_manager::CONFIG.read().await;
            let db_path = config.storage.db_path.clone();

            // Open RocksDB storage
            println!(" Opening RocksDB storage at {}", db_path);
            let db_pool = Arc::new(open_db(&db_path));

            // start P2P network
            let listen = config.network.listen_addr.clone();
            let tor_config = tor::TorConfig::from_env();
            let (bcast_sender, mut inbound_rx, peer_store) =
                start_network(&listen, Some(tor_config)).await;

            // start API server
            let api_addr = config.network.api_addr.clone();
            let api_addr_parsed: SocketAddr = api_addr.parse().map_err(|e| {
                eprintln!(" Invalid API_ADDR: {}", e);
                std::io::Error::new(std::io::ErrorKind::InvalidInput, e)
            })?;

            // TPS Optimization: Initialize batch transaction writer
            let batch_writer = Arc::new(crate::batch_writer::BatchWriter::new((*db_pool).clone()));
            println!(" Batch transaction writer initialized (target: 20k-50k TPS)");

            // Build main API router
            let (main_router, ids) = crate::api::router(db_pool.clone(), peer_store.clone(), batch_writer.clone());

            // Initialize additional services for subchain/microchain/mainchain APIs

            // Phase 5: Initialize Multi-Sig Coordinator for decentralized anchor posting
            let multisig_enabled = std::env::var("ENABLE_MULTISIG")
                .map(|v| v.to_lowercase() == "true" || v == "1")
                .unwrap_or(false);

            let anchor_service = if multisig_enabled {
                println!(" Multi-sig anchor posting ENABLED");

                // Load validator public keys from database
                let validator_keys =
                    match crate::multisig::MultiSigCoordinator::load_validator_keys().await {
                        Ok(keys) => keys,
                        Err(e) => {
                            eprintln!("  Failed to load multi-sig validator keys: {}", e);
                            eprintln!("   Falling back to single-sig mode");
                            std::collections::HashMap::new()
                        }
                    };

                if !validator_keys.is_empty() {
                    let threshold = std::env::var("MULTISIG_THRESHOLD")
                        .ok()
                        .and_then(|s| s.parse::<usize>().ok())
                        .unwrap_or_else(|| (validator_keys.len() * 2 / 3) + 1); // Default: 2/3 + 1

                    match crate::multisig::MultiSigConfig::new(threshold, validator_keys) {
                        Ok(config) => {
                            let coordinator = crate::multisig::MultiSigCoordinator::new(config);
                            println!(
                                " Multi-sig coordinator initialized: {}/{} threshold",
                                threshold, coordinator.config.total_validators
                            );
                            Arc::new(crate::anchor_service::AnchorService::new_with_multisig(
                                db_pool.clone(),
                                coordinator,
                            ))
                        }
                        Err(e) => {
                            eprintln!("  Multi-sig config error: {}", e);
                            eprintln!("   Falling back to single-sig mode");
                            Arc::new(crate::anchor_service::AnchorService::new(db_pool.clone()))
                        }
                    }
                } else {
                    println!("  No validator keys found, using single-sig mode");
                    Arc::new(crate::anchor_service::AnchorService::new(db_pool.clone()))
                }
            } else {
                println!(" Multi-sig DISABLED (using single-sig anchor posting)");
                Arc::new(crate::anchor_service::AnchorService::new(db_pool.clone()))
            };

            // Phase 5: Initialize Validator Registry
            let validator_registry = Arc::new(
                crate::validator_registration::ValidatorRegistry::new(db_pool.clone()),
            );
            println!(" Validator registry initialized");

            // Suggestion 2: Initialize Subchain Market Registry
            let subchain_registry = Arc::new(crate::subchain::registry::SubchainRegistry::new());
            println!(" Subchain Market Registry initialized");

            // Build sub-routers (now with authentication!)
            let subchain_router = crate::subchain::api::router(Arc::new(db_pool.clone()), subchain_registry.clone());
            let microchain_router = crate::microchain::api::router();
            let mainchain_router = crate::mainchain::api::router(anchor_service.clone());

            // Build Ouro Coin and Token Bucket routers
            let ouro_coin_router = crate::ouro_coin::api::router(Arc::new(db_pool.clone()));
            // TODO_ROCKSDB: Re-enable when token_bucket module is converted
            let token_bucket_router = crate::token_bucket::api::router(Arc::new(db_pool.clone()));

            // Phase 5: Validator registration router
            let validator_router =
                crate::validator_registration::api::router(validator_registry.clone());

            // Combine all routers
            // IDS extension is layered here so nested routers (e.g. /subchain)
            // that use auth_middleware can access IntrusionDetectionSystem.
            let router = main_router
                .nest("/subchain", subchain_router)
                .nest("/microchain", microchain_router)
                .nest("/mainchain", mainchain_router)
                .nest("/ouro", ouro_coin_router)
                // TODO_ROCKSDB: Re-enable when token_bucket module is converted
                .nest("/bucket", token_bucket_router)
                .nest("/validators", validator_router)
                .layer(axum::Extension(ids));

            // Load TLS configuration (optional)
            let tls_config = load_tls_config();

            // SECURITY: Enforce TLS in production mode
            let is_production = std::env::var("ENVIRONMENT")
                .map(|e| e.to_lowercase() == "production" || e.to_lowercase() == "prod")
                .unwrap_or(false);

            if is_production && tls_config.is_none() {
                eprintln!("\n CRITICAL: Production deployment REQUIRES TLS/HTTPS!");
                eprintln!("   Set TLS_CERT_PATH and TLS_KEY_PATH environment variables.");
                eprintln!(
                    "   Or set ENVIRONMENT to 'development' if this is a dev/test instance.\n"
                );
                std::process::exit(1);
            }

            tokio::spawn(async move {
                if let Some(tls) = tls_config {
                    // HTTPS mode
                    println!(
                        " Starting API server with TLS on https://{}",
                        api_addr_parsed
                    );
                    if let Err(e) = axum_server::bind_rustls(api_addr_parsed, tls)
                        .serve(router.into_make_service_with_connect_info::<SocketAddr>())
                        .await
                    {
                        eprintln!(
                            " API server (HTTPS) crashed unexpectedly on {}: {}\
                            \n   Check if port is already in use or permissions are correct.",
                            api_addr_parsed, e
                        );
                        std::process::exit(1);
                    }
                } else {
                    // HTTP mode (fallback)
                    println!(
                        " Starting API server (HTTP only) on http://{}",
                        api_addr_parsed
                    );
                    if let Err(e) = axum_server::bind(api_addr_parsed)
                        .serve(router.into_make_service_with_connect_info::<SocketAddr>())
                        .await
                    {
                        eprintln!(
                            " API server (HTTP) crashed unexpectedly on {}: {}\
                            \n   Check if port is already in use or permissions are correct.",
                            api_addr_parsed, e
                        );
                        std::process::exit(1);
                    }
                }
            });

            // Initialize global storage (used by reconciliation and VM)
            // sled_storage::init_global_storage((*db_pool).clone());

            // DAG
            let mut dag = DAG::new((*db_pool).clone());

            // Initialize global mempool (used by consensus via select_transactions())
            mempool::init_global_mempool((*db_pool).clone());

            // Also keep local mempool handle for API/main loop
            let mempool = Mempool::new((*db_pool).clone());
            let mempool_arc = Arc::new(mempool);

            let _validators = vec![
                BFTNode {
                    name: "NodeA".into(),
                    private_key_seed: vec![],
                    dilithium_keypair: None, // Phase 6: PQ not enabled by default
                    pq_migration_phase: crate::crypto::hybrid::MigrationPhase::Phase1EdOrHybrid,
                },
                BFTNode {
                    name: "NodeB".into(),
                    private_key_seed: vec![],
                    dilithium_keypair: None,
                    pq_migration_phase: crate::crypto::hybrid::MigrationPhase::Phase1EdOrHybrid,
                },
                BFTNode {
                    name: "NodeC".into(),
                    private_key_seed: vec![],
                    dilithium_keypair: None,
                    pq_migration_phase: crate::crypto::hybrid::MigrationPhase::Phase1EdOrHybrid,
                },
            ];

            // Initialize HotStuff BFT consensus
            let node_id = std::env::var("NODE_ID").unwrap_or_else(|_| "node-1".into());
            let bft_peers: Vec<SocketAddr> = std::env::var("BFT_PEERS")
                .unwrap_or_else(|_| "".into())
                .split(',')
                .filter(|s| !s.trim().is_empty())
                .filter_map(|s| s.trim().parse().ok())
                .collect();

            // Convert SocketAddr to NodeId (String) for HotStuffConfig
            let peer_node_ids: Vec<String> =
                bft_peers.iter().map(|addr| addr.to_string()).collect();

            println!(" Initializing HotStuff consensus:");
            println!("   Node ID: {}", node_id);
            println!("   BFT Peers (addresses): {:?}", bft_peers);
            println!("   BFT Peers (node IDs): {:?}", peer_node_ids);

            // Generate or load secret seed (32 bytes for Ed25519)
            let secret_seed = std::env::var("BFT_SECRET_SEED")
                .ok()
                .and_then(|s| hex::decode(s).ok())
                .unwrap_or_else(|| {
                    println!(
                        "  BFT_SECRET_SEED not set, using placeholder zeros (NOT FOR PRODUCTION)"
                    );
                    vec![0u8; 32]
                });

            // Phase 6: Load Dilithium Key (M6 fix: persist to disk)
            let dilithium_keypair = if std::env::var("ENABLE_PQ_CRYPTO").unwrap_or_default() == "true" {
                println!(" PQ Crypto ENABLED: Loading Dilithium5 keys...");
                let dil_path = std::path::PathBuf::from(&db_path).join(".dilithium_keypair");
                if dil_path.exists() {
                    match std::fs::read(&dil_path) {
                        Ok(bytes) => {
                            match bincode::deserialize::<DilithiumKeypair>(&bytes) {
                                Ok(kp) => {
                                    println!("  Loaded existing Dilithium5 keypair from {}", dil_path.display());
                                    Some(kp)
                                }
                                Err(e) => {
                                    println!("  Warning: Failed to deserialize Dilithium keypair: {}. Generating new one.", e);
                                    let kp = DilithiumKeypair::generate();
                                    let _ = std::fs::write(&dil_path, bincode::serialize(&kp).unwrap_or_default());
                                    crate::crypto::set_restrictive_permissions(&dil_path);
                                    Some(kp)
                                }
                            }
                        }
                        Err(e) => {
                            println!("  Warning: Failed to read Dilithium keypair file: {}. Generating new one.", e);
                            let kp = DilithiumKeypair::generate();
                            let _ = std::fs::write(&dil_path, bincode::serialize(&kp).unwrap_or_default());
                            crate::crypto::set_restrictive_permissions(&dil_path);
                            Some(kp)
                        }
                    }
                } else {
                    println!("  Generating new Dilithium5 keypair (first run)...");
                    let kp = DilithiumKeypair::generate();
                    let _ = std::fs::write(&dil_path, bincode::serialize(&kp).unwrap_or_default());
                    crate::crypto::set_restrictive_permissions(&dil_path);
                    Some(kp)
                }
            } else {
                None
            };

            let hotstuff_config = HotStuffConfig {
                id: node_id.clone(),
                peers: peer_node_ids,
                timeout_ms: 5000,
                secret_seed,
                dilithium_keypair,
            };

            let broadcast_handle = BroadcastHandle::new(bft_peers.clone());
            let state = Arc::new(BFTState::new(db_pool.clone()));
            let validator_registry = Arc::new(ValidatorRegistry::new());

            let hotstuff = Arc::new(HotStuff::new(
                Arc::new(hotstuff_config),
                broadcast_handle,
                state.clone(),
                validator_registry.clone(),
            ));

            // Start BFT message server on port 9091
            let bft_port = std::env::var("BFT_PORT")
                .unwrap_or_else(|_| "9091".into())
                .parse::<u16>()
                .unwrap_or(9091);

            let bft_addr: SocketAddr = format!("0.0.0.0:{}", bft_port).parse().map_err(|e| {
                eprintln!(" Invalid BFT_PORT configuration: {}", e);
                std::io::Error::new(std::io::ErrorKind::InvalidInput, e)
            })?;

            let hotstuff_for_server = hotstuff.clone();
            tokio::spawn(async move {
                println!(" Starting BFT server on {}", bft_addr);
                if let Err(e) = start_bft_server(bft_addr, hotstuff_for_server).await {
                    eprintln!(" BFT server error: {}", e);
                }
            });

            // Start liveness timer (checks every 1s)
            let _liveness_handle = hotstuff.clone().spawn_liveness_timer(1000);
            println!(" BFT liveness timer started");

            // load anchor key (optional)
            let anchor_key = keys::load_secret("ANCHOR_PRIVATE_KEY");
            if let Some(ref k) = anchor_key {
                println!("Loaded ANCHOR_PRIVATE_KEY (length {})", k.len());
            } else {
                eprintln!("WARNING: ANCHOR_PRIVATE_KEY not provided via Docker secret or env. Anchor operations will be disabled unless provided.");
            }

            // inbound p2p handler (spawn)
            let mempool_for_inbound = mempool_arc.clone();
            let _db_pool_for_inbound = db_pool.clone();
            tokio::spawn(async move {
                while let Some(txn) = inbound_rx.recv().await {
                    let message = format!("{}:{}:{}", txn.sender, txn.recipient, txn.amount);

                    // Strict verification — require real ed25519 verification (no fallback)
                    let verified =
                        verify_ed25519_hex(&txn.public_key, &txn.signature, message.as_bytes());
                    if !verified {
                        println!("P2P inbound txn signature invalid: {}", txn.id);
                        continue;
                    }

                    if let Err(e) = mempool_for_inbound.add_tx(&txn) {
                        println!("mempool add err (inbound): {}", e);
                    } else {
                        println!("P2P inbound txn added to mempool: {}", txn.id);
                    }
                }
            });

            let mut last_checkpoint = Instant::now();
            let checkpoint_interval = Duration::from_secs(5); // Consensus trigger interval

                println!("\n Ouroboros Node Running!");
                println!("   P2P: {}", listen);
                println!("   API: http://{}", api_addr);
                println!("   Storage: RocksDB ({})", db_path);

                if !is_headless {
                    println!("\n TIP: View live dashboard with: ouro status");
                }

                // Run API server
            println!(
                "   HotStuff consensus will propose blocks every {} seconds",
                checkpoint_interval.as_secs()
            );

            loop {
                // Get current role
                let role = {
                    let config = config_manager::CONFIG.read().await;
                    config.role.clone()
                };

                // check file-based submission
                let path = Path::new("dag_txn.json");
                handle_incoming_file(&path, &mut dag, &mempool_arc, &bcast_sender).await;

                // reconciliation - only for Heavy nodes (Settlement)
                if role == crate::config_manager::NodeRole::Heavy {
                    reconciliation::reconcile_token_spends(&mut dag);
                }

                // export state for debugging
                if let Err(e) = dag.export_state() {
                    log::warn!("Failed to export DAG state: {}", e);
                }

                // Consensus-driven block creation (HotStuff) - ONLY for Heavy nodes
                if role == crate::config_manager::NodeRole::Heavy && last_checkpoint.elapsed() >= checkpoint_interval {
                    // Trigger consensus view - HotStuff will propose a block if this node is the leader
                    println!(" Triggering consensus view...");
                    if let Err(e) = hotstuff.start_view().await {
                        eprintln!(" Consensus view failed: {}", e);
                    }

                    // Also run legacy checkpoint for balance finalization
                    // TODO: This will be fully integrated into consensus finalization callback
                    let block_txns = mempool_arc.pop_for_block(100).unwrap_or_default();

                    if !block_txns.is_empty() {
                        let mut tx_ids = vec![];
                        let mut block_txns_ref: Vec<Transaction> = Vec::new();
                        for tx in block_txns.iter() {
                            match dag.add_transaction(tx.clone()) {
                                Ok(_) => {
                                    tx_ids.push(tx.id);
                                    block_txns_ref.push(tx.clone());
                                }
                                Err(e) => println!("dag.add_transaction failed: {}", e),
                            }
                        }

                        if !tx_ids.is_empty() {
                            let block_id = Uuid::new_v4(); // Generate a new block ID
                            if let Err(e) = finalize_block(block_id).await {
                                println!(" Failed to finalize block: {}", e);
                                // Re-add txs to mempool if block finalization failed
                                for tx in &block_txns_ref {
                                    if let Err(err) = mempool_arc.add_tx(tx) {
                                        println!(
                                            "Failed to re-add tx {} to mempool: {}",
                                            tx.id, err
                                        );
                                    }
                                }
                                last_checkpoint = Instant::now();
                                continue;
                            }

                            // Create a Block struct for serialization and database insertion
                            let block = bft::consensus::Block {
                                id: block_id,
                                timestamp: Utc::now(),
                                tx_ids: tx_ids.clone(),
                                validator_signatures: vec![], // This will be filled by consensus
                                proposer: "genesis".to_string(), // TODO: Get actual proposer from consensus
                                height: 0, // TODO: Get actual height from blockchain
                            };
                            println!(" Block ID: {} at {}", block.id, block.timestamp);

                            // execute contracts (VM)
                            match vm::execute_contracts(&db_pool, &block_txns_ref) {
                                Ok(_res) => {
                                    // Persist block to RocksDB (authoritative storage)
                                    let block_key = format!("block:{}", block.id);
                                    if let Err(e) =
                                        put(&db_pool, block_key.clone().into_bytes(), &block)
                                    {
                                        println!(
                                            "Warning: Failed to persist block to local kv: {}",
                                            e
                                        );
                                    }

                                    let mut index_entries: Vec<(Vec<u8>, String)> = Vec::new();
                                    for txid in block.tx_ids.iter() {
                                        index_entries.push((
                                            format!("tx_index:{}", txid).into_bytes(),
                                            block.id.to_string(),
                                        ));
                                    }
                                    if let Err(e) = batch_put(&db_pool, index_entries) {
                                        println!(
                                                                    "Warning: Failed to persist tx_index entries to local kv: {}",
                                                                    e
                                                                );
                                    }

                                    println!("Persisted block {} to RocksDB", block.id);
                                }
                                Err(e) => {
                                    println!(
                                        " Contract execution failed for block {}: {}",
                                        block.id, e
                                    );
                                    // Put txs back into mempool
                                    for tx in &block_txns_ref {
                                        if let Err(err) = mempool_arc.add_tx(tx) {
                                            println!("Failed to re-add tx {} to mempool after contract failure: {}", tx.id, err);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    last_checkpoint = Instant::now();
                }

                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }

        // ==================== CLI DASHBOARD COMMANDS ====================
        Commands::Status { once, api } => {
            handle_status_command(!*once, api).await?;
        }

        Commands::Peers { api } => {
            handle_peers_command(api).await?;
        }

        Commands::Consensus { api } => {
            handle_consensus_command(api).await?;
        }

        Commands::Mempool { api } => {
            handle_mempool_command(api).await?;
        }

        Commands::Resources { api } => {
            handle_resources_command(api).await?;
        }

        Commands::Logs {
            lines,
            follow,
            export,
        } => {
            handle_logs_command(*lines, *follow, export.clone()).await?;
        }

        Commands::Stop { api } => {
            handle_stop_command(api).await?;
        }

        Commands::Restart { api } => {
            handle_restart_command(api).await?;
        }

        Commands::Diagnose { export } => {
            handle_diagnose_command(export.clone()).await?;
        }

        Commands::Wallet { command } => match command {
            WalletCommands::Status { api } => {
                handle_wallet_status(api).await?;
            }
            WalletCommands::Link {
                address,
                signature,
                api,
            } => {
                handle_wallet_link(address, signature, api).await?;
            }
            WalletCommands::Unlink { api } => {
                handle_wallet_unlink(api).await?;
            }
        },

        Commands::Account { command } => match command {
            AccountCommands::New => {
                handle_account_new().await?;
            }
            AccountCommands::Balance { address, api } => {
                handle_account_balance(address.clone(), api).await?;
            }
        },

        Commands::Tx { command } => match command {
            TxCommands::Send { to, amount, api } => {
                handle_tx_send(to, *amount, api).await?;
            }
        },

        Commands::Resync { api } => {
            handle_resync_command(api).await?;
        }

        Commands::Backup { output } => {
            handle_backup_command(output.clone()).await?;
        }

        Commands::RegisterUser { wallet_address } => {
            let mut config = config_manager::CONFIG.write().await;
            if config.wallet.is_none() {
                config.wallet = Some(config_manager::WalletLinkConfig {
                    wallet_address: wallet_address.clone(),
                    linked_at: Utc::now().to_rfc3339(),
                    wallet_signature: "".to_string(),
                    node_signature: "".to_string(),
                });
            } else {
                if let Some(w) = config.wallet.as_mut() {
                    w.wallet_address = wallet_address.clone();
                }
            }
            config.save().map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
            println!(" Registered wallet: {}", wallet_address);
        }

        Commands::RegisterNode { node_id } => {
            let mut config = config_manager::CONFIG.write().await;
            let id = node_id.clone().unwrap_or_else(|| Uuid::new_v4().to_string());
            config.identity.node_id = id.clone();
            config.save().map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
            println!(" Registered node ID: {}", id);
        }

        Commands::Logout {} => {
            let mut config = config_manager::CONFIG.write().await;
            config.identity.node_id = Uuid::new_v4().to_string();
            config.wallet = None;
            config.security.api_keys = vec![];
            config.save().map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
            println!(" Logged out and cleared credentials.");
        }

        Commands::Roles {} => {
            handle_roles_command().await?;
        }

        Commands::Oracle {
            peer,
            config,
            storage,
            rocksdb_path,
            api_port,
        } => {
            handle_oracle_command(
                peer.clone(),
                config.clone(),
                storage,
                rocksdb_path.clone(),
                *api_port,
            )
            .await?;
        }

        Commands::Benchmark { cycles } => {
            handle_benchmark_command(*cycles).await?;
        }
    }

    Ok(())
}

// ==================== CLI COMMAND HANDLERS ====================

async fn fetch_api<T: serde::de::DeserializeOwned>(url: &str) -> Result<T, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;
    let mut request = client.get(url);

    // Add API key authentication if available
    if let Ok(api_keys) = std::env::var("API_KEYS") {
        if let Some(key) = api_keys.split(',').next().filter(|k| !k.trim().is_empty()) {
            request = request.header("Authorization", format!("Bearer {}", key.trim()));
        }
    }

    let response = request
        .send()
        .await
        .map_err(|e| format!("Failed to connect: {}", e))?;
    if !response.status().is_success() {
        return Err(format!("API returned error: {}", response.status()));
    }
    response
        .json::<T>()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))
}

async fn handle_status_command(watch: bool, api: &str) -> std::io::Result<()> {
    use cli_dashboard::*;

    // Enter alternate screen for live-updating watch mode
    if watch {
        enter_dashboard_mode();
        // Enable raw mode so we can poll for Ctrl+C via crossterm events
        let _ = crossterm::terminal::enable_raw_mode();
    }

    // Ensure terminal is restored on exit (including Ctrl+C via tokio signal)
    let result = handle_status_loop(watch, api).await;

    if watch {
        let _ = crossterm::terminal::disable_raw_mode();
        leave_dashboard_mode();
    }

    result
}

async fn handle_status_loop(watch: bool, api: &str) -> std::io::Result<()> {
    use cli_dashboard::*;

    loop {
        // Check for Ctrl+C in watch mode
        if watch && poll_ctrl_c() {
            break;
        }

        let mut data = DashboardData::default();

        // Fetch status from API
        if let Ok(status) = fetch_api::<serde_json::Value>(&format!("{}/health", api)).await {
            data.status = NodeStatus::Running;
            if let Some(name) = status.get("node_name").and_then(|v| v.as_str()) {
                data.node_name = name.to_string();
            }
        } else {
            data.status = NodeStatus::Stopped;
            if watch {
                print_dashboard(&data);
            } else {
                print_dashboard_once(&data);
                println!(
                    "\n{}Node appears to be offline.{}\n",
                    colors::YELLOW,
                    colors::RESET
                );
                println!("  Start your node:  ouro start");
                println!("  Custom API:       ouro status --api http://HOST:PORT");
                println!("  Print once:       ouro status --once\n");
                break;
            }
            // Wait 2s but check for Ctrl+C every 100ms
            for _ in 0..20 {
                tokio::time::sleep(Duration::from_millis(100)).await;
                if poll_ctrl_c() {
                    return Ok(());
                }
            }
            continue;
        }

        // Try to fetch additional metrics
        if let Ok(identity) = fetch_api::<serde_json::Value>(&format!("{}/identity", api)).await {
            if let Some(name) = identity.get("public_name").and_then(|v| v.as_str()) {
                if !name.is_empty() {
                    data.node_name = name.to_string();
                }
            }
            if let Some(uptime) = identity.get("total_uptime_secs").and_then(|v| v.as_u64()) {
                data.uptime_secs = uptime;
            }
            if let Some(role) = identity.get("role").and_then(|v| v.as_str()) {
                data.role = role.to_string();
            }
            if let Some(diff) = identity.get("difficulty").and_then(|v| v.as_str()) {
                data.difficulty = diff.to_string();
            }
        }

        // Fetch peers
        if let Ok(peers) = fetch_api::<serde_json::Value>(&format!("{}/peers", api)).await {
            if let Some(count) = peers.get("count").and_then(|v| v.as_u64()) {
                data.peer_count = count as u32;
            }
            if let Some(peer_list) = peers.get("peers").and_then(|v| v.as_array()) {
                data.top_peers = peer_list
                    .iter()
                    .take(3)
                    .filter_map(|p| {
                        Some(PeerInfo {
                            id: p.get("id")?.as_str()?.to_string(),
                            addr: p.get("addr")?.as_str()?.to_string(),
                            role: p.get("role").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
                            latency_ms: p.get("latency_ms").and_then(|v| v.as_u64()).unwrap_or(0)
                                as u32,
                        })
                    })
                    .collect();
            }
        }

        // Fetch consensus state
        if let Ok(consensus) = fetch_api::<serde_json::Value>(&format!("{}/consensus", api)).await {
            data.view = consensus.get("view").and_then(|v| v.as_u64()).unwrap_or(0);
            data.leader = consensus
                .get("leader")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            data.highest_qc = consensus
                .get("highest_qc_view")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            if let Some(last) = consensus.get("last_committed") {
                if let Some(h) = last.get("height").and_then(|v| v.as_u64()) {
                    data.last_block_height = h;
                }
                if let Some(t) = last.get("timestamp").and_then(|v| v.as_str()) {
                    data.last_block_time = t.to_string();
                }
            }
        }

        // Fetch metrics (TPS, block height, etc.) - use JSON endpoint
        if let Ok(metrics) = fetch_api::<serde_json::Value>(&format!("{}/metrics/json", api)).await {
            if let Some(tps) = metrics.get("tps_1m").and_then(|v| v.as_f64()) {
                data.tps_1m = tps;
            }
            if let Some(tps) = metrics.get("tps_5m").and_then(|v| v.as_f64()) {
                data.tps_5m = tps;
            }
            if let Some(count) = metrics.get("mempool_count").and_then(|v| v.as_u64()) {
                data.mempool_tx_count = count as u32;
            }
            if let Some(height) = metrics.get("block_height").and_then(|v| v.as_u64()) {
                data.local_height = height;
                data.last_block_height = height;
            }
            if let Some(tip) = metrics.get("network_tip").and_then(|v| v.as_u64()) {
                data.network_tip = tip;
            }
            if let Some(sync) = metrics.get("sync_percent").and_then(|v| v.as_f64()) {
                data.sync_percent = sync;
            }
        }

        // Fetch resources (CPU, MEM, DISK, NET)
        if let Ok(resources) = fetch_api::<serde_json::Value>(&format!("{}/resources", api)).await {
            if let Some(cpu) = resources.get("cpu_pct").and_then(|v| v.as_f64()) {
                data.cpu_percent = cpu;
            }
            if let Some(mem) = resources.get("mem_mb").and_then(|v| v.as_u64()) {
                data.mem_mb = mem;
            }
            if let Some(disk_used) = resources.get("disk_gb_used").and_then(|v| v.as_f64()) {
                data.disk_used_gb = disk_used;
            }
            if let Some(disk_total) = resources.get("disk_gb_total").and_then(|v| v.as_f64()) {
                data.disk_total_gb = disk_total;
            }
            if let Some(net_in) = resources.get("net_in_kbps").and_then(|v| v.as_f64()) {
                data.net_in_kbps = net_in;
            }
            if let Some(net_out) = resources.get("net_out_kbps").and_then(|v| v.as_f64()) {
                data.net_out_kbps = net_out;
            }
        }

        if watch {
            print_dashboard(&data);
        } else {
            print_dashboard_once(&data);
            break;
        }

        // Wait 2s but check for Ctrl+C every 100ms
        for _ in 0..20 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if poll_ctrl_c() {
                return Ok(());
            }
        }
    }

    Ok(())
}

async fn handle_peers_command(api: &str) -> std::io::Result<()> {
    use cli_dashboard::*;

    match fetch_api::<serde_json::Value>(&format!("{}/peers", api)).await {
        Ok(peers) => {
            let peer_list: Vec<PeerInfo> = peers
                .get("peers")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|p| {
                            Some(PeerInfo {
                                id: p.get("id")?.as_str()?.to_string(),
                                addr: p.get("addr")?.as_str()?.to_string(),
                                role: p.get("role").and_then(|v| v.as_str()).unwrap_or("heavy").to_string(),
                                latency_ms: p
                                    .get("latency_ms")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0)
                                    as u32,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();

            print_peers(&peer_list);
        }
        Err(e) => {
            eprintln!(
                "{}Error:{} Failed to fetch peers: {}",
                colors::RED,
                colors::RESET,
                e
            );
            eprintln!("Is the node running? Try: ouro start");
        }
    }

    Ok(())
}

async fn handle_consensus_command(api: &str) -> std::io::Result<()> {
    use cli_dashboard::*;

    let mut data = DashboardData::default();

    if let Ok(consensus) = fetch_api::<serde_json::Value>(&format!("{}/consensus", api)).await {
        data.view = consensus.get("view").and_then(|v| v.as_u64()).unwrap_or(0);
        data.leader = consensus
            .get("leader")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        data.highest_qc = consensus
            .get("highest_qc_view")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        if let Some(last) = consensus.get("last_committed") {
            data.last_block_height = last.get("height").and_then(|v| v.as_u64()).unwrap_or(0);
            data.last_block_time = last
                .get("timestamp")
                .and_then(|v| v.as_str())
                .unwrap_or("N/A")
                .to_string();
        }
    }

    print_consensus(&data);
    Ok(())
}

async fn handle_mempool_command(api: &str) -> std::io::Result<()> {
    use cli_dashboard::*;

    let mut data = DashboardData::default();

    if let Ok(mempool) = fetch_api::<serde_json::Value>(&format!("{}/mempool", api)).await {
        data.mempool_tx_count = mempool
            .get("tx_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
        data.mempool_avg_age_secs = mempool
            .get("avg_age_seconds")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        data.tps_1m = mempool
            .get("tps_1m")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        data.tps_5m = mempool
            .get("tps_5m")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
    }

    print_mempool(&data);
    Ok(())
}

async fn handle_resources_command(api: &str) -> std::io::Result<()> {
    use cli_dashboard::*;

    let mut data = DashboardData::default();

    if let Ok(resources) = fetch_api::<serde_json::Value>(&format!("{}/resources", api)).await {
        data.cpu_percent = resources
            .get("cpu_pct")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        data.mem_mb = resources
            .get("mem_mb")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        data.disk_used_gb = resources
            .get("disk_gb_used")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        data.disk_total_gb = resources
            .get("disk_gb_total")
            .and_then(|v| v.as_f64())
            .unwrap_or(100.0);
        data.net_in_kbps = resources
            .get("net_in_kbps")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        data.net_out_kbps = resources
            .get("net_out_kbps")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
    }

    print_resources(&data);
    Ok(())
}

async fn handle_logs_command(
    lines: u32,
    follow: bool,
    export: Option<String>,
) -> std::io::Result<()> {
    use cli_dashboard::colors;

    let log_path = std::env::var("LOG_PATH").unwrap_or_else(|_| "ouro.log".to_string());

    if let Some(export_path) = export {
        // Export logs to file
        if std::path::Path::new(&log_path).exists() {
            std::fs::copy(&log_path, &export_path)?;
            println!(
                "{}Logs exported to: {}{}",
                colors::GREEN,
                export_path,
                colors::RESET
            );
        } else {
            eprintln!(
                "{}Error:{} Log file not found at {}",
                colors::RED,
                colors::RESET,
                log_path
            );
        }
        return Ok(());
    }

    if !std::path::Path::new(&log_path).exists() {
        println!(
            "{}No log file found at {}{}",
            colors::YELLOW,
            log_path,
            colors::RESET
        );
        println!("Node may not be running or logs are sent to stdout.");
        return Ok(());
    }

    // Read and display logs
    let content = std::fs::read_to_string(&log_path)?;
    let all_lines: Vec<&str> = content.lines().collect();
    let start = all_lines.len().saturating_sub(lines as usize);

    println!(
        "{}=== Last {} log entries ==={}",
        colors::CYAN,
        lines,
        colors::RESET
    );
    for line in &all_lines[start..] {
        // Color code log levels
        if line.contains("ERROR") || line.contains("error") {
            println!("{}{}{}", colors::RED, line, colors::RESET);
        } else if line.contains("WARN") || line.contains("warn") {
            println!("{}{}{}", colors::YELLOW, line, colors::RESET);
        } else if line.contains("INFO") || line.contains("info") {
            println!("{}", line);
        } else {
            println!("{}{}{}", colors::DIM, line, colors::RESET);
        }
    }

    if follow {
        println!(
            "\n{}Following logs (Ctrl+C to stop)...{}",
            colors::DIM,
            colors::RESET
        );
        // In a real implementation, we'd tail -f the log file
        // For now, just poll periodically
        let mut last_size = std::fs::metadata(&log_path)?.len();

        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;

            let current_size = std::fs::metadata(&log_path)?.len();
            if current_size > last_size {
                let content = std::fs::read_to_string(&log_path)?;
                let new_content = &content[last_size as usize..];
                for line in new_content.lines() {
                    if line.contains("ERROR") {
                        println!("{}{}{}", colors::RED, line, colors::RESET);
                    } else if line.contains("WARN") {
                        println!("{}{}{}", colors::YELLOW, line, colors::RESET);
                    } else {
                        println!("{}", line);
                    }
                }
                last_size = current_size;
            }
        }
    }

    Ok(())
}

async fn handle_stop_command(api: &str) -> std::io::Result<()> {
    use cli_dashboard::colors;

    println!("Sending stop command to node...");

    match reqwest::Client::new()
        .post(&format!("{}/admin/stop", api))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            println!(
                "{}Node stopped successfully.{}",
                colors::GREEN,
                colors::RESET
            );
        }
        Ok(resp) => {
            eprintln!(
                "{}Failed to stop node: {}{}",
                colors::RED,
                resp.status(),
                colors::RESET
            );
        }
        Err(e) => {
            // If connection refused, node might already be stopped
            if e.to_string().contains("connection refused") {
                println!("{}Node is not running.{}", colors::YELLOW, colors::RESET);
            } else {
                eprintln!("{}Error: {}{}", colors::RED, e, colors::RESET);
            }
        }
    }

    Ok(())
}

async fn handle_restart_command(api: &str) -> std::io::Result<()> {
    use cli_dashboard::colors;

    println!("Sending restart command to node...");

    match reqwest::Client::new()
        .post(&format!("{}/admin/restart", api))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            println!("{}Node restarting...{}", colors::GREEN, colors::RESET);
            println!(
                "{}Run 'ouro status --watch' to monitor startup.{}",
                colors::DIM,
                colors::RESET
            );
        }
        Ok(resp) => {
            eprintln!(
                "{}Failed to restart node: {}{}",
                colors::RED,
                resp.status(),
                colors::RESET
            );
        }
        Err(e) => {
            eprintln!("{}Error: {}{}", colors::RED, e, colors::RESET);
            eprintln!("Is the node running? Try: ouro start");
        }
    }

    Ok(())
}

async fn handle_diagnose_command(export: Option<String>) -> std::io::Result<()> {
    use cli_dashboard::colors;

    println!("{}Running diagnostics...{}\n", colors::CYAN, colors::RESET);

    let mut report = String::new();
    report.push_str("=== OUROBOROS NODE DIAGNOSTIC REPORT ===\n\n");
    report.push_str(&format!("Generated: {}\n", chrono::Utc::now()));
    report.push_str(&format!("Version: {}\n\n", env!("CARGO_PKG_VERSION")));

    // Check 1: Environment
    println!("[1/5] Checking environment...");
    report.push_str("--- Environment ---\n");

    let checks = [
        ("ROCKSDB_PATH", "Database path"),
        ("API_ADDR", "API address"),
        ("LISTEN_ADDR", "P2P address"),
        ("BFT_SECRET_SEED", "BFT key (set)"),
    ];

    for (var, desc) in &checks {
        let status = if std::env::var(var).is_ok() {
            "OK"
        } else {
            "NOT SET"
        };
        let color = if status == "OK" {
            colors::GREEN
        } else {
            colors::YELLOW
        };
        println!("  {}: {}{}{}", desc, color, status, colors::RESET);
        report.push_str(&format!("  {}: {}\n", desc, status));
    }

    // Check 2: Database
    println!("\n[2/5] Checking database...");
    report.push_str("\n--- Database ---\n");

    let db_path = std::env::var("ROCKSDB_PATH").unwrap_or_else(|_| "sled_data".to_string());
    if std::path::Path::new(&db_path).exists() {
        let size = fs_extra::dir::get_size(&db_path).unwrap_or(0);
        println!("  Database exists: {}OK{}", colors::GREEN, colors::RESET);
        println!("  Size: {}", cli_dashboard::format_bytes(size));
        report.push_str(&format!("  Database: OK ({} bytes)\n", size));
    } else {
        println!("  Database: {}NOT FOUND{}", colors::RED, colors::RESET);
        report.push_str("  Database: NOT FOUND\n");
    }

    // Check 3: Network connectivity
    println!("\n[3/5] Checking network...");
    report.push_str("\n--- Network ---\n");

    let api_addr = std::env::var("API_ADDR").unwrap_or_else(|_| "127.0.0.1:8000".to_string());
    // 0.0.0.0 is valid for binding but not for connecting; use localhost instead
    let connect_addr = api_addr.replace("0.0.0.0", "127.0.0.1");
    match reqwest::get(&format!("http://{}/health", connect_addr)).await {
        Ok(_) => {
            println!("  API server: {}ONLINE{}", colors::GREEN, colors::RESET);
            report.push_str("  API server: ONLINE\n");
        }
        Err(_) => {
            println!("  API server: {}OFFLINE{}", colors::RED, colors::RESET);
            report.push_str("  API server: OFFLINE\n");
        }
    }

    // Check 4: Disk space
    println!("\n[4/5] Checking disk space...");
    report.push_str("\n--- Disk ---\n");
    // Simplified disk check
    println!(
        "  Disk check: {}SKIPPED{} (platform-specific)",
        colors::YELLOW,
        colors::RESET
    );
    report.push_str("  Disk check: SKIPPED\n");

    // Check 5: Memory
    println!("\n[5/5] Checking system resources...");
    report.push_str("\n--- Resources ---\n");
    println!(
        "  Resource check: {}SKIPPED{} (platform-specific)",
        colors::YELLOW,
        colors::RESET
    );
    report.push_str("  Resource check: SKIPPED\n");

    report.push_str("\n=== END DIAGNOSTIC REPORT ===\n");

    // Export if requested
    if let Some(path) = export {
        std::fs::write(&path, &report)?;
        println!(
            "\n{}Diagnostic report exported to: {}{}",
            colors::GREEN,
            path,
            colors::RESET
        );
    }

    println!("\n{}Diagnostics complete.{}", colors::GREEN, colors::RESET);

    Ok(())
}

async fn handle_wallet_status(api: &str) -> std::io::Result<()> {
    use cli_dashboard::colors;

    match fetch_api::<serde_json::Value>(&format!("{}/wallet/link", api)).await {
        Ok(wallet) => {
            let linked = wallet
                .get("linked")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            if linked {
                let addr = wallet
                    .get("wallet_address")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let linked_at = wallet
                    .get("linked_at")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");

                println!("\n{}WALLET STATUS{}", colors::BOLD, colors::RESET);
                println!("{}", cli_dashboard::horizontal_line(40));
                println!("Status:      {}Linked{}", colors::GREEN, colors::RESET);
                println!("Address:     {}", addr);
                println!("Linked at:   {}", linked_at);
            } else {
                println!("\n{}WALLET STATUS{}", colors::BOLD, colors::RESET);
                println!("{}", cli_dashboard::horizontal_line(40));
                println!("Status:      {}Not Linked{}", colors::YELLOW, colors::RESET);
                println!("\nTo link a wallet:");
                println!("  ouro wallet link <address> --signature <sig>");
            }
        }
        Err(e) => {
            eprintln!("{}Error:{} {}", colors::RED, colors::RESET, e);
        }
    }

    println!();
    Ok(())
}

async fn handle_wallet_link(address: &str, signature: &str, api: &str) -> std::io::Result<()> {
    use cli_dashboard::colors;

    println!("Linking wallet {}...", address);

    let payload = serde_json::json!({
        "wallet_address": address,
        "wallet_signature": signature
    });

    match reqwest::Client::new()
        .post(&format!("{}/wallet/link", api))
        .json(&payload)
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            println!(
                "{}Wallet linked successfully!{}",
                colors::GREEN,
                colors::RESET
            );
            println!("Address: {}", address);
        }
        Ok(resp) => {
            let error = resp.text().await.unwrap_or_default();
            eprintln!(
                "{}Failed to link wallet:{} {}",
                colors::RED,
                colors::RESET,
                error
            );
        }
        Err(e) => {
            eprintln!("{}Error:{} {}", colors::RED, colors::RESET, e);
        }
    }

    Ok(())
}

async fn handle_wallet_unlink(api: &str) -> std::io::Result<()> {
    use cli_dashboard::colors;

    println!("Unlinking wallet...");

    match reqwest::Client::new()
        .delete(&format!("{}/wallet/link", api))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            println!(
                "{}Wallet unlinked successfully.{}",
                colors::GREEN,
                colors::RESET
            );
        }
        Ok(resp) => {
            eprintln!(
                "{}Failed to unlink wallet: {}{}",
                colors::RED,
                resp.status(),
                colors::RESET
            );
        }
        Err(e) => {
            eprintln!("{}Error:{} {}", colors::RED, colors::RESET, e);
        }
    }

    Ok(())
}

async fn handle_resync_command(api: &str) -> std::io::Result<()> {
    use cli_dashboard::colors;

    println!(
        "{}WARNING:{} This will resync the node from the network.",
        colors::YELLOW,
        colors::RESET
    );
    println!("Local state may be temporarily unavailable during sync.\n");

    println!("Sending resync command...");

    match reqwest::Client::new()
        .post(&format!("{}/admin/resync", api))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            println!("{}Resync initiated.{}", colors::GREEN, colors::RESET);
            println!("Run 'ouro status --watch' to monitor progress.");
        }
        Ok(resp) => {
            eprintln!(
                "{}Failed to initiate resync: {}{}",
                colors::RED,
                resp.status(),
                colors::RESET
            );
        }
        Err(e) => {
            eprintln!("{}Error:{} {}", colors::RED, colors::RESET, e);
        }
    }

    Ok(())
}

async fn handle_backup_command(output: Option<String>) -> std::io::Result<()> {
    use cli_dashboard::colors;

    let db_path = std::env::var("ROCKSDB_PATH").unwrap_or_else(|_| "sled_data".to_string());
    let backup_path = output.unwrap_or_else(|| {
        format!(
            "ouroboros_backup_{}.tar.gz",
            chrono::Utc::now().format("%Y%m%d_%H%M%S")
        )
    });

    println!("Creating backup of database...");
    println!("Source: {}", db_path);
    println!("Target: {}", backup_path);

    if !std::path::Path::new(&db_path).exists() {
        eprintln!(
            "{}Error:{} Database not found at {}",
            colors::RED,
            colors::RESET,
            db_path
        );
        return Ok(());
    }

    // Create tar.gz backup
    // Note: In a real implementation, we'd use the tar crate
    println!(
        "\n{}Backup functionality requires tar crate.{}",
        colors::YELLOW,
        colors::RESET
    );
    println!("For now, manually copy the database directory:");
    println!("  cp -r {} {}", db_path, backup_path);

    Ok(())
}

async fn handle_oracle_command(
    peer: Option<String>,
    config: Option<String>,
    _storage: &str,
    rocksdb_path: Option<String>,
    api_port: u16,
) -> std::io::Result<()> {
    use cli_dashboard::colors;

    println!("{}Starting Oracle Node...{}", colors::CYAN, colors::RESET);

    // Initialize database path
    let db_path = rocksdb_path.unwrap_or_else(|| "oracle_data".to_string());

    // Load or create node identity
    let identity_path = format!("{}/.node_identity.json", db_path);
    let identity = node_identity::NodeIdentity::load_or_create(Path::new(&identity_path))
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

    println!("Node ID: {}", identity.short_id());

    // Load or create oracle config
    let config_path = config.unwrap_or_else(|| format!("{}/oracle_config.json", db_path));
    let config_path = Path::new(&config_path);

    // Ensure parent directory exists
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let oracle_config = oracle_node::OracleNodeConfig::load_or_create(config_path, &identity)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

    println!("Operator ID: {}", oracle_config.operator_id);
    println!("Data Sources: {:?}", oracle_config.data_sources);
    println!("Update Interval: {}ms", oracle_config.update_interval_ms);
    println!("API Port: {}", api_port);

    if let Some(ref p) = peer {
        println!("Connecting to peer: {}", p);
    }

    println!("Database: {}", db_path);

    // Load or generate signing key for oracle submissions
    let keypair_path = format!("{}/.oracle_keypair", db_path);
    let keypair_path = Path::new(&keypair_path);

    let signing_key = if keypair_path.exists() {
        crate::crypto::load_signing_key(keypair_path)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?
    } else {
        println!("Generating new oracle signing key...");
        crate::crypto::generate_and_write_signing_key(keypair_path)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?
    };

    let pubkey_hex = hex::encode(signing_key.verifying_key().to_bytes());
    println!("Oracle Public Key: {}...", &pubkey_hex[..16]);

    // Start oracle node
    let oracle = oracle_node::OracleNode::new(oracle_config, signing_key);

    println!("\n{}Oracle node running!{}", colors::GREEN, colors::RESET);
    println!("Press Ctrl+C to stop.\n");

    // Run oracle main loop
    oracle
        .run()
        .await
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

    Ok(())
}

async fn handle_benchmark_command(cycles: u32) -> std::io::Result<()> {
    use cli_dashboard::colors;
    use std::time::Instant;

    println!("\n{}Running Hardware Benchmark ({} cycles)...{}", colors::CYAN, cycles, colors::RESET);
    println!("This will determine the optimal difficulty for your node.");
    println!("{}", cli_dashboard::horizontal_line(60));

    let mut total_duration = Duration::new(0, 0);
    
    // Warmup
    print!("Warmup... ");
    let _ = crate::zk_proofs::generate_proof(1000, 100, "bench_recipient");
    println!("Done.");

    for i in 1..=cycles {
        print!("Cycle {}/{}: ", i, cycles);
        use std::io::Write;
        std::io::stdout().flush()?;
        
        let start = Instant::now();
        match crate::zk_proofs::generate_proof(1000, 100, "bench_recipient") {
            Ok(_) => {
                let duration = start.elapsed();
                total_duration += duration;
                println!("{} ms", duration.as_millis());
            }
            Err(e) => {
                println!("{}Failed: {}{}", colors::RED, e, colors::RESET);
            }
        }
    }

    let avg_ms = (total_duration.as_millis() as u64) / cycles as u64;
    println!("{}", cli_dashboard::horizontal_line(60));
    println!("Average Proof Generation Time: {} ms", avg_ms);

    let difficulty = if avg_ms < 500 {
        "extra_large"
    } else if avg_ms < 2000 {
        "large"
    } else if avg_ms < 5000 {
        "medium"
    } else {
        "small"
    };

    println!("Recommended Difficulty: {}{}{}", colors::GREEN, difficulty, colors::RESET);

    // Save to config
    let mut config = config_manager::CONFIG.write().await;
    config.adaptive_difficulty.current = difficulty.to_string();
    config.adaptive_difficulty.last_performance_ms = avg_ms;
    
    match config.save() {
        Ok(_) => println!("{}Configuration updated!{}", colors::GREEN, colors::RESET),
        Err(e) => eprintln!("{}Failed to save config: {}{}", colors::RED, e, colors::RESET),
    }

    Ok(())
}

async fn handle_account_new() -> std::io::Result<()> {
    use cli_dashboard::colors;
    use crate::crypto::{generate_and_write_signing_key, pubkey_bytes};
    use hex;

    println!("{}Generating new account...{}", colors::CYAN, colors::RESET);
    
    // Generate keypair
    let key_path = Path::new("wallet.key");
    if key_path.exists() {
        println!("{}Warning: wallet.key already exists! Backing up to wallet.key.bak{}", colors::YELLOW, colors::RESET);
        std::fs::rename("wallet.key", "wallet.key.bak")?;
    }

    let signing_key = match generate_and_write_signing_key(key_path) {
        Ok(k) => k,
        Err(e) => {
            eprintln!("{}Error generating key: {}{}", colors::RED, e, colors::RESET);
            return Ok(());
        }
    };

    let pubkey = pubkey_bytes(&signing_key);
    let address = hex::encode(pubkey);

    println!("{}Account created successfully!{}", colors::GREEN, colors::RESET);
    println!("Address: {}", address);
    println!("Private Key saved to: wallet.key (KEEP THIS SAFE!)");

    Ok(())
}

async fn handle_account_balance(address: Option<String>, api: &str) -> std::io::Result<()> {
    use cli_dashboard::colors;

    let addr = if let Some(a) = address {
        a
    } else {
        // Try to load from wallet.key
        let key_path = Path::new("wallet.key");
        if !key_path.exists() {
            eprintln!("{}Error: No address provided and wallet.key not found.{}", colors::RED, colors::RESET);
            return Ok(());
        }
        let signing_key = crate::crypto::load_signing_key(key_path).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        hex::encode(crate::crypto::pubkey_bytes(&signing_key))
    };

    println!("Checking balance for: {}", addr);

    // TODO: Implement actual balance endpoint in API
    // For now, we'll check the wallet link status or mock it
    match fetch_api::<serde_json::Value>(&format!("{}/wallet/link", api)).await {
        Ok(wallet) => {
             // Mock balance for now since /api/balance isn't fully implemented in router
             println!("{}Balance: 0.0000 OURO{}", colors::GREEN, colors::RESET); 
             // Note: Real implementation would hit /api/balance/:addr
        }
        Err(e) => {
            eprintln!("{}Error fetching balance: {}{}", colors::RED, e, colors::RESET);
        }
    }

    Ok(())
}

async fn handle_tx_send(to: &str, amount: f64, api: &str) -> std::io::Result<()> {
    use cli_dashboard::colors;
    use crate::dag::transaction::Transaction;
    use crate::crypto::{sign_bytes, pubkey_bytes};

    println!("{}Sending {} OURO to {}...{}", colors::CYAN, amount, to, colors::RESET);

    let key_path = Path::new("wallet.key");
    if !key_path.exists() {
        eprintln!("{}Error: wallet.key not found. Run 'ouro account new' first.{}", colors::RED, colors::RESET);
        return Ok(());
    }

    let signing_key = crate::crypto::load_signing_key(key_path).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    let sender_pubkey = pubkey_bytes(&signing_key);
    let sender_addr = hex::encode(&sender_pubkey);

    // Convert amount to microunits
    let amount_u64 = (amount * 100_000_000.0) as u64;

    // Create transaction payload
    // Note: In a real implementation we'd need nonce fetching
    let nonce = 0; 
    
    // Construct simplified payload for signing (matches what verify_ed25519_hex expects in API)
    // The API expects signature over tx_hash.
    // In a real system, we'd hash the transaction fields.
    // Here we'll generate a UUID as the hash for simplicity in this CLI tool
    let tx_hash = Uuid::new_v4().to_string();
    
    // Sign the tx_hash
    let signature_bytes = sign_bytes(&signing_key, tx_hash.as_bytes());
    let signature = hex::encode(signature_bytes);

    let payload = serde_json::json!({
        "tx_hash": tx_hash,
        "sender": sender_addr,
        "recipient": to,
        "signature": signature,
        "payload": {
            "amount": amount_u64,
            "fee": 1000, // 0.00001 OURO fee
            "public_key": sender_addr,
            "signature": signature, // Duplicate inside payload as expected by API
            "nonce": nonce
        }
    });

    match reqwest::Client::new()
        .post(&format!("{}/tx/submit", api))
        .json(&payload)
        .send()
        .await 
    {
        Ok(resp) if resp.status().is_success() => {
            println!("{}Transaction submitted successfully!{}", colors::GREEN, colors::RESET);
            println!("Tx Hash: {}", tx_hash);
        }
        Ok(resp) => {
            let err = resp.text().await.unwrap_or_default();
            eprintln!("{}Transaction failed: {}{}", colors::RED, err, colors::RESET);
        }
        Err(e) => {
            eprintln!("{}Network error: {}{}", colors::RED, e, colors::RESET);
        }
    }

    Ok(())
}

/// Resolve the ouro_py directory relative to the running executable.
/// Searches: next to the exe, one level up from the exe, and OURO_PY_DIR env override.
fn dirs_or_home() -> Option<std::path::PathBuf> {
    let home = if cfg!(windows) {
        std::env::var("USERPROFILE").ok()
    } else {
        std::env::var("HOME").ok()
    };
    home.map(std::path::PathBuf::from)
}

fn resolve_py_dir() -> std::io::Result<std::path::PathBuf> {
    // 1. Explicit override via environment variable
    if let Ok(dir) = std::env::var("OURO_PY_DIR") {
        let p = std::path::PathBuf::from(dir);
        if p.exists() {
            return Ok(p);
        }
    }

    // 2. Inside ~/.ouroboros/ouro_py (installed by join/install scripts)
    if let Some(home) = dirs_or_home() {
        let home_py = home.join(".ouroboros").join("ouro_py");
        if home_py.exists() {
            return Ok(home_py);
        }
    }

    // 3. Relative to the executable (works for portable installs)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            let sibling = exe_dir.join("ouro_py");
            if sibling.exists() {
                return Ok(sibling);
            }
            if let Some(parent) = exe_dir.parent() {
                let parent_sibling = parent.join("ouro_py");
                if parent_sibling.exists() {
                    return Ok(parent_sibling);
                }
            }
        }
    }

    // 4. Relative to current working directory (dev fallback)
    let cwd_relative = std::path::PathBuf::from("ouro_py");
    if cwd_relative.exists() {
        return Ok(cwd_relative);
    }
    let cwd_parent = std::path::PathBuf::from("../ouro_py");
    if cwd_parent.exists() {
        return Ok(cwd_parent);
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "Cannot find ouro_py directory. Set OURO_PY_DIR environment variable or place ouro_py next to the ouro binary.",
    ))
}

/// Hands off execution to the Python implementation for Medium/Light nodes
async fn handoff_to_python_node(
    role: crate::config_manager::NodeRole,
    command: &Commands,
) -> std::io::Result<()> {
    use std::process::Command;
    use cli_dashboard::colors;

    let role_str = match role {
        crate::config_manager::NodeRole::Medium => "medium",
        crate::config_manager::NodeRole::Light => "light",
        _ => "heavy",
    };

    println!(
        "\n{}>>> HANDOFF: Preparing Python {} Node environment...{} ",
        colors::CYAN,
        role_str.to_uppercase(),
        colors::RESET
    );

    let py_dir = resolve_py_dir()?;
    println!("  Python dir: {}", py_dir.display());

    let venv_dir = py_dir.join(".venv");

    #[cfg(windows)]
    let python_exe = venv_dir.join("Scripts").join("python.exe");
    #[cfg(not(windows))]
    let python_exe = venv_dir.join("bin").join("python");

    // 1. Ensure venv exists
    if !venv_dir.exists() {
        println!(" {}Creating virtual environment...{}", colors::DIM, colors::RESET);
        let status = Command::new("python")
            .args(["-m", "venv"])
            .arg(&venv_dir)
            .status()?;

        if !status.success() {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, "Failed to create Python venv. Ensure Python 3 is installed."));
        }
    }

    // 2. Ensure dependencies are installed
    println!(" {}Checking dependencies...{}", colors::DIM, colors::RESET);
    let req_file = py_dir.join("requirements.txt");
    if req_file.exists() {
        let status = Command::new(&python_exe)
            .args(["-m", "pip", "install", "-q", "-r"])
            .arg(&req_file)
            .status()?;

        if !status.success() {
            eprintln!("{}Warning: Failed to install some Python dependencies. Node may still work.{}", colors::YELLOW, colors::RESET);
        }
    }

    println!(
        "{}>>> STARTING: Python {} Node implementation...{} ",
        colors::CYAN,
        role_str.to_uppercase(),
        colors::RESET
    );

    // Determine which Python script to run
    let script_path = if role == crate::config_manager::NodeRole::Medium {
        py_dir.join("ouro_medium").join("main.py")
    } else {
        py_dir.join("ouro_light").join("main.py")
    };

    if !script_path.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Python node script not found: {}", script_path.display()),
        ));
    }

    // Prepare the command
    let mut child = Command::new(&python_exe);
    child.arg(&script_path);

    // Forward relevant environment variables
    if let Commands::Join { peer, .. } = command {
        if let Some(p) = peer {
            child.env("PEER_ADDR", p);
        }
    }

    // Forward API keys so Python node can enforce auth
    if let Ok(api_keys) = std::env::var("API_KEYS") {
        child.env("API_KEYS", api_keys);
    }

    // Forward node identity
    {
        let config = crate::config_manager::CONFIG.read().await;
        child.env("NODE_ID", &config.identity.node_id);
        if !config.security.api_keys.is_empty() {
            child.env("API_KEYS", config.security.api_keys.join(","));
        }
    }

    child.env("NODE_ROLE", role_str);

    // Set consistent API ports for Python tiers
    let api_port = if role == crate::config_manager::NodeRole::Medium { "8001" } else { "8002" };
    child.env("API_PORT", api_port);

    // Run the Python process and pipe its output to our terminal
    let status = child
        .spawn()?
        .wait()?;

    if !status.success() {
        eprintln!(
            "\n{}ERROR: Python node exited with code {}{}",
            colors::RED,
            status.code().unwrap_or(-1),
            colors::RESET
        );
    }

    Ok(())
}

async fn handle_roles_command() -> std::io::Result<()> {
    use cli_dashboard::colors;

    println!("\n{}{}OUROBOROS NODE TIERS & ROLES{}", colors::BOLD, colors::CYAN, colors::RESET);
    println!("{}", cli_dashboard::horizontal_line(70));
    println!("Ouroboros uses a tiered architecture to balance security and scale.");
    println!("Choose the role that best matches your hardware and goals.\n");

    println!("{}1. HEAVY (Validator / Settlement Node){}", colors::BOLD, colors::RESET);
    println!("   {}Language:{} Rust", colors::DIM, colors::RESET);
    println!("   {}Reward:  {} 100% (1.0x multiplier)", colors::GREEN, colors::RESET);
    println!("   {}Duties:  {} BFT Consensus, Global Finality, Fraud Adjudication.", colors::DIM, colors::RESET);
    println!("   {}Hardware:{} 8+ CPU cores, 16GB+ RAM, 1TB+ SSD, 1Gbps Fiber.", colors::DIM, colors::RESET);
    println!("   {}Use Case:{} For institutional stakers and core security providers.\n", colors::DIM, colors::RESET);

    println!("{}2. MEDIUM (Subchain Aggregator / Shadow Hub){}", colors::BOLD, colors::RESET);
    println!("   {}Language:{} Python/Rust", colors::DIM, colors::RESET);
    println!("   {}Reward:  {} 50% (0.5x multiplier) + Aggregation Fees", colors::GREEN, colors::RESET);
    println!("   {}Duties:  {} Batching microchains, Ordering, Shadow Settlement.", colors::DIM, colors::RESET);
    println!("   {}Hardware:{} 4+ CPU cores, 8GB RAM, 500GB SSD, stable connection.", colors::DIM, colors::RESET);
    println!("   {}Use Case:{} For community infrastructure and app developers.\n", colors::DIM, colors::RESET);

    println!("{}3. LIGHT (App Node / Surveillance Watchdog){}", colors::BOLD, colors::RESET);
    println!("   {}Language:{} Python", colors::DIM, colors::RESET);
    println!("   {}Reward:  {} 10% (0.1x multiplier) + Fraud Bounties", colors::GREEN, colors::RESET);
    println!("   {}Duties:  {} Running App-WASM, Verifying Anchors, Catching Fraud.", colors::DIM, colors::RESET);
    println!("   {}Hardware:{} Any modern laptop, phone, or Raspberry Pi.", colors::DIM, colors::RESET);
    println!("   {}Use Case:{} For everyday users, gamers, and privacy advocates.\n", colors::DIM, colors::RESET);

    println!("{}", cli_dashboard::horizontal_line(70));
    println!("To start with a specific role:");
    println!("  {}ouro start --role <heavy|medium|light>{}", colors::YELLOW, colors::RESET);
    println!();

    Ok(())
}
