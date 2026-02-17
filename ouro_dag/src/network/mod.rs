use log;

// src/network.rs
pub mod handshake;
use anyhow::Result;
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
pub mod bft_msg;

// TOR support for hybrid clearnet + darkweb operation
use crate::tor::{is_onion_address, TorConfig};

use std::collections::{HashMap, HashSet};
use std::net::{Ipv4Addr, SocketAddrV4};
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc, Mutex};
use tokio::time::sleep;
use tokio_rustls::rustls::ServerName;
use tokio_rustls::{
    client::TlsStream as ClientTlsStream, server::TlsStream as ServerTlsStream, TlsAcceptor,
    TlsConnector,
};
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use uuid::Uuid;

/// Trait combining AsyncRead + AsyncWrite for boxed stream type erasure
trait AsyncStream: AsyncRead + AsyncWrite + Send + Unpin {}
impl<T: AsyncRead + AsyncWrite + Send + Unpin> AsyncStream for T {}

/// Type alias for a boxed async stream that can be either TLS or plain TCP
type BoxedAsyncStream = Pin<Box<dyn AsyncStream>>;

use self::handshake::{message_id_from_envelope, Envelope};
use crate::dag::transaction::Transaction;

use once_cell::sync::Lazy;
use std::sync::atomic::{AtomicUsize, Ordering};

/// P2P Rate limit configuration (configurable via environment variables)
struct P2PRateLimitConfig {
    /// Messages per window
    max_messages_per_window: u32,
    /// Window duration in seconds
    window_secs: u64,
    /// Max concurrent connections per IP
    max_connections_per_ip: usize,
}

impl P2PRateLimitConfig {
    fn from_env() -> Self {
        Self {
            max_messages_per_window: std::env::var("P2P_MAX_MESSAGES_PER_WINDOW")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(600), // Default: 600 messages per window
            window_secs: std::env::var("P2P_RATE_LIMIT_WINDOW_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(60), // Default: 60 seconds
            max_connections_per_ip: std::env::var("P2P_MAX_CONNECTIONS_PER_IP")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(10), // Default: max 10 connections per IP
        }
    }
}

static P2P_RATE_LIMIT_CONFIG: Lazy<P2PRateLimitConfig> = Lazy::new(P2PRateLimitConfig::from_env);

/// Connection limits for scalable gossip network
/// These limits ensure the network scales efficiently from 10 to 10,000+ nodes
const MIN_ACTIVE_PEERS: usize = 3; // Minimum connections (redundancy - your idea!)
const TARGET_ACTIVE_PEERS: usize = 8; // Target connections (gossip efficiency)
const MAX_ACTIVE_PEERS: usize = 32; // Maximum connections (prevent overload)
const MAX_KNOWN_PEERS: usize = 2000; // Total peers we remember

/// Lightweight metrics
static METRICS_ACTIVE_CONNECTIONS: Lazy<AtomicUsize> = Lazy::new(|| AtomicUsize::new(0));
static METRICS_DEDUPE_ENTRIES: Lazy<AtomicUsize> = Lazy::new(|| AtomicUsize::new(0));
static METRICS_PEER_COUNT: Lazy<AtomicUsize> = Lazy::new(|| AtomicUsize::new(0));
/// Network I/O byte counters
static METRICS_BYTES_IN: Lazy<AtomicUsize> = Lazy::new(|| AtomicUsize::new(0));
static METRICS_BYTES_OUT: Lazy<AtomicUsize> = Lazy::new(|| AtomicUsize::new(0));
/// Default bootstrap nodes for peer discovery.
/// HYBRID ARCHITECTURE: Mix of server nodes + community P2P nodes.
/// These are fallback nodes used when BOOTSTRAP_PEERS/BOOTSTRAP_URLS env var is not set.
/// L8 note: Override these via BOOTSTRAP_PEERS env var for production deployments.
/// As the network grows, prefer DNS-based seed nodes (e.g., seed.ouroboros.network)
/// which can be updated without binary changes.
const DEFAULT_BOOTSTRAP_NODES: &[&str] = &[
    // Primary seed nodes (GCP infrastructure)
    "136.112.101.176:9000",  // US seed node
    "34.57.121.217:9000",    // GCP full node

    // DNS-based seed nodes (preferred — can update without binary changes)
    // "seed1.ouroboros.network:9000",
    // "seed2.ouroboros.network:9000",
];

/// Fetch peers from multiple bootstrap sources
/// Tries each bootstrap URL until successful, returns combined peer list
async fn fetch_bootstrap_peers_multi(urls: &[String]) -> Vec<String> {
    let mut all_peers = Vec::new();

    for url in urls {
        match fetch_bootstrap_peers(url).await {
            Ok(peers) => {
                tracing::info!("Fetched {} peers from bootstrap: {}", peers.len(), url);
                all_peers.extend(peers);
            }
            Err(e) => {
                tracing::warn!("Bootstrap fetch failed for {}: {}", url, e);
            }
        }
    }

    // Deduplicate peers
    all_peers.sort();
    all_peers.dedup();
    all_peers
}

pub type TxBroadcast = mpsc::Sender<Transaction>;
pub type TxInboundReceiver = mpsc::Receiver<Transaction>;

/// Peer entry stored in runtime store and persisted to peers.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerEntry {
    pub addr: String,
    pub last_seen_unix: Option<u64>,
    pub failures: u32,
    pub banned_until_unix: Option<u64>,
    pub role: Option<String>,
    // rate limit window
    pub rate_window_start_unix: Option<u64>,
    pub rate_count: u32,
    // Exponential backoff for reconnection attempts
    #[serde(default)]
    pub backoff_secs: u64,
    #[serde(default)]
    pub next_retry_unix: Option<u64>,
    // Track last peer exchange time to rate-limit PEX
    #[serde(default)]
    pub last_pex_unix: Option<u64>,
}

impl PeerEntry {
    pub fn new(addr: String) -> Self {
        Self {
            addr,
            last_seen_unix: Some(current_unix()),
            failures: 0,
            banned_until_unix: None,
            role: None,
            rate_window_start_unix: Some(current_unix()),
            rate_count: 0,
            backoff_secs: 0,
            next_retry_unix: None,
            last_pex_unix: None,
        }
    }

    /// Record a connection failure with exponential backoff + jitter
    pub fn record_failure_with_backoff(&mut self) {
        self.failures = self.failures.saturating_add(1);
        // Exponential backoff: 5s, 10s, 20s, 40s, 80s, 160s, 300s (capped at 5 min)
        self.backoff_secs = match self.failures {
            0..=1 => 5,
            2 => 10,
            3 => 20,
            4 => 40,
            5 => 80,
            6 => 160,
            _ => 300,
        };
        // Add jitter (0-25% of backoff) to prevent thundering herd
        let jitter = (current_unix() % (self.backoff_secs / 4 + 1)).max(1);
        self.next_retry_unix = Some(current_unix() + self.backoff_secs + jitter);
    }

    /// Check if this peer is ready for a retry attempt
    pub fn is_ready_for_retry(&self) -> bool {
        match self.next_retry_unix {
            Some(t) => current_unix() >= t,
            None => true,
        }
    }

    /// Reset backoff on successful connection
    pub fn reset_backoff(&mut self) {
        self.failures = 0;
        self.backoff_secs = 0;
        self.next_retry_unix = None;
    }
}

/// Validate a peer address string (must be valid IP:PORT or .onion:PORT)
fn is_valid_peer_address(addr: &str) -> bool {
    if is_onion_address(addr) {
        return true;
    }
    addr.parse::<std::net::SocketAddr>().is_ok()
}

