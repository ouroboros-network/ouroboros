// src/config.rs
// Configuration validation and secure key management

use log::{error, info, warn};
use std::env;

/// Validation result for configuration checks
pub struct ConfigValidation {
    pub valid: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

impl ConfigValidation {
    fn new() -> Self {
        Self {
            valid: true,
            warnings: Vec::new(),
            errors: Vec::new(),
        }
    }

    fn add_warning(&mut self, msg: String) {
        self.warnings.push(msg);
    }

    fn add_error(&mut self, msg: String) {
        self.errors.push(msg);
        self.valid = false;
    }

    pub fn print_summary(&self) {
        if !self.warnings.is_empty() {
            warn!("WARNING Configuration Warnings:");
            for w in &self.warnings {
                warn!(" - {}", w);
            }
        }

        if !self.errors.is_empty() {
            error!("ERROR Configuration Errors:");
            for e in &self.errors {
                error!(" - {}", e);
            }
        }

        if self.valid && self.warnings.is_empty() {
            info!(" Configuration validation passed");
        }
    }
}

/// Validate all critical configuration at startup
pub fn validate_config() -> ConfigValidation {
    let mut validation = ConfigValidation::new();

    info!("DEBUG: Validating configuration...");

    // Validate RocksDB path
    validate_rocksdb_path(&mut validation);

    // Validate API_KEYS (security)
    validate_api_keys(&mut validation);

    // Validate TLS configuration (optional but if set, must be valid)
    validate_tls_config(&mut validation);

    // Validate addresses
    validate_addresses(&mut validation);

    // Check for insecure configurations
    check_insecure_configs(&mut validation);

    validation
}

fn validate_rocksdb_path(validation: &mut ConfigValidation) {
    let rocksdb_path = env::var("ROCKSDB_PATH").unwrap_or_else(|_| "./data".into());

    info!("RocksDB: Using data path: {}", rocksdb_path);

    // Check if path exists or can be created
    let path = std::path::Path::new(&rocksdb_path);
    if !path.exists() {
        if let Err(e) = std::fs::create_dir_all(path) {
            validation.add_error(format!(
                "Cannot create RocksDB directory '{}': {}",
                rocksdb_path, e
            ));
        } else {
            info!("Created RocksDB directory: {}", rocksdb_path);
        }
    } else {
        info!(" RocksDB path exists: {}", rocksdb_path);
    }
}

fn validate_api_keys(validation: &mut ConfigValidation) {
    match env::var("API_KEYS") {
        Ok(keys) if !keys.is_empty() => {
            let key_list: Vec<&str> = keys.split(',').map(|k| k.trim()).collect();

            if key_list.is_empty() {
                validation.add_error("API_KEYS is set but contains no valid keys".into());
                return;
            }

            info!(" API authentication enabled ({} key(s))", key_list.len());

            // Validate each key
            for (i, key) in key_list.iter().enumerate() {
                if key.len() < 32 {
                    validation.add_warning(format!(
                        "API key #{} is too short ({} chars) - recommend at least 32 characters",
                        i + 1,
                        key.len()
                    ));
                }

                // Check for obviously insecure keys
                if key.to_lowercase() == "password"
                    || key.to_lowercase() == "secret"
                    || *key == "12345"
                    || *key == "test"
                {
                    validation.add_error(format!(
                        "API key #{} is insecure (common/weak value) - MUST change for production!",
                        i + 1
                    ));
                }
            }
        }
        _ => {
            validation
                .add_warning("API_KEYS not set - Node will run without API authentication".into());
        }
    }
}

fn validate_tls_config(validation: &mut ConfigValidation) {
    // API TLS validation
    let cert_path = env::var("TLS_CERT_PATH").ok();
    let key_path = env::var("TLS_KEY_PATH").ok();

    match (cert_path.clone(), key_path.clone()) {
        (Some(cert), Some(key)) => {
            info!(" API TLS configuration detected");

            // Check if files exist
            if !std::path::Path::new(&cert).exists() {
                validation.add_error(format!(
                    "TLS_CERT_PATH points to non-existent file: {}",
                    cert
                ));
            }

            if !std::path::Path::new(&key).exists() {
                validation.add_error(format!("TLS_KEY_PATH points to non-existent file: {}", key));
            }
        }
        (Some(_), None) => {
            validation.add_warning(
                "TLS_CERT_PATH set but TLS_KEY_PATH missing - TLS will be disabled".into(),
            );
        }
        (None, Some(_)) => {
            validation.add_warning(
                "TLS_KEY_PATH set but TLS_CERT_PATH missing - TLS will be disabled".into(),
            );
        }
        (None, None) => {
            validation
                .add_warning("TLS not configured - API will run over HTTP (unencrypted)".into());
        }
    }

    // P2P TLS validation (can use same certs as API or dedicated P2P certs)
    let p2p_cert = env::var("P2P_TLS_CERT_PATH").ok().or(cert_path);
    let p2p_key = env::var("P2P_TLS_KEY_PATH").ok().or(key_path);

    match (p2p_cert, p2p_key) {
        (Some(cert), Some(key)) => {
            info!(" P2P TLS configuration detected");

            // Check if files exist
            if !std::path::Path::new(&cert).exists() {
                validation.add_error(format!(
                    "P2P TLS cert points to non-existent file: {}",
                    cert
                ));
            }

            if !std::path::Path::new(&key).exists() {
                validation.add_error(format!("P2P TLS key points to non-existent file: {}", key));
            }
        }
        _ => {
            validation
                .add_warning("P2P TLS not configured - peer connections will use plain TCP".into());
        }
    }
}

fn validate_addresses(validation: &mut ConfigValidation) {
    // Validate API_ADDR
    let api_addr = env::var("API_ADDR").unwrap_or_else(|_| "0.0.0.0:8000".into());
    if let Err(_) = api_addr.parse::<std::net::SocketAddr>() {
        validation.add_error(format!(
            "API_ADDR has invalid format: '{}' (expected IP:PORT)",
            api_addr
        ));
    }

    // Validate LISTEN_ADDR (P2P)
    let listen_addr = env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:9000".into());
    if let Err(_) = listen_addr.parse::<std::net::SocketAddr>() {
        validation.add_error(format!(
            "LISTEN_ADDR has invalid format: '{}' (expected IP:PORT)",
            listen_addr
        ));
    }
}

fn check_insecure_configs(validation: &mut ConfigValidation) {
    // Check if running in production-like environment
    let is_production = env::var("ENVIRONMENT")
        .map(|e| e.to_lowercase() == "production" || e.to_lowercase() == "prod")
        .unwrap_or(false);

    if is_production {
        info!(" Production environment detected - enforcing stricter validation");

        // In production, certain things are errors not warnings
        if env::var("API_KEYS").is_err() {
            validation.add_error("Production deployment MUST have API_KEYS configured!".into());
        }

        if env::var("TLS_CERT_PATH").is_err() {
            validation.add_error("Production deployment MUST use TLS/HTTPS!".into());
        }
    }

    // Check rate limiting configuration
    if let Ok(max_req) = env::var("RATE_LIMIT_MAX_REQUESTS") {
        if let Ok(limit) = max_req.parse::<u32>() {
            if limit > 10000 {
                validation.add_warning(format!(
                    "RATE_LIMIT_MAX_REQUESTS is very high ({}) - may not prevent DoS effectively",
                    limit
                ));
            }
        }
    }
}

/// Generate a secure random API key (for admin use)
pub fn generate_api_key() -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();

