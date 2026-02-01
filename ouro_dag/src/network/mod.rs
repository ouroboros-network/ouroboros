use log;

// src/network.rs
pub mod handshake;
use anyhow::Result;
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
pub mod bft_msg;

// TOR support for hybrid clearnet + darkweb operation
use crate::tor::{TorConfig, is_onion_address};


use std::collections::{HashMap, HashSet};
use std::net::{Ipv4Addr, SocketAddrV4};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc, Mutex};
use tokio::time::sleep;
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use uuid::Uuid;
use tokio_rustls::{TlsAcceptor, TlsConnector, client::TlsStream as ClientTlsStream, server::TlsStream as ServerTlsStream};
use tokio::io::{AsyncRead, AsyncWrite};
use std::pin::Pin;
use tokio_rustls::rustls::ServerName;

/// Trait combining AsyncRead + AsyncWrite for boxed stream type erasure
trait AsyncStream: AsyncRead + AsyncWrite + Send + Unpin {}
impl<T: AsyncRead + AsyncWrite + Send + Unpin> AsyncStream for T {}

/// Type alias for a boxed async stream that can be either TLS or plain TCP
type BoxedAsyncStream = Pin<Box<dyn AsyncStream>>;

use crate::dag::transaction::Transaction;
use self::handshake::{Envelope, message_id_from_envelope};

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
const MIN_ACTIVE_PEERS: usize = 3;     // Minimum connections (redundancy - your idea!)
const TARGET_ACTIVE_PEERS: usize = 8;  // Target connections (gossip efficiency)
const MAX_ACTIVE_PEERS: usize = 32;    // Maximum connections (prevent overload)
const MAX_KNOWN_PEERS: usize = 2000;   // Total peers we remember

/// Lightweight metrics
static METRICS_ACTIVE_CONNECTIONS: Lazy<AtomicUsize> = Lazy::new(|| AtomicUsize::new(0));
static METRICS_DEDUPE_ENTRIES: Lazy<AtomicUsize> = Lazy::new(|| AtomicUsize::new(0));
static METRICS_PEER_COUNT: Lazy<AtomicUsize> = Lazy::new(|| AtomicUsize::new(0));
/// Default bootstrap nodes for peer discovery
/// HYBRID ARCHITECTURE: Mix of server nodes + community P2P nodes
/// These are fallback nodes used when BOOTSTRAP_PEERS/BOOTSTRAP_URLS env var is not set
const DEFAULT_BOOTSTRAP_NODES: &[&str] = &[
    // Official seed servers (when available - add domains when you deploy)
    // "seed1.ouroboros.network:9001",
    // "seed2.ouroboros.network:9001",

    // Community-run nodes (volunteers can add their addresses here)
    // "community-node-1.example.com:9001",

    // For testing: localhost nodes (remove in production)
    // "127.0.0.1:9002",
    // "127.0.0.1:9003",
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
 // rate limit window
 pub rate_window_start_unix: Option<u64>,
 pub rate_count: u32,
}