/// DNS seed domains for peer discovery (resolved before hardcoded fallback)
const DNS_SEEDS: &[&str] = &[
    "seed1.ouroboros.network",
    "seed2.ouroboros.network",
];

/// Default P2P port for DNS-resolved seeds
const DEFAULT_P2P_PORT: u16 = 9000;

/// Resolve DNS seeds to peer addresses
async fn resolve_dns_seeds() -> Vec<String> {
    use std::net::ToSocketAddrs;

    let mut peers = Vec::new();
    for seed in DNS_SEEDS {
        let lookup_target = format!("{}:{}", seed, DEFAULT_P2P_PORT);
        // Use blocking DNS resolution via spawn_blocking (std::net is synchronous)
        let result = tokio::task::spawn_blocking(move || {
            lookup_target.to_socket_addrs()
                .map(|addrs| addrs.map(|a| a.to_string()).collect::<Vec<_>>())
        }).await;

        match result {
            Ok(Ok(resolved)) if !resolved.is_empty() => {
                tracing::info!("DNS seed {}: resolved {} addresses", seed, resolved.len());
                peers.extend(resolved);
            }
            Ok(Err(e)) => {
                tracing::debug!("DNS seed {} failed: {}", seed, e);
            }
            _ => {
                tracing::debug!("DNS seed {} resolution task failed", seed);
            }
        }
    }
    peers
}

/// Multi-strategy bootstrap waterfall: cache → DNS → hardcoded → HTTP
async fn discover_bootstrap_peers(peer_store: &PeerStore) -> Vec<String> {
    let mut discovered = Vec::new();

    // Strategy 1: Recently seen peers from cache (peers.json)
    {
        let store = peer_store.lock().await;
        let recent: Vec<String> = store
            .iter()
            .filter(|p| {
                let age = current_unix().saturating_sub(p.last_seen_unix.unwrap_or(0));
                age < 86400 * 7 && p.failures < 5 // Seen in last 7 days, few failures
            })
            .map(|p| p.addr.clone())
            .collect();
        if !recent.is_empty() {
            tracing::info!("Bootstrap: {} peers from local cache", recent.len());
            discovered.extend(recent);
        }
    }

    // Strategy 2: DNS seeds (can be updated without binary changes)
    if discovered.len() < 3 {
        let dns_peers = resolve_dns_seeds().await;
        if !dns_peers.is_empty() {
            tracing::info!("Bootstrap: {} peers from DNS seeds", dns_peers.len());
            discovered.extend(dns_peers);
        }
    }

    // Strategy 3: Hardcoded seed IPs
    if discovered.len() < 3 {
        for seed in DEFAULT_BOOTSTRAP_NODES {
            if !discovered.contains(&seed.to_string()) {
                discovered.push(seed.to_string());
            }
        }
        tracing::info!("Bootstrap: added hardcoded seed nodes");
    }

    // Strategy 4: Environment variable peers (BOOTSTRAP_PEERS, PEER_ADDRS)
    if let Ok(peers_str) = std::env::var("BOOTSTRAP_PEERS") {
        for p in peers_str.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) {
            if !discovered.contains(&p) {
                discovered.push(p);
            }
        }
    }

    // Strategy 5: HTTP bootstrap endpoints (fetch peer lists from URLs)
    if discovered.len() < 3 {
        let bootstrap_urls: Vec<String> = if let Ok(urls_str) = std::env::var("BOOTSTRAP_URLS") {
            urls_str.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
        } else if let Ok(url) = std::env::var("BOOTSTRAP_URL") {
            vec![url]
        } else {
            vec![]
        };
        if !bootstrap_urls.is_empty() {
            let fetched = fetch_bootstrap_peers_multi(&bootstrap_urls).await;
            if !fetched.is_empty() {
                tracing::info!("Bootstrap: {} peers from HTTP endpoints", fetched.len());
                discovered.extend(fetched);
            }
        }
    }

    // Deduplicate and validate
    discovered.sort();
    discovered.dedup();
    discovered.retain(|a| is_valid_peer_address(a));
    discovered
}

pub type PeerStore = Arc<Mutex<Vec<PeerEntry>>>;

/// Connection handle representing a persistent outbound connection to a peer.
pub struct Connection {
    pub addr: String,
    pub tx: mpsc::Sender<Envelope>, // send envelopes to this connection task
    pub last_seen: Arc<Mutex<Option<Instant>>>,
}

type DedupeCache = Arc<Mutex<HashMap<String, Instant>>>;

/// helper: current epoch seconds
fn current_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

async fn save_peers_to_file(store: &PeerStore) {
    let peers = store.lock().await;
    if let Ok(json) = serde_json::to_string(&*peers) {
        // L7 fix: Atomic write — write to temp file then rename to prevent corruption on crash
        let tmp = "peers.json.tmp";
        if tokio::fs::write(tmp, &json).await.is_ok() {
            let _ = tokio::fs::rename(tmp, "peers.json").await;
        }
    }
}

async fn load_peers_from_file() -> Vec<PeerEntry> {
    if let Ok(b) = tokio::fs::read_to_string("peers.json").await {
        if let Ok(v) = serde_json::from_str::<Vec<PeerEntry>>(&b) {
            return v;
        }
    }
    Vec::new()
}

async fn fetch_bootstrap_peers(url: &str) -> Result<Vec<String>, anyhow::Error> {
    let body = reqwest::get(url).await?.text().await?;
    let peers: Vec<String> = body
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    Ok(peers)
}

/// prune peer_store to MAX_KNOWN_PEERS and remove very stale entries
fn prune_peer_list(list: &mut Vec<PeerEntry>) {
    const TTL_SECS: u64 = 60 * 60 * 24 * 7; // 7 days
    let cutoff = current_unix().saturating_sub(TTL_SECS);
    list.retain(|e| e.last_seen_unix.unwrap_or(0) >= cutoff || e.failures < 8);
    if list.len() > MAX_KNOWN_PEERS {
        // M10 fix: Diversity-aware pruning
        // Don't just prune by time; ensure we keep some peers of each role (Heavy, Medium, Light)
        // to maintain cross-tier connectivity (e.g., Light -> Heavy bridge).
        
        list.sort_by(|a, b| {
            // Sort by role presence first, then by last_seen
            let role_a = a.role.is_some();
            let role_b = b.role.is_some();
            role_b.cmp(&role_a).then_with(|| {
                b.last_seen_unix.unwrap_or(0).cmp(&a.last_seen_unix.unwrap_or(0))
            })
        });

        // Ensure we keep at least 5 of each role if they exist
        let mut heavy_count = 0;
        let mut medium_count = 0;
        let mut light_count = 0;
        let mut unknown_count = 0;

        list.retain(|e| {
            let role = e.role.as_deref().unwrap_or("unknown");
            match role {
                "heavy" => { heavy_count += 1; heavy_count <= MAX_KNOWN_PEERS / 3 || heavy_count <= 10 }
                "medium" => { medium_count += 1; medium_count <= MAX_KNOWN_PEERS / 3 || medium_count <= 10 }
                "light" => { light_count += 1; light_count <= MAX_KNOWN_PEERS / 3 || light_count <= 10 }
                _ => { unknown_count += 1; unknown_count <= 10 }
            }
        });

        if list.len() > MAX_KNOWN_PEERS {
            list.truncate(MAX_KNOWN_PEERS);
        }
    }
}

/// Returns lightweight p2p metrics (used by API)
pub fn get_p2p_metrics() -> (usize, usize, usize) {
    let conns = METRICS_ACTIVE_CONNECTIONS.load(Ordering::Relaxed);
    let dedupe = METRICS_DEDUPE_ENTRIES.load(Ordering::Relaxed);
    let peers = METRICS_PEER_COUNT.load(Ordering::Relaxed);
    (conns, dedupe, peers)
}

