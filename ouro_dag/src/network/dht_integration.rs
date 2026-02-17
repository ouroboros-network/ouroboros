// src/network/dht_integration.rs
// Integration of DHT with existing P2P network

use super::dht::{DhtPeer, Kademlia, NodeId};
use super::PeerEntry;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

/// DHT-enabled P2P network manager
pub struct DhtNetworkManager {
    /// Kademlia DHT
    dht: Arc<Kademlia>,
    /// Traditional peer store
    peer_store: Arc<Mutex<Vec<PeerEntry>>>,
}

impl DhtNetworkManager {
    /// Create new DHT network manager
    pub fn new(my_address: &str, peer_store: Arc<Mutex<Vec<PeerEntry>>>) -> Self {
        let my_id = Kademlia::node_id_from_address(my_address);
        let dht = Arc::new(Kademlia::new(my_id));

        Self { dht, peer_store }
    }

    /// Bootstrap DHT from any peer (zero-config discovery)
    pub async fn bootstrap(&self, bootstrap_peer: &str) -> Result<(), String> {
        tracing::info!("DHT: Bootstrapping from {}", bootstrap_peer);

        // Bootstrap DHT
        self.dht.bootstrap(bootstrap_peer).await?;

        // Get discovered peers from DHT
        let dht_peers = self.dht.get_all_peers().await;

        tracing::info!("DHT: Discovered {} peers", dht_peers.len());

        // Add to traditional peer store
        let mut store = self.peer_store.lock().await;
        for dht_peer in dht_peers {
            if !store.iter().any(|p| p.addr == dht_peer.address) {
                store.push(PeerEntry::new(dht_peer.address));
            }
        }

        Ok(())
    }

    /// Automatic peer discovery (no bootstrap addresses needed!)
    pub async fn discover_peers(&self, count: usize) -> Result<Vec<String>, String> {
        // Find closest peers to our ID
        let my_id = self.dht.my_id();
        let all_peers = self.dht.find_closest(&my_id, count + 1).await; // Get extra to account for self

        // Filter out ourselves from the peer list
        let peers: Vec<_> = all_peers
            .into_iter()
            .filter(|p| p.node_id != my_id)
            .take(count)
            .collect();

        let addresses: Vec<String> = peers.iter().map(|p| p.address.clone()).collect();

        // Add to peer store
        let mut store = self.peer_store.lock().await;
        for peer in peers {
            if !store.iter().any(|p| p.addr == peer.address) {
                store.push(PeerEntry::new(peer.address));
            }
        }

        tracing::info!("DHT: Discovered {} new peers", addresses.len());

        Ok(addresses)
    }

    /// Announce ourselves to the network
    pub async fn announce(&self, my_address: &str) -> Result<(), String> {
        let my_id = Kademlia::node_id_from_address(my_address);

        let peer = DhtPeer {
            node_id: my_id,
            address: my_address.to_string(),
            last_seen: current_unix(),
        };

        self.dht.add_peer(peer).await;

        tracing::info!("DHT: Announced {} to network", my_address);

        Ok(())
    }

    /// Periodic DHT refresh (background task)
    pub async fn refresh_loop(&self) {
        let mut interval = tokio::time::interval(Duration::from_secs(600)); // 10 minutes

        loop {
            interval.tick().await;

            tracing::debug!("DHT: Refreshing routing table");

            // Refresh buckets
            self.dht.refresh_buckets().await;

            // Discover new peers
            if let Ok(peers) = self.discover_peers(20).await {
                tracing::info!("DHT: Refresh discovered {} peers", peers.len());
            }
        }
    }

    /// Convert PeerEntry to DhtPeer
    pub async fn sync_peer_to_dht(&self, peer: &PeerEntry) {
        let node_id = Kademlia::node_id_from_address(&peer.addr);

        let dht_peer = DhtPeer {
            node_id,
            address: peer.addr.clone(),
            last_seen: peer.last_seen_unix.unwrap_or(current_unix()),
        };

        self.dht.add_peer(dht_peer).await;
    }

    /// Full sync: add all traditional peers to DHT
    pub async fn sync_all_peers(&self) {
        let store = self.peer_store.lock().await;
        let peers = store.clone();
        drop(store);

        let peer_count = peers.len();
        for peer in peers {
            self.sync_peer_to_dht(&peer).await;
        }

        tracing::info!("DHT: Synced {} peers to DHT", peer_count);
    }

    /// Find peer by address using DHT
    pub async fn find_peer(&self, address: &str) -> Option<DhtPeer> {
        let node_id = Kademlia::node_id_from_address(address);
        let peers = self.dht.find_closest(&node_id, 1).await;

        peers.into_iter().next()
    }
}

fn current_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Initialize DHT for existing P2P network
pub async fn init_dht(
    my_address: &str,
    peer_store: Arc<Mutex<Vec<PeerEntry>>>,
    bootstrap_peers: &[String],
) -> Result<Arc<DhtNetworkManager>, String> {
    let manager = Arc::new(DhtNetworkManager::new(my_address, peer_store));

    // Sync existing peers to DHT
    manager.sync_all_peers().await;

    // Bootstrap from any available peer
    if !bootstrap_peers.is_empty() {
        for bootstrap in bootstrap_peers {
            if let Ok(_) = manager.bootstrap(bootstrap).await {
                tracing::info!("DHT: Successfully bootstrapped from {}", bootstrap);
                break;
            }
        }
    }

    // Announce ourselves
    manager.announce(my_address).await?;

    // Start refresh loop in background
    let manager_clone = manager.clone();
    tokio::spawn(async move {
        manager_clone.refresh_loop().await;
    });

    tracing::info!("DHT: Initialized and running");

    Ok(manager)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_dht_network_manager() {
        let peer_store = Arc::new(Mutex::new(Vec::new()));
        let manager = DhtNetworkManager::new("127.0.0.1:9001", peer_store);

        // Announce ourselves
        manager.announce("127.0.0.1:9001").await.unwrap();

        // Discover peers (will be empty in test)
        let peers = manager.discover_peers(5).await.unwrap();
        assert_eq!(peers.len(), 0); // No peers in test environment
    }

    #[tokio::test]
    async fn test_peer_sync() {
        let peer_store = Arc::new(Mutex::new(vec![
            PeerEntry::new("127.0.0.1:9002".to_string()),
            PeerEntry::new("127.0.0.1:9003".to_string()),
        ]));

        let manager = DhtNetworkManager::new("127.0.0.1:9001", peer_store);

        manager.sync_all_peers().await;

        let dht_peers = manager.dht.get_all_peers().await;
        assert_eq!(dht_peers.len(), 2);
    }
}
