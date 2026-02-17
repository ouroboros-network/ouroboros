// src/network/dht.rs
// DHT/Kademlia for zero-config peer discovery

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

/// Kademlia node ID (256-bit)
pub type NodeId = [u8; 32];

/// K-bucket size (number of peers per bucket)
const K: usize = 20;

/// Alpha parameter (parallel queries)
const ALPHA: usize = 3;

/// DHT peer info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DhtPeer {
    pub node_id: NodeId,
    pub address: String,
    pub last_seen: u64,
}

/// K-bucket (stores peers at specific distance)
#[derive(Debug, Clone)]
struct KBucket {
    peers: VecDeque<DhtPeer>,
    last_updated: Instant,
}

impl KBucket {
    fn new() -> Self {
        Self {
            peers: VecDeque::new(),
            last_updated: Instant::now(),
        }
    }

    fn add(&mut self, peer: DhtPeer) -> bool {
        // Remove if already exists
        self.peers.retain(|p| p.node_id != peer.node_id);

        if self.peers.len() < K {
            self.peers.push_back(peer);
            self.last_updated = Instant::now();
            true
        } else {
            // Bucket full, try to replace stale peer
            if let Some(stale) = self.peers.iter().position(|p| is_stale(p)) {
                self.peers.remove(stale);
                self.peers.push_back(peer);
                self.last_updated = Instant::now();
                true
            } else {
                false
            }
        }
    }

    fn get_all(&self) -> Vec<DhtPeer> {
        self.peers.iter().cloned().collect()
    }
}

/// Check if peer is stale (not seen in 15 minutes)
fn is_stale(peer: &DhtPeer) -> bool {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    now.saturating_sub(peer.last_seen) > 900
}

/// Kademlia DHT
pub struct Kademlia {
    /// Our node ID
    my_id: NodeId,
    /// Routing table (256 buckets for 256-bit node IDs)
    buckets: Vec<Arc<Mutex<KBucket>>>,
    /// Peer storage for quick lookup
    peers: Arc<Mutex<HashMap<NodeId, DhtPeer>>>,
}