    (0..64)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

// ============================================================================
// TESTNET CONFIGURATION
// ============================================================================

use serde::{Deserialize, Serialize};

/// Network type (testnet vs mainnet)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum NetworkType {
    Testnet,
    Mainnet,
}

/// Oracle testnet configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleTestnetConfig {
    pub network: NetworkConfig,
    pub oracle_system: OracleSystemConfig,
    pub guardian_council: GuardianCouncilConfig,
    pub free_data_feeds: DataFeedsConfig,
    pub testnet_faucet: Option<FaucetConfig>,
    pub endpoints: EndpointsConfig,
    pub monitoring: MonitoringConfig,
    pub security: SecurityConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub chain_id: String,
    pub network_name: String,
    pub is_testnet: bool,
    pub genesis_time: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleSystemConfig {
    pub min_stake_required: u64,
    pub update_interval_ms: u64,
    pub max_price_deviation_percent: f64,
    pub heartbeat_interval_ms: u64,
    pub slashing_threshold: u32,
    pub reputation_decay_rate: f64,
    pub min_sources_for_aggregation: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardianCouncilConfig {
    pub members: Vec<String>,
    pub threshold: usize,
    pub timelock_delay_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataFeedsConfig {
    pub crypto: CryptoFeedConfig,
    pub weather: WeatherFeedConfig,
    pub stocks: StocksFeedConfig,
    pub news: NewsFeedConfig,
    pub random: RandomFeedConfig,
    pub government: GovernmentFeedConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CryptoFeedConfig {
    pub enabled: bool,
    pub sources: Vec<String>,
    pub symbols: Vec<String>,
    pub refresh_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherFeedConfig {
    pub enabled: bool,
    pub source: String,
    pub default_locations: Vec<LocationConfig>,
    pub refresh_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocationConfig {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StocksFeedConfig {
    pub enabled: bool,
    pub source: String,
    pub symbols: Vec<String>,
    pub refresh_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsFeedConfig {
    pub enabled: bool,
    pub sources: Vec<String>,
    pub subreddits: Vec<String>,
    pub refresh_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RandomFeedConfig {
    pub enabled: bool,
    pub source: String,
    pub refresh_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernmentFeedConfig {
    pub enabled: bool,
    pub nasa_api_key: String,
    pub refresh_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaucetConfig {
    pub enabled: bool,
    pub amount_per_request: u64,
    pub cooldown_seconds: u64,
    pub max_daily_claims: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointsConfig {
    pub oracle_api: String,
    pub node_api: String,
    pub p2p_port: u16,
    pub bft_port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    pub prometheus_enabled: bool,
    pub prometheus_port: u16,
    pub grafana_enabled: bool,
    pub log_level: String,
    pub enable_tracing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    pub rate_limit_enabled: bool,
    pub max_requests_per_minute: u32,
    pub require_api_key: bool,
    pub tls_enabled: bool,
}

impl OracleTestnetConfig {
    /// Load configuration from JSON file
    pub fn load_from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let config: OracleTestnetConfig = serde_json::from_str(&content)?;
        Ok(config)
    }

    /// Get default testnet configuration
    pub fn default_testnet() -> Self {
        Self {
            network: NetworkConfig {
                chain_id: "ouroboros-testnet-1".to_string(),
                network_name: "Ouroboros Testnet".to_string(),
                is_testnet: true,
                genesis_time: chrono::Utc::now().to_rfc3339(),
            },
            oracle_system: OracleSystemConfig {
                min_stake_required: 5_000_000_000_000, // 5,000 OURO
                update_interval_ms: 5000,
                max_price_deviation_percent: 5.0,
                heartbeat_interval_ms: 30000,
                slashing_threshold: 3,
                reputation_decay_rate: 0.995,
                min_sources_for_aggregation: 3,
            },
            guardian_council: GuardianCouncilConfig {
                members: vec![
                    "testnet_guardian_1".to_string(),
                    "testnet_guardian_2".to_string(),
                    "testnet_guardian_3".to_string(),
                    "testnet_guardian_4".to_string(),
                    "testnet_guardian_5".to_string(),
                ],
                threshold: 3,
                timelock_delay_secs: 86400, // 1 day for testnet
            },
            free_data_feeds: DataFeedsConfig {
                crypto: CryptoFeedConfig {
                    enabled: true,
                    sources: vec!["coingecko".into(), "binance".into(), "coinbase".into()],
                    symbols: vec!["BTC".into(), "ETH".into(), "OURO".into()],
                    refresh_ms: 5000,
                },
                weather: WeatherFeedConfig {
                    enabled: true,
                    source: "open-meteo".to_string(),
                    default_locations: vec![LocationConfig {
                        name: "New York".into(),
                        lat: 40.7128,
                        lon: -74.0060,
                    }],
                    refresh_ms: 300000,
                },
                stocks: StocksFeedConfig {
                    enabled: true,
                    source: "yahoo-finance".to_string(),
                    symbols: vec!["AAPL".into(), "GOOGL".into()],
                    refresh_ms: 60000,
                },
                news: NewsFeedConfig {
                    enabled: true,
                    sources: vec!["hackernews".into(), "reddit".into()],
                    subreddits: vec!["cryptocurrency".into()],
                    refresh_ms: 300000,
                },
                random: RandomFeedConfig {
                    enabled: true,
                    source: "random-org".to_string(),
                    refresh_ms: 60000,
                },
                government: GovernmentFeedConfig {
                    enabled: true,
                    nasa_api_key: "DEMO_KEY".to_string(),
                    refresh_ms: 3600000,
                },
            },
            testnet_faucet: Some(FaucetConfig {
                enabled: true,
                amount_per_request: 100_000_000_000, // 1,000 OURO
                cooldown_seconds: 86400,
                max_daily_claims: 10,
            }),
            endpoints: EndpointsConfig {
                oracle_api: "http://localhost:8081".to_string(),
                node_api: "http://localhost:8001".to_string(),
                p2p_port: 9001,
                bft_port: 9091,
            },
            monitoring: MonitoringConfig {
                prometheus_enabled: true,
                prometheus_port: 9090,
                grafana_enabled: true,
                log_level: "info".to_string(),
                enable_tracing: true,
            },
            security: SecurityConfig {
                rate_limit_enabled: true,
                max_requests_per_minute: 100,
                require_api_key: false,
                tls_enabled: false,
            },
        }
    }

    /// Validate testnet configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.guardian_council.members.len() < 3 {
            return Err("Guardian council must have at least 3 members".into());
        }
        if self.guardian_council.threshold > self.guardian_council.members.len() {
            return Err("Guardian threshold cannot exceed member count".into());
        }
        Ok(())
    }
}

/// Check if running in testnet mode
pub fn is_testnet() -> bool {
    env::var("CHAIN_ID")
        .map(|id| id.contains("testnet"))
        .unwrap_or(false)
        || env::var("TEST_MODE").map(|v| v == "true").unwrap_or(false)
}
