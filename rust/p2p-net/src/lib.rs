pub mod connection;
pub mod discovery;
pub mod peer;
pub mod transport;

pub use connection::{ConnectionState, PeerConnection};
pub use discovery::{DiscoveryStrategy, PeerDiscovery};
pub use peer::{Peer, PeerId};
pub use transport::{QuicTransport, TransportConfig};

/// Main P2P network manager
pub struct P2PNetwork {
    #[allow(dead_code)]
    config: TransportConfig,
    discovery: PeerDiscovery,
    connections: std::collections::HashMap<PeerId, PeerConnection>,
}

impl P2PNetwork {
    pub fn new(config: TransportConfig) -> Self {
        Self {
            config,
            discovery: PeerDiscovery::new(),
            connections: std::collections::HashMap::new(),
        }
    }

    pub fn add_peer(&mut self, peer: Peer) {
        self.discovery.add_peer(peer);
    }

    pub fn discover_peers(&self) -> Vec<Peer> {
        self.discovery.get_peers()
    }

    pub async fn connect_to_peer(&mut self, _peer_id: &PeerId) -> Result<(), String> {
        // TODO: Implement connection logic
        Ok(())
    }

    pub fn get_connection(&self, peer_id: &PeerId) -> Option<&PeerConnection> {
        self.connections.get(peer_id)
    }
}