impl Kademlia {
    /// Create new Kademlia DHT
    pub fn new(my_id: NodeId) -> Self {
        let buckets = (0..256)
            .map(|_| Arc::new(Mutex::new(KBucket::new())))
            .collect();

        Self {
            my_id,
            buckets,
            peers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Get our node ID
    pub fn my_id(&self) -> NodeId {
        self.my_id
    }

    /// Generate node ID from address
    pub fn node_id_from_address(address: &str) -> NodeId {
        let mut hasher = Sha256::new();
        hasher.update(address.as_bytes());
        let hash = hasher.finalize();

        let mut id = [0u8; 32];
        id.copy_from_slice(&hash[..]);
        id
    }

    /// Add peer to routing table
    pub async fn add_peer(&self, peer: DhtPeer) {
        let bucket_index = self.bucket_index(&peer.node_id);
        let bucket = self.buckets[bucket_index].clone();

        let mut b = bucket.lock().await;
        if b.add(peer.clone()) {
            let mut peers = self.peers.lock().await;
            peers.insert(peer.node_id, peer);
        }
    }

    /// Find closest peers to target
    pub async fn find_closest(&self, target: &NodeId, count: usize) -> Vec<DhtPeer> {
        let mut all_peers = Vec::new();

        // Get peers from relevant buckets
        let start_bucket = self.bucket_index(target);

        for offset in 0..256 {
            let bucket_idx = (start_bucket + offset) % 256;
            let bucket = self.buckets[bucket_idx].lock().await;
            all_peers.extend(bucket.get_all());

            if all_peers.len() >= count * 2 {
                break;
            }
        }

        // Sort by distance to target
        all_peers.sort_by_key(|p| xor_distance(&p.node_id, target));
        all_peers.truncate(count);
        all_peers
    }

    /// Bootstrap DHT from known peer
    pub async fn bootstrap(&self, bootstrap_peer: &str) -> Result<(), String> {
        let peer_id = Self::node_id_from_address(bootstrap_peer);

        let peer = DhtPeer {
            node_id: peer_id,
            address: bootstrap_peer.to_string(),
            last_seen: current_unix(),
        };

        self.add_peer(peer).await;

        // Find nodes close to our ID
        let _ = self.lookup_node(&self.my_id).await;

        Ok(())
    }

    /// Lookup node by ID (iterative)
    pub async fn lookup_node(&self, target: &NodeId) -> Vec<DhtPeer> {
        let mut queried = HashMap::new();
        let mut result = Vec::new();

        // Start with closest known peers
        let mut candidates = self.find_closest(target, K).await;

        while !candidates.is_empty() {
            // Query ALPHA peers in parallel
            let to_query: Vec<_> = candidates
                .iter()
                .filter(|p| !queried.contains_key(&p.node_id))
                .take(ALPHA)
                .cloned()
                .collect();

            if to_query.is_empty() {
                break;
            }

            for peer in to_query {
                queried.insert(peer.node_id, ());
                result.push(peer);
            }

            // Get new candidates from responses
            // TODO: Actually query remote peers via network
            candidates = self.find_closest(target, K).await;
        }

        result.sort_by_key(|p| xor_distance(&p.node_id, target));
        result.truncate(K);
        result
    }

    /// Get bucket index for node ID
    fn bucket_index(&self, node_id: &NodeId) -> usize {
        let distance = xor_distance(&self.my_id, node_id);

        // Find first non-zero bit (distance)
        for (i, &byte) in distance.iter().enumerate() {
            if byte != 0 {
                return 255 - (i * 8 + byte.leading_zeros() as usize);
            }
        }

        0
    }

    /// Get all known peers
    pub async fn get_all_peers(&self) -> Vec<DhtPeer> {
        let peers = self.peers.lock().await;
        peers.values().cloned().collect()
    }

    /// Periodic refresh of buckets
    pub async fn refresh_buckets(&self) {
        for i in 0..256 {
            let bucket = self.buckets[i].lock().await;

            // If bucket hasn't been updated in 1 hour, refresh it
            if bucket.last_updated.elapsed() > Duration::from_secs(3600) {
                drop(bucket);

                // Generate random ID in this bucket's range
                let mut random_id = self.my_id;
                random_id[i / 8] ^= 1 << (i % 8);

                // Lookup that ID to discover peers
                let _ = self.lookup_node(&random_id).await;
            }
        }
    }
}

/// XOR distance between two node IDs
fn xor_distance(a: &NodeId, b: &NodeId) -> NodeId {
    let mut result = [0u8; 32];
    for i in 0..32 {
        result[i] = a[i] ^ b[i];
    }
    result
}

/// Get current unix timestamp
fn current_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xor_distance() {
        let a = [1u8; 32];
        let b = [2u8; 32];
        let dist = xor_distance(&a, &b);

        assert_eq!(dist[0], 3); // 1 XOR 2 = 3
    }

    #[tokio::test]
    async fn test_add_peer() {
        let my_id = [0u8; 32];
        let dht = Kademlia::new(my_id);

        let peer = DhtPeer {
            node_id: [1u8; 32],
            address: "127.0.0.1:9001".to_string(),
            last_seen: current_unix(),
        };

        dht.add_peer(peer).await;

        let peers = dht.get_all_peers().await;
        assert_eq!(peers.len(), 1);
    }

    #[tokio::test]
    async fn test_find_closest() {
        let my_id = [0u8; 32];
        let dht = Kademlia::new(my_id);

        // Add several peers
        for i in 1..=10 {
            let mut peer_id = [0u8; 32];
            peer_id[0] = i;

            let peer = DhtPeer {
                node_id: peer_id,
                address: format!("127.0.0.1:{}", 9000 + (i as u16)),
                last_seen: current_unix(),
            };

            dht.add_peer(peer).await;
        }

        let target = [5u8; 32];
        let closest = dht.find_closest(&target, 3).await;

        assert_eq!(closest.len(), 3);
        // Should return peers closest to target
    }
}
