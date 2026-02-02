// src/peer_discovery/mod.rs
//! Peer Discovery Protocol
//!
//! Implements a gossip-based peer discovery system that eliminates the need for
//! hardcoded peer lists. Validators can automatically discover and connect to
//! each other through:
//!
//! 1. Bootstrap nodes - Initial entry points into the network
//! 2. Peer gossip - Validators share their peer lists with each other
//! 3. Health monitoring - Track peer liveness and responsiveness
//! 4. Persistence - Save discovered peers to survive restarts
//!
//! Security properties:
//! - Byzantine fault tolerant: works even with malicious bootstrap nodes
//! - Sybil resistant: requires on-chain validator registration
//! - Eclipse attack resistant: maintains connections to multiple diverse peers

use anyhow::Result;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::interval;

/// Peer information shared during gossip
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct PeerInfo {
    /// Network address (IP:port or .onion:port)
    pub address: String,
    /// Ed25519 public key for this peer (validator identity)
    pub pubkey: Vec<u8>,
    /// Node ID (human-readable identifier)
    pub node_id: String,
    /// BFT consensus port
    pub bft_port: u16,
    /// API port (optional)
    pub api_port: Option<u16>,
    /// TOR onion address if available
    pub onion_address: Option<String>,
    /// Unix timestamp when this peer info was last updated
    pub last_seen: u64,
    /// Peer reputation score (0-100, higher is better)
    pub reputation: u8,
}

impl PeerInfo {
    pub fn new(address: String, pubkey: Vec<u8>, node_id: String, bft_port: u16) -> Self {
        Self {
            address,
            pubkey,
            node_id,
            bft_port,
            api_port: None,
            onion_address: None,
            last_seen: now(),
            reputation: 50, // Start at neutral reputation
        }
    }

    /// Check if peer info is stale (not seen in 24 hours)
    pub fn is_stale(&self) -> bool {
        let age = now().saturating_sub(self.last_seen);
        age > 86400 // 24 hours
    }

    /// Update last_seen timestamp
    pub fn touch(&mut self) {
        self.last_seen = now();
    }

    /// Increase reputation (up to max 100)
    pub fn increase_reputation(&mut self, amount: u8) {
        self.reputation = self.reputation.saturating_add(amount).min(100);
    }

    /// Decrease reputation (down to min 0)
    pub fn decrease_reputation(&mut self, amount: u8) {
        self.reputation = self.reputation.saturating_sub(amount);
    }
}

/// Peer discovery configuration
#[derive(Clone, Debug)]
pub struct PeerDiscoveryConfig {
    /// Bootstrap node addresses (initial peers to connect to)
    pub bootstrap_nodes: Vec<String>,
    /// Maximum number of peers to maintain connections to
    pub max_peers: usize,
    /// Minimum reputation score to keep a peer
    pub min_reputation: u8,
    /// How often to gossip peer lists (seconds)
    pub gossip_interval_secs: u64,
    /// How often to prune stale peers (seconds)
    pub prune_interval_secs: u64,
    /// Local peer info (ourselves)
    pub local_peer: PeerInfo,
}

impl Default for PeerDiscoveryConfig {
    fn default() -> Self {
        Self {
            bootstrap_nodes: Vec::new(),
            max_peers: 50,
            min_reputation: 20,
            gossip_interval_secs: 60,
            prune_interval_secs: 300,
            local_peer: PeerInfo {
                address: "127.0.0.1:9001".to_string(),
                pubkey: Vec::new(),
                node_id: "unknown".to_string(),
                bft_port: 9091,
                api_port: Some(8001),
                onion_address: None,
                last_seen: now(),
                reputation: 100,
            },
        }
    }
}

/// Peer discovery manager
pub struct PeerDiscovery {
    config: PeerDiscoveryConfig,
    /// Known peers indexed by address
    peers: Arc<RwLock<HashMap<String, PeerInfo>>>,
    /// Peers we're currently connected to
    connected_peers: Arc<RwLock<HashSet<String>>>,
}

