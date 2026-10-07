use crate::peer::{Peer, PeerId};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug)]
pub enum DiscoveryStrategy {
    /// Bootstrap server: query centralized index
    Bootstrap,
    /// Gossip: peer shares known peers
    Gossip,
    /// DHT: distributed hash table lookup
    DHT,
    /// Hybrid: try all strategies
    Hybrid,
}

/// Peer discovery manager
pub struct PeerDiscovery {
    peers: HashMap<PeerId, Peer>,
    #[allow(dead_code)]
    strategy: DiscoveryStrategy,
    bootstrap_servers: Vec<String>,
}

impl PeerDiscovery {
    pub fn new() -> Self {
        Self {
            peers: HashMap::new(),
            strategy: DiscoveryStrategy::Hybrid,
            bootstrap_servers: Vec::new(),
        }
    }

    pub fn add_bootstrap_server(&mut self, addr: String) {
        self.bootstrap_servers.push(addr);
    }

    pub fn add_peer(&mut self, peer: Peer) {
        self.peers.insert(peer.id.clone(), peer);
    }

    pub fn get_peer(&self, id: &PeerId) -> Option<&Peer> {
        self.peers.get(id)
    }

    pub fn get_peers(&self) -> Vec<Peer> {
        self.peers.values().cloned().collect()
    }

    pub fn get_reachable_peers(&self) -> Vec<Peer> {
        self.peers
            .values()
            .filter(|p| p.is_reachable())
            .cloned()
            .collect()
    }

    pub fn get_trusted_peers(&self) -> Vec<Peer> {
        self.peers
            .values()
            .filter(|p| p.is_trusted())
            .cloned()
            .collect()
    }

    pub fn get_peer_count(&self) -> usize {
        self.peers.len()
    }

    pub fn remove_peer(&mut self, id: &PeerId) -> Option<Peer> {
        self.peers.remove(id)
    }

    pub fn update_peer<F>(&mut self, id: &PeerId, f: F) -> bool
    where
        F: FnOnce(&mut Peer),
    {
        if let Some(peer) = self.peers.get_mut(id) {
            f(peer);
            true
        } else {
            false
        }
    }

    /// Gossip protocol: broadcast known peers to a peer
    pub fn get_peers_to_share(&self) -> Vec<Peer> {
        self.peers
            .values()
            .take(10) // Share up to 10 peers
            .cloned()
            .collect()
    }

    /// Query bootstrap server for peers matching criteria
    pub async fn query_bootstrap(&self, _query: &str) -> Result<Vec<Peer>, String> {
        // TODO: Implement bootstrap query
        Ok(Vec::new())
    }
}

impl Default for PeerDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discovery_creation() {
        let discovery = PeerDiscovery::new();
        assert_eq!(discovery.get_peer_count(), 0);
    }

    #[test]
    fn test_add_peer() {
        let mut discovery = PeerDiscovery::new();
        let peer = Peer::new("peer1".to_string(), "pubkey1".to_string());
        discovery.add_peer(peer);

        assert_eq!(discovery.get_peer_count(), 1);
        assert!(discovery.get_peer(&"peer1".to_string()).is_some());
    }

    #[test]
    fn test_filter_trusted_peers() {
        let mut discovery = PeerDiscovery::new();
        let mut peer1 = Peer::new("peer1".to_string(), "pubkey1".to_string());
        peer1.mark_verified();
        peer1.increase_reputation(50);

        let peer2 = Peer::new("peer2".to_string(), "pubkey2".to_string());

        discovery.add_peer(peer1);
        discovery.add_peer(peer2);

        let trusted = discovery.get_trusted_peers();
        assert_eq!(trusted.len(), 1);
        assert_eq!(trusted[0].id, "peer1");
    }

    #[test]
    fn test_remove_peer() {
        let mut discovery = PeerDiscovery::new();
        let peer = Peer::new("peer1".to_string(), "pubkey1".to_string());
        discovery.add_peer(peer);

        let removed = discovery.remove_peer(&"peer1".to_string());
        assert!(removed.is_some());
        assert_eq!(discovery.get_peer_count(), 0);
    }

    #[test]
    fn test_update_peer() {
        let mut discovery = PeerDiscovery::new();
        let peer = Peer::new("peer1".to_string(), "pubkey1".to_string());
        discovery.add_peer(peer);

        discovery.update_peer(&"peer1".to_string(), |p| {
            p.increase_reputation(50);
        });

        let updated = discovery.get_peer(&"peer1".to_string()).unwrap();
        assert_eq!(updated.reputation, 50);
    }
}
