// src/tor.rs
// Tor support for hybrid clearnet + darkweb operation

use anyhow::{Context, Result};
use tokio::net::TcpStream;
use tokio_socks::tcp::Socks5Stream;

/// Tor configuration
#[derive(Debug, Clone)]
pub struct TorConfig {
    /// Enable Tor support
    pub enabled: bool,
    /// Tor SOCKS proxy address (default: 127.0.0.1:9050)
    pub proxy_addr: String,
    /// Optional: This node's Tor hidden service address
    pub hidden_service: Option<String>,
}

impl TorConfig {
    /// Load Tor configuration from environment variables
    ///
    /// Environment variables:
    /// - ENABLE_TOR: Set to enable Tor support
    /// - TOR_PROXY: Tor SOCKS proxy address (default: 127.0.0.1:9050)
    /// - TOR_HIDDEN_SERVICE: Your .onion address (e.g., abc123.onion:9001)
    pub fn from_env() -> Self {
        Self {
            enabled: std::env::var("ENABLE_TOR").is_ok(),
            proxy_addr: std::env::var("TOR_PROXY").unwrap_or_else(|_| "127.0.0.1:9050".to_string()),
            hidden_service: std::env::var("TOR_HIDDEN_SERVICE").ok(),
        }
    }

    /// Check if Tor is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Get the Tor SOCKS proxy address
    pub fn proxy_addr(&self) -> &str {
        &self.proxy_addr
    }

    /// Get this node's hidden service address, if configured
    pub fn hidden_service(&self) -> Option<&str> {
        self.hidden_service.as_deref()
    }
}

impl Default for TorConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            proxy_addr: "127.0.0.1:9050".to_string(),
            hidden_service: None,
        }
    }
}

/// Check if an address is a Tor .onion address
///
/// # Examples
/// ```
/// use ouro_dag::tor::is_onion_address;
/// assert!(is_onion_address("abc123def456.onion:9001"));
/// assert!(!is_onion_address("192.168.1.1:9001"));
/// assert!(!is_onion_address("example.com:9001"));
/// ```
pub fn is_onion_address(addr: &str) -> bool {
    addr.contains(".onion")
}

/// Connect to a peer, automatically detecting Tor vs clearnet
///
/// If the address is a .onion address, connect via Tor SOCKS proxy.
/// Otherwise, connect directly via clearnet.
///
/// # Arguments
/// * `addr` - Peer address (either clearnet or .onion)
/// * `tor_config` - Tor configuration
///
/// # Returns
/// TCP stream connected to the peer
pub async fn connect_to_peer(addr: &str, tor_config: &TorConfig) -> Result<TcpStream> {
    if is_onion_address(addr) {
        // Darkweb connection via Tor SOCKS proxy
        if !tor_config.is_enabled() {
            anyhow::bail!(
                "Attempted to connect to .onion address {} but Tor is not enabled. \
 Set ENABLE_TOR=true and TOR_PROXY environment variables.",
                addr
            );
        }
        connect_via_tor(addr, tor_config.proxy_addr()).await
    } else {
        // Normal clearnet connection
        TcpStream::connect(addr)
            .await
            .with_context(|| format!("Failed to connect to clearnet peer {}", addr))
    }
}

/// Connect to a .onion address via Tor SOCKS proxy
///
/// # Arguments
/// * `onion_addr` - The .onion address to connect to (e.g., "abc123.onion:9001")
/// * `proxy_addr` - Tor SOCKS proxy address (e.g., "127.0.0.1:9050")
///
/// # Returns
/// TCP stream connected through Tor
async fn connect_via_tor(onion_addr: &str, proxy_addr: &str) -> Result<TcpStream> {
    tracing::debug!(
        "TOR Connecting to {} via Tor proxy {}",
        onion_addr,
        proxy_addr
    );

    let stream = Socks5Stream::connect(proxy_addr, onion_addr)
        .await
        .with_context(|| {
            format!(
                "Failed to connect via Tor to {}. \
 Is Tor daemon running on {}? \
 Is the .onion address correct?",
                onion_addr, proxy_addr
            )
        })?;

    tracing::info!("TOR Connected to {} via Tor", onion_addr);
    Ok(stream.into_inner())
}