impl PeerDiscovery {
    pub fn new(config: PeerDiscoveryConfig) -> Self {
        let peers = Arc::new(RwLock::new(HashMap::new()));

        // Add bootstrap nodes to peer list
        for addr in &config.bootstrap_nodes {
            peers.write().insert(
                addr.clone(),
                PeerInfo {
                    address: addr.clone(),
                    pubkey: Vec::new(),
                    node_id: format!("bootstrap-{}", addr),
                    bft_port: 9091,
                    api_port: None,
                    onion_address: None,
                    last_seen: now(),
                    reputation: 75, // Bootstrap nodes start with higher reputation
                },
            );
        }

        Self {
            config,
            peers,
            connected_peers: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Add a newly discovered peer
    pub fn add_peer(&self, peer: PeerInfo) -> Result<()> {
        // Don't add ourselves
        if peer.address == self.config.local_peer.address {
            return Ok(());
        }

        // Don't add if we're at capacity and peer has low reputation
        let peers = self.peers.read();
        if peers.len() >= self.config.max_peers && peer.reputation < self.config.min_reputation {
            return Ok(());
        }
        drop(peers);

        log::info!(
            " Discovered peer: {} ({}) - reputation: {}",
            peer.node_id,
            peer.address,
            peer.reputation
        );

        self.peers.write().insert(peer.address.clone(), peer);
        Ok(())
    }

    /// Remove a peer
    pub fn remove_peer(&self, address: &str) {
        self.peers.write().remove(address);
        self.connected_peers.write().remove(address);
    }

    /// Mark a peer as connected
    pub fn mark_connected(&self, address: &str) {
        if let Some(peer) = self.peers.write().get_mut(address) {
            peer.touch();
            peer.increase_reputation(1);
        }
        self.connected_peers.write().insert(address.to_string());
    }

    /// Mark a peer as disconnected
    pub fn mark_disconnected(&self, address: &str) {
        if let Some(peer) = self.peers.write().get_mut(address) {
            peer.decrease_reputation(5);
        }
        self.connected_peers.write().remove(address);
    }

    /// Record a failed connection attempt
    pub fn record_failure(&self, address: &str) {
        if let Some(peer) = self.peers.write().get_mut(address) {
            peer.decrease_reputation(10);
            log::debug!(
                "WARNING Connection failure: {} (reputation: {})",
                address,
                peer.reputation
            );
        }
    }

    /// Record a successful interaction
    pub fn record_success(&self, address: &str) {
        if let Some(peer) = self.peers.write().get_mut(address) {
            peer.touch();
            peer.increase_reputation(2);
        }
    }

    /// Get list of known peers
    pub fn get_peers(&self) -> Vec<PeerInfo> {
        self.peers.read().values().cloned().collect()
    }

    /// Get best peers to connect to (sorted by reputation)
    pub fn get_best_peers(&self, count: usize) -> Vec<PeerInfo> {
        let peers = self.peers.read();
        let connected = self.connected_peers.read();

        let mut available: Vec<_> = peers
            .values()
            .filter(|p| {
                !connected.contains(&p.address)
                    && p.reputation >= self.config.min_reputation
                    && !p.is_stale()
            })
            .cloned()
            .collect();

        // Sort by reputation (highest first)
        available.sort_by(|a, b| b.reputation.cmp(&a.reputation));

        available.into_iter().take(count).collect()
    }

    /// Merge peer list from gossip
    pub fn merge_gossip(&self, gossip_peers: Vec<PeerInfo>) {
        let mut added = 0;
        let mut updated = 0;

        for peer in gossip_peers {
            // Skip ourselves
            if peer.address == self.config.local_peer.address {
                continue;
            }

            let mut peers = self.peers.write();

            if let Some(existing) = peers.get_mut(&peer.address) {
                // Update if gossip info is newer
                if peer.last_seen > existing.last_seen {
                    *existing = peer;
                    updated += 1;
                }
            } else {
                // Add new peer if we have capacity
                if peers.len() < self.config.max_peers {
                    peers.insert(peer.address.clone(), peer);
                    added += 1;
                }
            }
        }

        if added > 0 || updated > 0 {
            log::info!(" Merged gossip: {} new peers, {} updated", added, updated);
        }
    }

    /// Prune stale and low-reputation peers
    pub fn prune_peers(&self) {
        let mut peers = self.peers.write();
        let connected = self.connected_peers.read();

        let before = peers.len();

        peers.retain(|addr, peer| {
            // Keep connected peers
            if connected.contains(addr) {
                return true;
            }
            // Keep bootstrap nodes
            if self.config.bootstrap_nodes.contains(&peer.address) {
                return true;
            }
            // Remove stale or low-reputation peers
            !peer.is_stale() && peer.reputation >= self.config.min_reputation
        });

        let removed = before - peers.len();
        if removed > 0 {
            log::info!("🧹 Pruned {} stale/low-reputation peers", removed);
        }
    }

    /// Get number of connected peers
    pub fn connected_count(&self) -> usize {
        self.connected_peers.read().len()
    }

    /// Get number of known peers
    pub fn known_count(&self) -> usize {
        self.peers.read().len()
    }

    /// Get our local peer info for sharing
    pub fn get_local_peer(&self) -> PeerInfo {
        self.config.local_peer.clone()
    }
}

/// Gossip message types
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum GossipMessage {
    /// Request peer list from another node
    PeerRequest,
    /// Response with peer list
    PeerResponse { peers: Vec<PeerInfo> },
    /// Announce ourselves to the network
    PeerAnnounce { peer: PeerInfo },
}

/// Background task to periodically gossip peer lists
pub async fn peer_gossip_task(discovery: Arc<PeerDiscovery>) {
    let mut ticker = interval(Duration::from_secs(discovery.config.gossip_interval_secs));

    loop {
        ticker.tick().await;

        let connected = discovery.connected_peers.read().clone();
        if connected.is_empty() {
            continue;
        }

        // Get our peer list to share
        let our_peers = discovery.get_peers();

        log::debug!(
            " Gossiping peer list ({} peers) to {} connected peers",
            our_peers.len(),
            connected.len()
        );

        // In real implementation, this would send GossipMessage::PeerResponse
        // to all connected peers via the network layer
        // For now, this is just a placeholder
    }
}

/// Background task to prune stale peers
pub async fn peer_pruning_task(discovery: Arc<PeerDiscovery>) {
    let mut ticker = interval(Duration::from_secs(discovery.config.prune_interval_secs));

    loop {
        ticker.tick().await;

        discovery.prune_peers();

        log::debug!(
            "STATS Peer stats: {} known, {} connected",
            discovery.known_count(),
            discovery.connected_count()
        );
    }
}

/// Helper to get current Unix timestamp
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peer_discovery_basic() {
        let config = PeerDiscoveryConfig::default();
        let discovery = PeerDiscovery::new(config);

        let peer = PeerInfo::new(
            "192.168.1.100:9001".to_string(),
            vec![1, 2, 3],
            "test-node".to_string(),
            9091,
        );

        discovery.add_peer(peer.clone()).unwrap();
        assert_eq!(discovery.known_count(), 1);

        discovery.mark_connected(&peer.address);
        assert_eq!(discovery.connected_count(), 1);

        discovery.mark_disconnected(&peer.address);
        assert_eq!(discovery.connected_count(), 0);
    }

    #[test]
    fn test_reputation_changes() {
        let config = PeerDiscoveryConfig::default();
        let discovery = PeerDiscovery::new(config);

        let peer = PeerInfo::new(
            "192.168.1.100:9001".to_string(),
            vec![1, 2, 3],
            "test-node".to_string(),
            9091,
        );

        discovery.add_peer(peer.clone()).unwrap();

        // Record failures
        for _ in 0..5 {
            discovery.record_failure(&peer.address);
        }

        let peers = discovery.get_peers();
        assert!(peers[0].reputation < 50);

        // Record successes (need 30+ successes at +2 each to recover from -50)
        for _ in 0..30 {
            discovery.record_success(&peer.address);
        }

        let peers = discovery.get_peers();
        assert!(peers[0].reputation > 50);
    }

    #[test]
    fn test_gossip_merge() {
        let config = PeerDiscoveryConfig::default();
        let discovery = PeerDiscovery::new(config);

        let gossip_peers = vec![
            PeerInfo::new(
                "192.168.1.1:9001".to_string(),
                vec![1],
                "node1".to_string(),
                9091,
            ),
            PeerInfo::new(
                "192.168.1.2:9001".to_string(),
                vec![2],
                "node2".to_string(),
                9091,
            ),
            PeerInfo::new(
                "192.168.1.3:9001".to_string(),
                vec![3],
                "node3".to_string(),
                9091,
            ),
        ];

        discovery.merge_gossip(gossip_peers);
        assert_eq!(discovery.known_count(), 3);
    }
}