impl PeerEntry {
 pub fn new(addr: String) -> Self {
 Self {
 addr,
 last_seen_unix: Some(current_unix()),
 failures: 0,
 banned_until_unix: None,
 rate_window_start_unix: Some(current_unix()),
 rate_count: 0,
 }
 }
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
 SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

async fn save_peers_to_file(store: &PeerStore) {
 let peers = store.lock().await;
 if let Ok(json) = serde_json::to_string(&*peers) {
 let _ = tokio::fs::write("peers.json", json).await;
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
 let peers: Vec<String> = body.lines().map(|l| l.trim().to_string()).filter(|s| !s.is_empty()).collect();
 Ok(peers)
}

/// prune peer_store to MAX_KNOWN_PEERS and remove very stale entries
fn prune_peer_list(list: &mut Vec<PeerEntry>) {
 const TTL_SECS: u64 = 60 * 60 * 24 * 7; // 7 days
 let cutoff = current_unix().saturating_sub(TTL_SECS);
 list.retain(|e| e.last_seen_unix.unwrap_or(0) >= cutoff || e.failures < 8);
 if list.len() > MAX_KNOWN_PEERS {
 list.sort_by_key(|e| std::cmp::Reverse(e.last_seen_unix.unwrap_or(0)));
 list.truncate(MAX_KNOWN_PEERS);
 }
}

/// Returns lightweight p2p metrics (used by API)
pub fn get_p2p_metrics() -> (usize, usize, usize) {
 let conns = METRICS_ACTIVE_CONNECTIONS.load(Ordering::Relaxed);
 let dedupe = METRICS_DEDUPE_ENTRIES.load(Ordering::Relaxed);
 let peers = METRICS_PEER_COUNT.load(Ordering::Relaxed);
 (conns, dedupe, peers)
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
        subnet_groups.entry(subnet).or_insert_with(Vec::new).push(peer);
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
            // Shuffle within subnet for fairness
            group.shuffle(&mut rng);
            
            // Prefer recently active peers (last_seen_unix)
            group.sort_by_key(|p| std::cmp::Reverse(p.last_seen_unix.unwrap_or(0)));
            
            if let Some(peer) = group.first() {
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
        Ok((peer_info, mut framed)) => {
            tracing::info!(peer = %peer_info.node_id, addr = %peer_addr, "handshake success");
            tracing::info!("P2P CONNECTION ALLOWED: {}", peer_addr);

            // SECURITY: Peer authentication - verify against allowlist if configured
            if let Ok(allowlist) = std::env::var("PEER_ALLOWLIST") {
                let allowed_peers: Vec<&str> = allowlist
                    .split(',')
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .collect();

                if !allowed_peers.is_empty() {
                    let peer_allowed = allowed_peers.iter().any(|allowed_id| {
                        *allowed_id == peer_info.node_id ||
                        *allowed_id == peer_addr.to_string()
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
                if !store.iter().any(|e| e.addr == remote_addr) {
                    store.push(PeerEntry::new(remote_addr.clone()));
                    prune_peer_list(&mut store);
                    METRICS_PEER_COUNT.store(store.len(), Ordering::Relaxed);
                    let _ = save_peers_to_file(&ps).await;
                } else {
                    if let Some(e) = store.iter_mut().find(|x| x.addr == remote_addr) {
                        e.last_seen_unix = Some(current_unix());
                        e.failures = 0;
                    }
                }
            }

            // Track inbound connection
            METRICS_ACTIVE_CONNECTIONS.fetch_add(1, Ordering::Relaxed);

            // read loop: framed stream -> handle envelopes
            while let Some(frame) = framed.next().await {
                match frame {
                    Ok(bytes) => {
                        if bytes.is_empty() { continue; }
                        if bytes.len() > 64 * 1024 {
                            tracing::warn!("incoming envelope too large from {}: {} bytes", peer_addr, bytes.len());
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
                                    if *exp > Instant::now() { continue; }
                                }
                                ded.insert(msgid, Instant::now() + Duration::from_secs(300));
                            }

                            if env.typ == "gossip_tx" {
                                if let Ok(txn) = serde_json::from_value::<Transaction>(env.payload.clone()) {
                                    let _ = inbound_tx.send(txn).await;
                                }
                            } else if env.typ == "peer_list" {
                                if let Ok(pl) = serde_json::from_value::<handshake::PeerList>(env.payload) {
                                    let mut store = ps.lock().await;
                                    let mut changed = false;
                                    for p in pl.peers {
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
 let external_port = std::env::var("EXTERNAL_PORT").ok().and_then(|s| s.parse::<u16>().ok()).unwrap_or(local_port);
 let local_ip_str = local_ip_address::local_ip()
 .unwrap_or(std::net::IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)))
 .to_string();
 let local_ipv4: Ipv4Addr = local_ip_str.parse().unwrap_or(Ipv4Addr::new(127, 0, 0, 1));
 let local_socket_addr = SocketAddrV4::new(local_ipv4, local_port);
 let lifetime = 60 * 60 * 24; // 1 day
 match gateway.add_port(igd::PortMappingProtocol::TCP, external_port, local_socket_addr, lifetime, "ouro_p2p").await {
 Ok(_) => {
 tracing::info!("UPnP: mapped external {} -> local {} (ttl {})", external_port, local_port, lifetime);
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
 let tls_acceptor = tls_config.as_ref().map(|config| {
 TlsAcceptor::from(config.clone())
 });

 // Load TLS client configuration for outbound connections
 let tls_client_config = crate::load_p2p_client_tls_config();
 let tls_connector = tls_client_config.map(|config| {
 TlsConnector::from(config)
 });

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
 log::warn!("WARNING: TOR proxy test failed: {}. TOR connections may not work.", e);
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
 tokio::spawn(async move {
     // Handle connection with TLS if configured
     if let Some(acceptor) = tls_acceptor_clone {
         // TLS-enabled connection - wrap stream before handshake
         match acceptor.accept(stream).await {
             Ok(tls_stream) => {
                 tracing::info!("TLS handshake successful from {}", peer_addr);
                 handle_inbound_connection_generic(
                     tls_stream,
                     peer_addr,
                     inbound_for_conn,
                     ps,
                     dedupe_clone
                 ).await;
             }
             Err(e) => {
                 tracing::warn!("TLS handshake failed from {}: {}", peer_addr, e);
             }
         }
     } else {
         // Plain TCP connection (development/testing only - NOT RECOMMENDED FOR PRODUCTION)
         tracing::debug!("Plain TCP connection from {} (TLS not configured)", peer_addr);
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
 let current_active = METRICS_ACTIVE_CONNECTIONS.load(Ordering::Relaxed);

 // Enforce connection limits for scalable gossip network
 if current_active >= MAX_ACTIVE_PEERS {
 // Already at max capacity
 continue;
 }

 let target_new_connections = if current_active < MIN_ACTIVE_PEERS {
 // Emergency: need more connections for redundancy
 MIN_ACTIVE_PEERS - current_active
 } else if current_active < TARGET_ACTIVE_PEERS {
 // Normal: try to reach target
 TARGET_ACTIVE_PEERS - current_active
 } else {
 // Above target: don't add more
 0
 };

                if target_new_connections == 0 {
                    continue;
                }

                // Filter out banned peers
                let now = current_unix();
                let available_peers: Vec<PeerEntry> = peers
                    .iter()
                    .filter(|p| {
                        if let Some(banned_until) = p.banned_until_unix {
                            banned_until <= now
                        } else {
                            true
                        }
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
                    
                    // Insert connection and track it
                    {
                        let mut conns = connections_for_manager.lock().await;
                        conns.insert(addr.clone(), conn);
                        METRICS_ACTIVE_CONNECTIONS.fetch_add(1, Ordering::Relaxed);
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
 let connection_result: Result<(Framed<BoxedAsyncStream, LengthDelimitedCodec>, Vec<String>), anyhow::Error> = async {
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

 // For clearnet connections, use TLS if configured
 // For .onion addresses, TOR already provides encryption
 let boxed_stream: BoxedAsyncStream = if !is_onion {
     if let Some(ref connector) = tls_connector_clone {
         // Extract hostname for SNI (Server Name Indication)
         let host = addr.split(':').next().unwrap_or(&addr);
         let server_name = match ServerName::try_from(host) {
             Ok(name) => name,
             Err(_) => {
                 // Fallback for IP addresses (no SNI)
                 tracing::warn!("TLS SNI: Using IP address {} - some servers may reject this", host);
                 ServerName::try_from("localhost").unwrap()
             }
         };

         match connector.connect(server_name, tcp_stream).await {
             Ok(tls_stream) => {
                 log::info!("TLS connection established to {}", addr);
                 Box::pin(tls_stream) as BoxedAsyncStream
             }
             Err(e) => {
                 log::warn!("TLS connection failed to {}: {} - connection rejected (TLS required)", addr, e);
                 return Err(anyhow::anyhow!("TLS connection failed: {}", e));
             }
         }
     } else {
         // Plain TCP (TLS not configured) - log warning for production awareness
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
 Ok((framed_conn, discovered)) => {
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

 // reset failure count & update last_seen
 {
 let mut store = peer_store_clone.lock().await;
 if let Some(e) = store.iter_mut().find(|e| e.addr == addr) {
 e.failures = 0;
 e.last_seen_unix = Some(current_unix());
 }
 }

 // spawn sender task: rx -> sink
 let mut send_rx = rx;
 let send_addr = addr.clone();
 let peer_store_clone2 = peer_store_clone.clone();
 let sender_handle = tokio::spawn(async move {
 // keepalive timer
 let mut keepalive = tokio::time::interval(Duration::from_secs(15));
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
 if let Err(e) = sink.send(bytes.into()).await {
 tracing::warn!("keepalive send to {} failed: {}", &send_addr, e);
 break;
 }
 }
 }
 }
 });

 // read loop: stream -> handle incoming envelopes
 while let Some(item) = stream.next().await {
 match item {
 Ok(bytes) => {
 if bytes.is_empty() { continue; }
 if bytes.len() > 64 * 1024 { continue; }
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
 let mut store = peer_store_clone.lock().await;
 let mut changed = false;
 for p in pl.peers {
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
 // mark failure
 let mut store = peer_store_clone.lock().await;
 if let Some(ent) = store.iter_mut().find(|e| e.addr == addr) {
 ent.failures = ent.failures.saturating_add(1);
 if ent.failures >= 5 {
 ent.banned_until_unix = Some(current_unix() + 60 * 5);
 }
 let _ = save_peers_to_file(&peer_store_clone).await;
 }
 }
 }

 // remove connection mapping at the end of the task's life
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
                    // Try bootstrap if we have no connections
                    let mut remote_peers = Vec::new();
                    
                    // Option 1: Direct peer addresses (BOOTSTRAP_PEERS)
                    if let Ok(peers_str) = std::env::var("BOOTSTRAP_PEERS") {
                        let direct_peers: Vec<String> = peers_str.split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                        
                        if !direct_peers.is_empty() {
                            tracing::info!("Bootstrap: using {} direct peer addresses", direct_peers.len());
                            remote_peers.extend(direct_peers);
                        }
                    }
                    
                    // Option 2: Bootstrap URLs (fetch peer lists from HTTP endpoints)
                    let bootstrap_urls: Vec<String> = if let Ok(urls_str) = std::env::var("BOOTSTRAP_URLS") {
                        urls_str.split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect()
                    } else if let Ok(url) = std::env::var("BOOTSTRAP_URL") {
                        vec![url]
                    } else {
                        DEFAULT_BOOTSTRAP_NODES.iter().map(|s| s.to_string()).collect()
                    };
                    
                    if !bootstrap_urls.is_empty() {
                        let fetched_peers = fetch_bootstrap_peers_multi(&bootstrap_urls).await;
                        if !fetched_peers.is_empty() {
                            tracing::info!("Bootstrap: fetched {} peers from URLs", fetched_peers.len());
                            remote_peers.extend(fetched_peers);
                        }
                    }
                    
                    // Add discovered peers to peer store
                    if !remote_peers.is_empty() {
                        tracing::info!("Bootstrap: total {} peers discovered", remote_peers.len());
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
                        tracing::warn!("Bootstrap: no peers configured. Set BOOTSTRAP_PEERS or BOOTSTRAP_URLS in .env");
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
                    match crate::rewards::record_heartbeat(&heartbeat_db, &heartbeat_node_id, &wallet_address).await {
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

pub async fn handle_connection(mut stream: tokio::net::TcpStream, node: std::sync::Arc<tokio::sync::Mutex<crate::bft::consensus::HotStuff>>) {
 loop {
 // read 4-byte length
 let mut lenb = [0u8; 4];
 if let Err(_) = stream.read_exact(&mut lenb).await { break; }
 let len = u32::from_be_bytes(lenb) as usize;
 let mut buf = vec![0u8; len];
 if let Err(_) = stream.read_exact(&mut buf).await { break; }

 match serde_json::from_slice::<BftMessage>(&buf) {
 Ok(msg) => {
 let node = node.clone();
 tokio::spawn(async move {
 let n = node.lock().await;
 match msg {
 BftMessage::Proposal(p) => { let _ = n.handle_proposal(p).await; },
 BftMessage::Vote(v) => { let _ = n.handle_vote(v).await; },
 BftMessage::QC(qc) => { let _ = n.handle_qc(qc).await; },
 BftMessage::Ping => { /* ignore or respond */ },
 BftMessage::Pong => { /* ignore */ },
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