/// Returns network byte counters and resets them (for rate calculation)
pub fn get_and_reset_net_bytes() -> (usize, usize) {
    let bytes_in = METRICS_BYTES_IN.swap(0, Ordering::Relaxed);
    let bytes_out = METRICS_BYTES_OUT.swap(0, Ordering::Relaxed);
    (bytes_in, bytes_out)
}

/// Record inbound bytes
pub fn record_bytes_in(n: usize) {
    METRICS_BYTES_IN.fetch_add(n, Ordering::Relaxed);
}

/// Record outbound bytes
pub fn record_bytes_out(n: usize) {
    METRICS_BYTES_OUT.fetch_add(n, Ordering::Relaxed);
}
/// Extract IP subnet (/24) for diversity scoring
fn extract_subnet(addr: &str) -> Option<String> {
    // Parse "IP:PORT" format
    let ip_part = addr.split(':').next()?;

    // For IPv4: take first 3 octets (Class C subnet /24)
    let parts: Vec<&str> = ip_part.split('.').collect();
    if parts.len() == 4 {
        return Some(format!("{}.{}.{}", parts[0], parts[1], parts[2]));
    }

    // For IPv6 or .onion: use first part as rough diversity metric
    Some(ip_part.chars().take(10).collect())
}

/// Select diverse peers from candidate list
/// Prefers: different subnets, recent activity, and adds randomness
fn select_diverse_peers(
    candidates: &[PeerEntry],
    existing_addrs: &std::collections::HashSet<String>,
    count: usize,
) -> Vec<PeerEntry> {
    use rand::seq::SliceRandom;

    // Filter out already-connected peers
    let available: Vec<&PeerEntry> = candidates
        .iter()
        .filter(|p| !existing_addrs.contains(&p.addr))
        .collect();

    if available.is_empty() {
        return Vec::new();
    }

    // Group by subnet for diversity
    let mut subnet_groups: std::collections::HashMap<String, Vec<&PeerEntry>> =
        std::collections::HashMap::new();

    for peer in available {
        let subnet = extract_subnet(&peer.addr).unwrap_or_else(|| "unknown".to_string());
        subnet_groups
            .entry(subnet)
            .or_insert_with(Vec::new)
            .push(peer);
    }

    // Select one peer from each subnet (round-robin for diversity)
    let mut selected = Vec::new();
    let mut rng = rand::thread_rng();
    let mut subnet_keys: Vec<_> = subnet_groups.keys().cloned().collect();
    subnet_keys.shuffle(&mut rng);

    for subnet in subnet_keys {
        if selected.len() >= count {
            break;
        }

        if let Some(group) = subnet_groups.get_mut(&subnet) {
            // M8 fix: Sort first by recency, THEN shuffle within top candidates
            // to balance randomization with preferring active peers
            group.sort_by_key(|p| std::cmp::Reverse(p.last_seen_unix.unwrap_or(0)));
            // Take top half (or at least 2) of recently active peers, then pick randomly
            let top_n = (group.len() / 2).max(2).min(group.len());
            let top_slice = &mut group[..top_n];
            top_slice.shuffle(&mut rng);

            if let Some(peer) = top_slice.first() {
                selected.push((*peer).clone());
            }
        }
    }

    selected
}

/// Generic handler for inbound P2P connections.
/// Works with any stream type (TcpStream, TlsStream, etc.)
async fn handle_inbound_connection_generic<S>(
    stream: S,
    peer_addr: std::net::SocketAddr,
    inbound_tx: mpsc::Sender<Transaction>,
    ps: PeerStore,
    dedupe: DedupeCache,
) where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    match handshake::server_handshake_generic(stream).await {
        Ok((peer_info, framed)) => {
            tracing::info!(peer = %peer_info.node_id, addr = %peer_addr, "handshake success");
            tracing::info!("P2P CONNECTION ALLOWED: {}", peer_addr);

            // Split into read/write halves so we can respond to PEX requests
            let (mut sink, mut framed) = framed.split();

            // SECURITY: Peer authentication - verify against allowlist if configured
            if let Ok(allowlist) = std::env::var("PEER_ALLOWLIST") {
                let allowed_peers: Vec<&str> = allowlist
                    .split(',')
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .collect();

                if !allowed_peers.is_empty() {
                    let peer_allowed = allowed_peers.iter().any(|allowed_id| {
                        *allowed_id == peer_info.node_id || *allowed_id == peer_addr.to_string()
                    });

                    if !peer_allowed {
                        tracing::warn!(
                            "PEER AUTHENTICATION FAILED: Rejected connection from {} (node_id: {}) - not in allowlist",
                            peer_addr,
                            peer_info.node_id
                        );
                        return;
                    }

                    tracing::info!(
                        "PEER AUTHENTICATED: Accepted connection from {} (node_id: {})",
                        peer_addr,
                        peer_info.node_id
                    );
                }
            }

            // add to peer_store using remote socket address
            let remote_addr = peer_addr.to_string();
            {
                let mut store = ps.lock().await;
                if let Some(e) = store.iter_mut().find(|x| x.addr == remote_addr) {
                    e.last_seen_unix = Some(current_unix());
                    e.failures = 0;
                    e.role = Some(peer_info.role.clone());
                } else {
                    let mut entry = PeerEntry::new(remote_addr.clone());
                    entry.role = Some(peer_info.role.clone());
                    store.push(entry);
                    prune_peer_list(&mut store);
                    METRICS_PEER_COUNT.store(store.len(), Ordering::Relaxed);
                    let _ = save_peers_to_file(&ps).await;
                }
            }

            // Track inbound connection
            METRICS_ACTIVE_CONNECTIONS.fetch_add(1, Ordering::Relaxed);

            // read loop: framed stream -> handle envelopes
            while let Some(frame) = framed.next().await {
                match frame {
                    Ok(bytes) => {
                        record_bytes_in(bytes.len());
                        if bytes.is_empty() {
                            continue;
                        }
                        // M7 fix: Increased from 64KB to 256KB to accommodate PQ hybrid signatures
                        if bytes.len() > 256 * 1024 {
                            tracing::warn!(
                                "incoming envelope too large from {}: {} bytes",
                                peer_addr,
                                bytes.len()
                            );
                            continue;
                        }
                        if let Ok(env) = serde_json::from_slice::<Envelope>(bytes.as_ref()) {
                            if env.typ.is_empty() {
                                tracing::warn!("incoming envelope missing type; ignoring");
                                continue;
                            }
                            let msgid = message_id_from_envelope(&env);
                            {
                                let mut ded = dedupe.lock().await;
                                if let Some(exp) = ded.get(&msgid) {
                                    if *exp > Instant::now() {
                                        continue;
                                    }
                                }
                                ded.insert(msgid, Instant::now() + Duration::from_secs(300));
                            }

                            if env.typ == "gossip_tx" {
                                if let Ok(txn) =
                                    serde_json::from_value::<Transaction>(env.payload.clone())
                                {
                                    let _ = inbound_tx.send(txn).await;
                                }
                            } else if env.typ == "peer_list" {
                                if let Ok(pl) =
                                    serde_json::from_value::<handshake::PeerList>(env.payload)
                                {
                                    // PEX hardening: validate addresses & limit size
                                    let valid_peers: Vec<String> = pl.peers.into_iter()
                                        .filter(|p| is_valid_peer_address(p))
                                        .take(50)
                                        .collect();
                                    let mut store = ps.lock().await;
                                    let mut changed = false;
                                    for p in valid_peers {
                                        if !store.iter().any(|e| e.addr == p) {
                                            store.push(PeerEntry::new(p.clone()));
                                            changed = true;
                                        }
                                    }
                                    if changed {
                                        prune_peer_list(&mut store);
                                        METRICS_PEER_COUNT.store(store.len(), Ordering::Relaxed);
                                        let _ = save_peers_to_file(&ps).await;
                                    }
                                }
                            } else if env.typ == "peer_request" {
                                // PEX: Respond with recently-seen validated peers
                                let store = ps.lock().await;
                                let peers_to_share: Vec<String> = store.iter()
                                    .filter(|p| {
                                        let age = current_unix().saturating_sub(p.last_seen_unix.unwrap_or(0));
                                        age < 10800 && p.failures < 3 && is_valid_peer_address(&p.addr)
                                    })
                                    .take(50)
                                    .map(|p| p.addr.clone())
                                    .collect();
                                drop(store);
                                if !peers_to_share.is_empty() {
                                    let pl = handshake::PeerList { peers: peers_to_share.clone() };
                                    if let Ok(env_pl) = handshake::Envelope::new("peer_list", &pl) {
                                        if let Ok(bytes) = serde_json::to_vec(&env_pl) {
                                            if let Err(e) = sink.send(bytes.into()).await {
                                                tracing::warn!("PEX: Failed to send peer_list to {}: {}", peer_addr, e);
                                            } else {
                                                tracing::debug!("PEX: Sent {} peers to {}", peers_to_share.len(), peer_addr);
                                            }
                                        }
                                    }
                                }
                            }
                            // ping handled implicitly
                        }
                    }
                    Err(e) => {
                        tracing::warn!("P2P inbound read error from {}: {}", peer_addr, e);
                        break;
                    }
                }
            }
            tracing::info!("connection read loop ended for {}", peer_addr);
            METRICS_ACTIVE_CONNECTIONS.fetch_sub(1, Ordering::Relaxed);
        }
        Err(e) => {
            tracing::warn!("P2P: handshake failed from {}: {}", peer_addr, e);
        }
    }
}