/// Test Tor proxy connectivity
///
/// Attempts to connect to a known .onion address to verify Tor is working.
///
/// # Arguments
/// * `proxy_addr` - Tor SOCKS proxy address to test
///
/// # Returns
/// Ok if Tor proxy is working, Err otherwise
pub async fn test_tor_proxy(proxy_addr: &str) -> Result<()> {
    tracing::debug!("Testing Tor proxy at {}", proxy_addr);

    // Try to connect to Tor Project's check service
    // Note: This is clearnet, not .onion, but tests SOCKS proxy functionality
    let test_addr = "check.torproject.org:80";

    match tokio::time::timeout(
        std::time::Duration::from_secs(30),
        Socks5Stream::connect(proxy_addr, test_addr),
    )
    .await
    {
        Ok(Ok(_stream)) => {
            tracing::info!(" Tor proxy {} is working", proxy_addr);
            Ok(())
        }
        Ok(Err(e)) => {
            anyhow::bail!(
                "ERROR Tor proxy {} connection failed: {}. \
 Is Tor daemon running? Try: systemctl status tor",
                proxy_addr,
                e
            )
        }
        Err(_) => {
            anyhow::bail!(
                "ERROR Tor proxy {} connection timed out after 30s. \
 Is Tor daemon running and responsive?",
                proxy_addr
            )
        }
    }
}

/// Validate a .onion address format
///
/// Tor v3 addresses are 56 characters (before .onion suffix)
/// Tor v2 addresses are 16 characters (deprecated but still check)
pub fn validate_onion_address(addr: &str) -> Result<()> {
    if !addr.contains(".onion") {
        anyhow::bail!("Address {} is not a .onion address", addr);
    }

    // Extract the hash part (before .onion:port)
    let parts: Vec<&str> = addr.split('.').collect();
    if parts.len() < 2 {
        anyhow::bail!("Invalid .onion address format: {}", addr);
    }

    let hash = parts[0];
    let hash_len = hash.len();

    // Tor v3: 56 characters (base32 encoded)
    // Tor v2: 16 characters (deprecated)
    if hash_len != 56 && hash_len != 16 {
        anyhow::bail!(
            "Invalid .onion address length: {} chars. Expected 56 (v3) or 16 (v2). Address: {}",
            hash_len,
            addr
        );
    }

    // Check if hash contains only valid base32 characters
    let valid_chars = "abcdefghijklmnopqrstuvwxyz234567";
    if !hash.chars().all(|c| valid_chars.contains(c)) {
        anyhow::bail!("Invalid characters in .onion address: {}", addr);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_onion_address() {
        assert!(is_onion_address("abc123def456.onion:9001"));
        assert!(is_onion_address("test.onion:8080"));
        assert!(!is_onion_address("192.168.1.1:9001"));
        assert!(!is_onion_address("example.com:9001"));
    }

    #[test]
    fn test_validate_onion_v3() {
        // Valid v3 address (56 chars)
        let valid_v3 = "abcdefgh2345abcdefgh2345abcdefgh2345abcdefgh2345abcdefgh.onion:9001";
        assert!(validate_onion_address(valid_v3).is_ok());
    }

    #[test]
    fn test_validate_onion_v2() {
        // Valid v2 address (16 chars)
        let valid_v2 = "3g2upl4pq6kufc4m.onion:8080";
        assert!(validate_onion_address(valid_v2).is_ok());
    }

    #[test]
    fn test_validate_onion_invalid() {
        // Too short
        assert!(validate_onion_address("short.onion:9001").is_err());

        // Not a .onion
        assert!(validate_onion_address("example.com:9001").is_err());

        // Invalid characters
        assert!(validate_onion_address("ABC123!@#.onion:9001").is_err());
    }

    #[test]
    fn test_tor_config_from_env() {
        // Default config when no env vars set
        std::env::remove_var("ENABLE_TOR");
        std::env::remove_var("TOR_PROXY");
        std::env::remove_var("TOR_HIDDEN_SERVICE");

        let config = TorConfig::from_env();
        assert!(!config.is_enabled());
        assert_eq!(config.proxy_addr(), "127.0.0.1:9050");
        assert!(config.hidden_service().is_none());
    }

    #[test]
    fn test_tor_config_enabled() {
        // Test with direct struct construction to avoid env var race conditions
        let config = TorConfig {
            enabled: true,
            proxy_addr: "127.0.0.1:9150".to_string(),
            hidden_service: Some("test.onion:9001".to_string()),
        };
        assert!(config.is_enabled());
        assert_eq!(config.proxy_addr(), "127.0.0.1:9150");
        assert_eq!(config.hidden_service(), Some("test.onion:9001"));
    }
}