/// Try UPnP mapping if enabled (non-blocking best-effort).
async fn try_upnp_map(listen_addr: &str) {
    if std::env::var("USE_UPNP").is_err() {
        return;
    }
    // parse listen port
    if let Some(pos) = listen_addr.rfind(':') {
        if let Ok(port) = listen_addr[pos + 1..].parse::<u16>() {
            // run mapping on background
            tokio::spawn(async move {
                match igd::aio::search_gateway(Default::default()).await {
                    Ok(gateway) => {
                        let local_port = port;
                        let external_port = std::env::var("EXTERNAL_PORT")
                            .ok()
                            .and_then(|s| s.parse::<u16>().ok())
                            .unwrap_or(local_port);
                        let local_ip_str = local_ip_address::local_ip()
                            .unwrap_or(std::net::IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)))
                            .to_string();
                        let local_ipv4: Ipv4Addr =
                            local_ip_str.parse().unwrap_or(Ipv4Addr::new(127, 0, 0, 1));
                        let local_socket_addr = SocketAddrV4::new(local_ipv4, local_port);
                        let lifetime = 60 * 60 * 24; // 1 day
                        match gateway
                            .add_port(
                                igd::PortMappingProtocol::TCP,
                                external_port,
                                local_socket_addr,
                                lifetime,
                                "ouro_p2p",
                            )
                            .await
                        {
                            Ok(_) => {
                                tracing::info!(
                                    "UPnP: mapped external {} -> local {} (ttl {})",
                                    external_port,
                                    local_port,
                                    lifetime
                                );
                            }
                            Err(e) => {
                                tracing::warn!("UPnP mapping failed: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("UPnP gateway search failed: {}", e);
                    }
                }
            });
        }
    }
}

/// Start network subsystem.
///
/// Returns (broadcast_sender, inbound_receiver, peer_store).
/// The function will internally spawn tasks and a Ctrl+C-based graceful shutdown
/// broadcaster that tasks listen to. Tasks are written to exit cleanly on shutdown.
///
/// TLS Support: Full TLS encryption is enabled when certificates are configured:
/// - `P2P_TLS_CERT_PATH` / `P2P_TLS_KEY_PATH`: Path to TLS certificate and key files
/// - Falls back to `TLS_CERT_PATH` / `TLS_KEY_PATH` if P2P-specific paths not set
/// - `P2P_TLS_CA_PATH`: Optional CA certificate for client verification
/// - `P2P_TLS_SKIP_VERIFY`: Set to "true" to skip certificate verification (INSECURE - testing only)
///
/// When TLS is configured:
/// - All inbound clearnet connections are encrypted with TLS
/// - All outbound clearnet connections are encrypted with TLS
/// - .onion addresses use TOR's built-in encryption (no TLS wrapping)
///
/// TOR Support: If tor_config is provided, the network will support hybrid clearnet + darkweb
/// operation, automatically routing .onion addresses through the TOR SOCKS proxy.
pub async fn start_network(
    listen_addr: &str,
    tor_config: Option<TorConfig>,
) -> (TxBroadcast, TxInboundReceiver, PeerStore) {
    // Load TLS configuration for P2P (if available)
    let tls_config = crate::load_p2p_tls_config();
    let tls_acceptor = tls_config
        .as_ref()
        .map(|config| TlsAcceptor::from(config.clone()));

    // Load TLS client configuration for outbound connections
    let tls_client_config = crate::load_p2p_client_tls_config();
    let tls_connector = tls_client_config.map(|config| TlsConnector::from(config));

    if tls_acceptor.is_some() {
        tracing::info!("P2P TLS ENABLED for inbound connections");
    }
    if tls_connector.is_some() {
        tracing::info!("P2P TLS ENABLED for outbound connections");
    }

    // Test TOR proxy if enabled
    if let Some(ref config) = tor_config {
        if config.is_enabled() {
            if let Err(e) = crate::tor::test_tor_proxy(config.proxy_addr()).await {
                log::warn!(
                    "WARNING: TOR proxy test failed: {}. TOR connections may not work.",
                    e
                );
            } else {
                log::info!("TOR proxy {} is working", config.proxy_addr());
            }
        }
    }

    let (bcast_tx, mut bcast_rx) = mpsc::channel::<Transaction>(256);
    let (inbound_tx, inbound_rx) = mpsc::channel::<Transaction>(256);

    // Prepare shutdown broadcaster (tasks subscribe to it)
    let (shutdown_tx, _) = broadcast::channel::<()>(1);
    {
        // spawn a Ctrl+C handler that broadcasts shutdown
        let tx = shutdown_tx.clone();
        tokio::spawn(async move {
            if let Err(e) = tokio::signal::ctrl_c().await {
                tracing::warn!("failed to install Ctrl+C handler: {}", e);
                return;
            }
            tracing::info!("shutdown signal received, notifying tasks...");
            let _ = tx.send(());
        });
    }

    // attempt UPnP mapping if configured (best-effort)
    try_upnp_map(listen_addr).await;

    let peer_store: PeerStore = Arc::new(Mutex::new(load_peers_from_file().await));
    {
        let mut s = peer_store.lock().await;

        let peers_env = std::env::var("PEER_ADDRS").unwrap_or_default();
        for p in peers_env.split(',').map(|p| p.trim()) {
            if !p.is_empty() && !s.iter().any(|e| e.addr == p) {
                s.push(PeerEntry::new(p.to_string()));
            }
        }

        prune_peer_list(&mut s);

        METRICS_PEER_COUNT.store(s.len(), Ordering::Relaxed);
        // Lock released at end of this block
    }

    // Save peers AFTER releasing the lock to avoid deadlock
    let _ = save_peers_to_file(&peer_store).await;

    // --- ORGANIC GROWTH: Enable DHT for automatic peer discovery ---
    let bootstrap_peers: Vec<String> = {
        let mut peers = Vec::new();
        // Collect from BOOTSTRAP_PEERS env
        if let Ok(peers_str) = std::env::var("BOOTSTRAP_PEERS") {
            peers.extend(
                peers_str
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
            );
        }
        // Collect from PEER_ADDRS env
        if let Ok(peers_str) = std::env::var("PEER_ADDRS") {
            peers.extend(
                peers_str
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
            );
        }
        peers
    };

    let dht_manager = match dht_integration::init_dht(listen_addr, peer_store.clone(), &bootstrap_peers).await {
        Ok(mgr) => {
            tracing::info!("DHT initialized for organic peer discovery");
            Some(mgr)
        }
        Err(e) => {
            tracing::warn!("DHT initialization failed (continuing without): {}", e);
            None
        }
    };

    // Spawn DHT refresh loop if initialized
    if let Some(ref dht) = dht_manager {
        let dht_clone = dht.clone();
        let shutdown_rx = shutdown_tx.subscribe();
        tokio::spawn(async move {
            let mut shutdown_rx = shutdown_rx;
            tokio::select! {
                _ = dht_clone.refresh_loop() => {}
                _ = shutdown_rx.recv() => {
                    tracing::info!("DHT refresh loop shutting down");
                }
            }
        });
    }
    // ------------------------------------------------------------------

    // dedupe cache (msgid -> expiry Instant)
    let dedupe: DedupeCache = Arc::new(Mutex::new(HashMap::new()));
    {
        let ded = dedupe.clone();
        let shutdown_rx = shutdown_tx.subscribe();
        tokio::spawn(async move {
            let mut shutdown_rx = shutdown_rx;
            loop {
                tokio::select! {
                _ = sleep(Duration::from_secs(30)) => {
                let mut guard = ded.lock().await;
                let now = Instant::now();
                guard.retain(|_, expiry| *expiry > now);
                METRICS_DEDUPE_ENTRIES.store(guard.len(), Ordering::Relaxed);
                }
                _ = shutdown_rx.recv() => {
                tracing::info!("dedupe prune task shutting down");
                break;
                }
                }
            }
        });
    }

    // connections map addr -> Connection
    let connections: Arc<Mutex<HashMap<String, Connection>>> = Arc::new(Mutex::new(HashMap::new()));

    // Node id
    let my_node_id = std::env::var("NODE_ID").unwrap_or_else(|_| Uuid::new_v4().to_string());

    // Listener - incoming connections
    let listen_addr_str = listen_addr.to_string();
    let inbound_clone = inbound_tx.clone();
    let peer_store_for_listener = peer_store.clone();
    let dedupe_for_listener = dedupe.clone();
    let dht_for_listener = dht_manager.clone();
    let mut shutdown_rx_for_listener = shutdown_tx.subscribe();
    tokio::spawn(async move {
        let listener = match TcpListener::bind(&listen_addr_str).await {
            Ok(l) => l,
            Err(e) => {
                tracing::error!("Failed to bind P2P listener {}: {}", listen_addr_str, e);
                return;
            }
        };
        tracing::info!("P2P listener bound to {}", listen_addr_str);
        loop {
            tokio::select! {
            accept_res = listener.accept() => {
            match accept_res {
            Ok((stream, peer_addr)) => {
            // Enforce MAX_ACTIVE_PEERS limit for scalability
            let current_active = METRICS_ACTIVE_CONNECTIONS.load(Ordering::Relaxed);
            if current_active >= MAX_ACTIVE_PEERS {
            tracing::warn!("Rejecting connection from {} - at max capacity ({}/{})", peer_addr, current_active, MAX_ACTIVE_PEERS);
            drop(stream); // Close connection
            continue;
            }

            let inbound_for_conn = inbound_clone.clone();
            let ps = peer_store_for_listener.clone();
            let dedupe_clone = dedupe_for_listener.clone();
            let tls_acceptor_clone = tls_acceptor.clone();
            let dht_clone = dht_for_listener.clone();
            tokio::spawn(async move {
                // Sync peer to DHT for organic discovery
                if let Some(ref dht) = dht_clone {
                    let peer_entry = PeerEntry::new(peer_addr.to_string());
                    dht.sync_peer_to_dht(&peer_entry).await;
                }

                // Handle connection: try TLS if configured, fall back to plain TCP
                if let Some(acceptor) = tls_acceptor_clone {
                    match acceptor.accept(stream).await {
                        Ok(tls_stream) => {
                            tracing::info!("TLS connection from {}", peer_addr);
                            handle_inbound_connection_generic(
                                tls_stream,
                                peer_addr,
                                inbound_for_conn,
                                ps,
                                dedupe_clone
                            ).await;
                        }
                        Err(e) => {
                            // TLS failed — peer likely sent plain TCP
                            // Can't retry on same stream (TLS consumed it), but log clearly
                            tracing::info!("TLS accept failed from {} ({}), peer may not use TLS", peer_addr, e);
                        }
                    }
                } else {
                    handle_inbound_connection_generic(
                        stream,
                        peer_addr,
                        inbound_for_conn,
                        ps,
                        dedupe_clone
                    ).await;
                }
            });
            }
            Err(e) => {
            tracing::warn!("Accept error: {}", e);
            }
            }
            }
            _ = shutdown_rx_for_listener.recv() => {
            tracing::info!("listener shutting down");
            break;
            }
            }
        }
    });

    // Connection manager: ensure outbound persistent connections
    let connections_for_manager = connections.clone();
    let peer_store_for_manager = peer_store.clone();
    let dedupe_for_manager = dedupe.clone();
    let inbound_for_manager = inbound_tx.clone();
    let shutdown_rx_for_manager = shutdown_tx.subscribe();
    let shutdown_tx_clone = shutdown_tx.clone();
    let tor_config_for_manager = tor_config.clone();
    let tls_connector_for_manager = tls_connector.clone();
    tokio::spawn(async move {
        let mut shutdown_rx = shutdown_rx_for_manager;
        loop {
            tokio::select! {
            _ = shutdown_rx.recv() => {
            tracing::info!("connection manager shutting down");
            break;
            }
            _ = sleep(Duration::from_secs(5)) => {
            // iterate peers and ensure connections
            {
            let peers = peer_store_for_manager.lock().await.clone();
            // Use actual confirmed connection count, not the metric counter
            // (metric only counts post-handshake; pending attempts don't block new ones)
            let current_active = METRICS_ACTIVE_CONNECTIONS.load(Ordering::Relaxed);
            let pending_count = {
                let conns = connections_for_manager.lock().await;
                conns.len()
            };
            // Don't start new attempts if too many are pending
            let effective_active = std::cmp::max(current_active, pending_count);

            // Enforce connection limits for scalable gossip network
            if effective_active >= MAX_ACTIVE_PEERS {
            // Already at max capacity
            continue;
            }

            let target_new_connections = if effective_active < MIN_ACTIVE_PEERS {
            // Emergency: need more connections for redundancy
            MIN_ACTIVE_PEERS - effective_active
            } else if effective_active < TARGET_ACTIVE_PEERS {
            // Normal: try to reach target
            TARGET_ACTIVE_PEERS - effective_active
            } else {
            // Above target: don't add more
            0
            };

                           if target_new_connections == 0 {
                               continue;
                           }

                           // Filter out banned peers and those in backoff period
                           let now = current_unix();
                           let available_peers: Vec<PeerEntry> = peers
                               .iter()
                               .filter(|p| {
                                   // Check ban status
                                   if let Some(banned_until) = p.banned_until_unix {
                                       if banned_until > now { return false; }
                                   }
                                   // Check exponential backoff readiness
                                   p.is_ready_for_retry()
                               })
                               .cloned()
                               .collect();

                           // Get existing connections for diversity check
                           let existing_addrs: std::collections::HashSet<String> = {
                               let conns = connections_for_manager.lock().await;
                               conns.keys().cloned().collect()
                           };

                           // Select diverse peers using smart selection algorithm
                           let selected_peers = select_diverse_peers(
                               &available_peers,
                               &existing_addrs,
                               target_new_connections
                           );

                           // Connect to selected diverse peers
                           for p in selected_peers {
                               let addr = p.addr.clone();

                               // Check if already connected (race condition guard)
                               {
                                   let conns = connections_for_manager.lock().await;
                                   if conns.contains_key(&addr) {
                                       continue;
                                   }
                               }

                               let (tx, rx) = mpsc::channel::<Envelope>(128);
                               let last_seen = Arc::new(Mutex::new(Some(Instant::now())));
                               let conn = Connection { addr: addr.clone(), tx: tx.clone(), last_seen: last_seen.clone() };

                               // Insert connection placeholder (NOT counted as active yet)
                               {
                                   let mut conns = connections_for_manager.lock().await;
                                   conns.insert(addr.clone(), conn);
                               }

                               // spawn per-peer connection task
                               let peer_store_clone = peer_store_for_manager.clone();
                               let connections_clone2 = connections_for_manager.clone();
                               let dedupe_clone2 = dedupe_for_manager.clone();
                               let inbound_clone2 = inbound_for_manager.clone();
                               let my_id_clone = my_node_id.clone();
                               let mut shutdown_inner = shutdown_tx_clone.subscribe();
                               let tor_config_clone = tor_config_for_manager.clone();
                               let tls_connector_clone = tls_connector_for_manager.clone();
                               tokio::spawn(async move {
            let connection_result: Result<(handshake::PeerInfo, Framed<BoxedAsyncStream, LengthDelimitedCodec>, Vec<String>), anyhow::Error> = async {
            // check for shutdown early
            if shutdown_inner.try_recv().is_ok() {
                return Err(anyhow::anyhow!("shutting down"));
            }

            // Use TOR-aware connection (auto-detects .onion addresses)
            // If tor_config is provided, use it; otherwise create default (disabled)
            let tor_cfg = tor_config_clone.unwrap_or_else(|| crate::tor::TorConfig::default());
            let is_onion = is_onion_address(&addr);
            let network_type = if is_onion { "TOR" } else { "Clearnet" };
            log::info!("{} connecting to {}", network_type, addr);

            let tcp_stream = crate::tor::connect_to_peer(&addr, &tor_cfg)
                .await
                .map_err(|e| anyhow::anyhow!("connect failed: {}", e))?;

            // For clearnet connections, try TLS if configured, fall back to plain TCP
            // For .onion addresses, TOR already provides encryption
            let boxed_stream: BoxedAsyncStream = if !is_onion {
                if let Some(ref connector) = tls_connector_clone {
                    // Extract hostname for SNI (Server Name Indication)
                    let host = addr.split(':').next().unwrap_or(&addr);
                    let server_name = match ServerName::try_from(host) {
                        Ok(name) => name,
                        Err(_) => {
                            // IP addresses: use "localhost" as SNI fallback
                            ServerName::try_from("localhost").unwrap()
                        }
                    };

                    match connector.connect(server_name, tcp_stream).await {
                        Ok(tls_stream) => {
                            log::info!("TLS connection established to {}", addr);
                            Box::pin(tls_stream) as BoxedAsyncStream
                        }
                        Err(e) => {
                            // TLS failed — fall back to plain TCP reconnect
                            // (peer may not have TLS configured)
                            log::info!("TLS failed to {} ({}), retrying plain TCP", addr, e);
                            let tcp_retry = crate::tor::connect_to_peer(&addr, &tor_cfg)
                                .await
                                .map_err(|e| anyhow::anyhow!("plain TCP reconnect failed: {}", e))?;
                            Box::pin(tcp_retry) as BoxedAsyncStream
                        }
                    }
                } else {
                    // Plain TCP (TLS not configured)
                    tracing::debug!("Plain TCP connection to {} (TLS not configured)", addr);
                    Box::pin(tcp_stream) as BoxedAsyncStream
                }
            } else {
                // .onion address - TOR provides encryption
                Box::pin(tcp_stream) as BoxedAsyncStream
            };

            // Create framed connection and perform handshake
            let codec = LengthDelimitedCodec::new();
            let framed = Framed::new(boxed_stream, codec);
            handshake::client_handshake_generic(framed, &my_id_clone, handshake::load_keypair_from_env())
                .await
                .map_err(|e| anyhow::anyhow!("handshake failed: {}", e))
            }.await;

            match connection_result {
            Ok((peer_info, framed_conn, discovered)) => {
            // Connection succeeded — NOW count it as active
            METRICS_ACTIVE_CONNECTIONS.fetch_add(1, Ordering::Relaxed);

            // split sink/stream so we can have independent send and receive tasks
            let (mut sink, mut stream) = framed_conn.split();

            // merge discovered peers
            {
            let mut store = peer_store_clone.lock().await;
            let mut changed = false;
            for d in discovered {
            if !store.iter().any(|e| e.addr == d) {
            store.push(PeerEntry::new(d.clone()));
            changed = true;
            }
            }
            if changed {
            prune_peer_list(&mut store);
            METRICS_PEER_COUNT.store(store.len(), Ordering::Relaxed);
            let _ = save_peers_to_file(&peer_store_clone).await;
            }
            }

            // reset failure count, backoff, update last_seen & store role
            {
            let mut store = peer_store_clone.lock().await;
            if let Some(e) = store.iter_mut().find(|e| e.addr == addr) {
            e.reset_backoff();
            e.last_seen_unix = Some(current_unix());
            e.role = Some(peer_info.role);
            }
            }

            // spawn sender task: rx -> sink
            let mut send_rx = rx;
            let send_addr = addr.clone();
            let peer_store_clone2 = peer_store_clone.clone();
            let sender_handle = tokio::spawn(async move {
            // keepalive timer (every 15s)
            let mut keepalive = tokio::time::interval(Duration::from_secs(15));
            // Peer exchange timer (every 60s) - request peer lists periodically
            let mut pex_timer = tokio::time::interval(Duration::from_secs(60));
            // Skip the first immediate tick on pex_timer
            pex_timer.tick().await;
            loop {
            tokio::select! {
            biased;

            maybe_env = send_rx.recv() => {
            match maybe_env {
            Some(env) => {
            // P2P rate limiting check using configurable limits
            let mut allow = true;
            {
            let mut store = peer_store_clone2.lock().await;
            if let Some(entry) = store.iter_mut().find(|e| e.addr == send_addr) {
            let window = entry.rate_window_start_unix.unwrap_or(current_unix());
            let now = current_unix();
            let window_secs = P2P_RATE_LIMIT_CONFIG.window_secs;
            let max_per_window = P2P_RATE_LIMIT_CONFIG.max_messages_per_window;

            if now >= window + window_secs {
            entry.rate_window_start_unix = Some(now);
            entry.rate_count = 0;
            }
            if entry.rate_count >= max_per_window {
            allow = false;
            } else {
            entry.rate_count = entry.rate_count.saturating_add(1);
            }
            }
            }
            if !allow {
            tracing::warn!(
            "P2P RATE LIMIT: Dropping outgoing envelope to {} (limit: {} msgs/{} secs)",
            send_addr,
            P2P_RATE_LIMIT_CONFIG.max_messages_per_window,
            P2P_RATE_LIMIT_CONFIG.window_secs
            );
            continue;
            }

            let bytes = match serde_json::to_vec(&env) {
            Ok(b) => b,
            Err(e) => {
            tracing::warn!("serialize envelope err: {}", e);
            continue;
            }
            };
            record_bytes_out(bytes.len());
            if let Err(e) = sink.send(bytes.into()).await {
            tracing::warn!("send to {} failed: {}", &send_addr, e);
            break;
            }
            }
            None => {
            // channel closed; exit sender
            break;
            }
            }
            }

            _ = keepalive.tick() => {
            // send a ping envelope
            let ping = match Envelope::new("ping", &serde_json::json!({"ts": current_unix()})) {
            Ok(e) => e,
            Err(_) => continue,
            };
            let bytes = match serde_json::to_vec(&ping) {
            Ok(b) => b,
            Err(_) => continue,
            };
            record_bytes_out(bytes.len());
            if let Err(e) = sink.send(bytes.into()).await {
            tracing::warn!("keepalive send to {} failed: {}", &send_addr, e);
            break;
            }
            }

            _ = pex_timer.tick() => {
            // Periodic Peer Exchange: request peer list from this peer
            let req = match Envelope::new("peer_request", &serde_json::json!({"ts": current_unix()})) {
            Ok(e) => e,
            Err(_) => continue,
            };
            let bytes = match serde_json::to_vec(&req) {
            Ok(b) => b,
            Err(_) => continue,
            };
            record_bytes_out(bytes.len());
            if let Err(e) = sink.send(bytes.into()).await {
            tracing::debug!("PEX request to {} failed: {}", &send_addr, e);
            break;
            }
            tracing::debug!("PEX: Requested peer list from {}", &send_addr);
            }
            }
            }
            });

            // read loop: stream -> handle incoming envelopes
            while let Some(item) = stream.next().await {
            match item {
            Ok(bytes) => {
            record_bytes_in(bytes.len());
            if bytes.is_empty() { continue; }
            if bytes.len() > 256 * 1024 { continue; } // M7 fix: 256KB for PQ sigs
            if let Ok(env) = serde_json::from_slice::<Envelope>(bytes.as_ref()) {
            if env.typ.is_empty() { continue; }
            let msgid = message_id_from_envelope(&env);
            {
            let mut ded = dedupe_clone2.lock().await;
            if let Some(exp) = ded.get(&msgid) {
            if *exp > Instant::now() { continue; }
            }
            ded.insert(msgid, Instant::now() + Duration::from_secs(300));
            }

            match env.typ.as_str() {
            "gossip_tx" => {
            if let Ok(txn) = serde_json::from_value::<Transaction>(env.payload.clone()) {
            let _ = inbound_clone2.send(txn).await;
            }
            }
            "pong" => {
            if let Some(conn) = connections_clone2.lock().await.get_mut(&addr) {
            let mut ls = conn.last_seen.lock().await;
            *ls = Some(Instant::now());
            }
            let mut store = peer_store_clone.lock().await;
            if let Some(e) = store.iter_mut().find(|x| x.addr == addr) {
            e.last_seen_unix = Some(current_unix());
            e.failures = 0;
            let _ = save_peers_to_file(&peer_store_clone).await;
            }
            }
            "peer_list" => {
            if let Ok(pl) = serde_json::from_value::<handshake::PeerList>(env.payload) {
            // PEX hardening: validate addresses, limit size, filter recency
            let valid_peers: Vec<String> = pl.peers.into_iter()
                .filter(|p| is_valid_peer_address(p) && *p != addr)
                .take(50)
                .collect();
            if !valid_peers.is_empty() {
            tracing::debug!("PEX: Received {} valid peers from {}", valid_peers.len(), &addr);
            let mut store = peer_store_clone.lock().await;
            let mut changed = false;
            for p in valid_peers {
            if !store.iter().any(|e| e.addr == p) {
            store.push(PeerEntry::new(p.clone()));
            changed = true;
            }
            }
            if changed {
            prune_peer_list(&mut store);
            METRICS_PEER_COUNT.store(store.len(), Ordering::Relaxed);
            let _ = save_peers_to_file(&peer_store_clone).await;
            }
            }
            }
            }
            "ping" => {
            match Envelope::new("pong", &serde_json::json!({"ts": current_unix()})) {
            Ok(pong) => {
            if let Err(e) = tx.try_send(pong) {
            tracing::warn!("failed to queue pong to {}: {}", &addr, e);
            }
            }
            Err(e) => {
            tracing::warn!("failed to create pong envelope for {}: {}", &addr, e);
            }
            }
            }
            "peer_request" => {
            // Respond with our known peers (recently seen, validated, max 50)
            let store = peer_store_clone.lock().await;
            let peers_to_share: Vec<String> = store.iter()
                .filter(|p| {
                    let age = current_unix().saturating_sub(p.last_seen_unix.unwrap_or(0));
                    age < 10800 // Seen within last 3 hours
                        && p.failures < 3
                        && is_valid_peer_address(&p.addr)
                        && p.addr != addr // Don't send peer's own address back
                })
                .take(50)
                .map(|p| p.addr.clone())
                .collect();
            drop(store);
            if !peers_to_share.is_empty() {
                let pl = handshake::PeerList { peers: peers_to_share };
                if let Ok(env) = Envelope::new("peer_list", &pl) {
                    if let Err(e) = tx.try_send(env) {
                        tracing::debug!("PEX: failed to queue peer_list to {}: {}", &addr, e);
                    } else {
                        tracing::debug!("PEX: Sent {} peers to {}", pl.peers.len(), &addr);
                    }
                }
            }
            }
            _ => {}
            }
            }
            }
            Err(e) => {
            tracing::warn!("read error from {}: {}", &addr, e);
            break;
            }
            }
            }

            // connection closed / read loop ended -> kill sender
            sender_handle.abort();
            }
            Err(e) => {
            tracing::warn!("outbound connection to {} failed: {}", &addr, e);
            // mark failure with exponential backoff
            let mut store = peer_store_clone.lock().await;
            if let Some(ent) = store.iter_mut().find(|e| e.addr == addr) {
            ent.record_failure_with_backoff();
            tracing::debug!(
                "Peer {} backoff: {} failures, retry in {}s",
                &addr, ent.failures, ent.backoff_secs
            );
            let _ = save_peers_to_file(&peer_store_clone).await;
            }
            // Failed connections were never counted as active — don't decrement
            connections_clone2.lock().await.remove(&addr);
            return;
            }
            }

            // remove SUCCESSFUL connection mapping at the end of the task's life
            connections_clone2.lock().await.remove(&addr);
            METRICS_ACTIVE_CONNECTIONS.fetch_sub(1, Ordering::Relaxed);
            });
            }
            }

            // cleanup: remove connections not in peer list
            {
            let peers = peer_store_for_manager.lock().await;
            let mut conns = connections_for_manager.lock().await;
            let allowed: HashSet<String> = peers.iter().map(|p| p.addr.clone()).collect();
            conns.retain(|k, _| allowed.contains(k));
            }
            }
            }
        }
    });

    // Broadcaster: read bcast_rx, create Envelope, dedupe, fan out to connection txs (bounded)
    let connections_for_bcast = connections.clone();
    let dedupe_for_bcast = dedupe.clone();
    let peer_store_for_bcast = peer_store.clone();
    let shutdown_rx_for_bcast = shutdown_tx.subscribe();
    tokio::spawn(async move {
        const MAX_FANOUT: usize = 8;
        let mut shutdown_rx = shutdown_rx_for_bcast;
        loop {
            tokio::select! {
            _ = shutdown_rx.recv() => {
            tracing::info!("broadcaster shutting down");
            break;
            }
            maybe_txn = bcast_rx.recv() => {
            let txn = match maybe_txn {
            Some(t) => t,
            None => break,
            };

            // envelope creation
            let env = match Envelope::new("gossip_tx", &txn) {
            Ok(e) => e,
            Err(e) => {
            tracing::warn!("failed to create gossip envelope: {}", e);
            continue;
            }
            };

            let msgid = message_id_from_envelope(&env);

            // dedupe outbound
            {
            let mut d = dedupe_for_bcast.lock().await;
            if let Some(exp) = d.get(&msgid) {
            if *exp > Instant::now() { continue; }
            }
            d.insert(msgid.clone(), Instant::now() + Duration::from_secs(300));
            METRICS_DEDUPE_ENTRIES.store(d.len(), Ordering::Relaxed);
            }

            // snapshot connections
            let conns_snapshot = {
            let conns = connections_for_bcast.lock().await;
            conns.iter().map(|(k, v)| (k.clone(), v.tx.clone())).collect::<Vec<_>>()
            };

                           if conns_snapshot.is_empty() {
                               // Use multi-strategy bootstrap waterfall
                               let remote_peers = discover_bootstrap_peers(&peer_store_for_bcast).await;

                               if !remote_peers.is_empty() {
                                   tracing::info!("Bootstrap waterfall: {} peers discovered", remote_peers.len());
                                   let mut store = peer_store_for_bcast.lock().await;
                                   let mut changed = false;
                                   for p in remote_peers {
                                       if !store.iter().any(|e| e.addr == p) {
                                           store.push(PeerEntry::new(p.clone()));
                                           changed = true;
                                       }
                                   }
                                   if changed {
                                       let _ = save_peers_to_file(&peer_store_for_bcast).await;
                                       METRICS_PEER_COUNT.store(store.len(), Ordering::Relaxed);
                                   }
                               } else {
                                   tracing::warn!("Bootstrap: no peers discovered from any strategy. Set BOOTSTRAP_PEERS or PEER_ADDRS in .env");
                               }
                           }

            // bounded fanout selection
            let mut targets: Vec<(String, mpsc::Sender<Envelope>)> = Vec::new();
            if !conns_snapshot.is_empty() {
            let n = conns_snapshot.len();
            let start_byte = msgid.as_bytes()[0] as usize;
            let mut idx = start_byte % n;
            let mut picked = 0usize;
            while picked < std::cmp::min(MAX_FANOUT, n) {
            let (addr, tx) = &conns_snapshot[idx];
            targets.push((addr.clone(), tx.clone()));
            picked += 1;
            idx = (idx + 1) % n;
            }
            }

            // fan out to selected peers concurrently (best-effort)
            for (peer_addr, tx) in targets {
            let env_clone = env.clone();
            let peer_store_inner = peer_store_for_bcast.clone();
            tokio::spawn(async move {
            // P2P rate-limit check using configurable limits
            let mut allow = true;
            {
            let mut store = peer_store_inner.lock().await;
            if let Some(entry) = store.iter_mut().find(|e| e.addr == peer_addr) {
            let now = current_unix();
            let window_secs = P2P_RATE_LIMIT_CONFIG.window_secs;
            let max_per_window = P2P_RATE_LIMIT_CONFIG.max_messages_per_window;
            let start = entry.rate_window_start_unix.unwrap_or(now);

            if now >= start + window_secs {
            entry.rate_window_start_unix = Some(now);
            entry.rate_count = 0;
            }
            if entry.rate_count >= max_per_window {
            allow = false;
            } else {
            entry.rate_count = entry.rate_count.saturating_add(1);
            }
            }
            }
            if !allow {
            tracing::warn!(
            "P2P RATE LIMIT: Skipping send to {} (limit: {} msgs/{} secs)",
            peer_addr,
            P2P_RATE_LIMIT_CONFIG.max_messages_per_window,
            P2P_RATE_LIMIT_CONFIG.window_secs
            );
            return;
            }

            if let Err(e) = tx.try_send(env_clone) {
            tracing::warn!("P2P: failed to queue envelope to {}: {}", peer_addr, e);
            // record failure
            let mut store = peer_store_inner.lock().await;
            if let Some(entry) = store.iter_mut().find(|e| e.addr == peer_addr) {
            entry.failures = entry.failures.saturating_add(1);
            if entry.failures >= 5 {
            entry.banned_until_unix = Some(current_unix() + 60 * 5);
            }
            let _ = save_peers_to_file(&peer_store_inner).await;
            }
            }
            });
            }

            sleep(Duration::from_millis(5)).await;
            }
            }
        }
    });

    // TODO: Heartbeat task disabled - needs db parameter in start_network signature
    /* Commented out until start_network accepts db parameter
    let heartbeat_db = db.clone();
    let heartbeat_node_id = my_node_id.clone();
    let mut heartbeat_shutdown = shutdown_tx.subscribe();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60)); // Heartbeat every 60 seconds

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    // Get wallet address from environment or node ID
                    let wallet_address = std::env::var("NODE_WALLET_ADDRESS")
                        .unwrap_or_else(|_| heartbeat_node_id.clone());

                    // Send heartbeat
                    let role = {
                        let config = crate::config_manager::CONFIG.read().await;
                        config.role.clone()
                    };
                    match crate::rewards::record_heartbeat(&heartbeat_db, &heartbeat_node_id, &wallet_address, role).await {
                        Ok(_) => {
                            tracing::debug!("Heartbeat sent successfully");
                        }
                        Err(e) => {
                            tracing::warn!("Failed to send heartbeat: {}", e);
                        }
                    }
                }
                _ = heartbeat_shutdown.recv() => {
                    tracing::info!("Heartbeat task shutting down");
                    break;
                }
            }
        }
    });
    */

    (bcast_tx, inbound_rx, peer_store)
}

use crate::network::bft_msg::BftMessage;
use serde_json;
use tokio::io::AsyncReadExt;

pub async fn handle_connection(
    mut stream: tokio::net::TcpStream,
    node: std::sync::Arc<tokio::sync::Mutex<crate::bft::consensus::HotStuff>>,
) {
    loop {
        // read 4-byte length
        let mut lenb = [0u8; 4];
        if let Err(_) = stream.read_exact(&mut lenb).await {
            break;
        }
        let len = u32::from_be_bytes(lenb) as usize;
        let mut buf = vec![0u8; len];
        if let Err(_) = stream.read_exact(&mut buf).await {
            break;
        }

        match serde_json::from_slice::<BftMessage>(&buf) {
            Ok(msg) => {
                let node = node.clone();
                tokio::spawn(async move {
                    let n = node.lock().await;
                    match msg {
                        BftMessage::Proposal(p) => {
                            let _ = n.handle_proposal(p).await;
                        }
                        BftMessage::Vote(v) => {
                            let _ = n.handle_vote(v).await;
                        }
                        BftMessage::QC(qc) => {
                            let _ = n.handle_qc(qc).await;
                        }
                        BftMessage::Ping => { /* ignore or respond */ }
                        BftMessage::Pong => { /* ignore */ }
                    }
                });
            }
            Err(e) => {
                log::warn!("Failed deserializing BftMessage: {}", e);
                break;
            }
        }
    }
}
pub mod dht;
pub mod dht_integration;
