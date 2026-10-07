use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

pub type PeerId = String; // Public key hash

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Peer {
    pub id: PeerId,
    pub addresses: Vec<SocketAddr>,
    pub public_key: String,     // Hex-encoded Ed25519 public key
    pub verified: bool,         // TOFU verification status
    pub last_seen: Option<i64>, // Unix millis
    pub reputation: i32,        // Simple reputation score
}

impl Peer {
    pub fn new(id: PeerId, public_key: String) -> Self {
        Self {
            id,
            addresses: Vec::new(),
            public_key,
            verified: false,
            last_seen: None,
            reputation: 0,
        }
    }

    pub fn add_address(&mut self, addr: SocketAddr) {
        if !self.addresses.contains(&addr) {
            self.addresses.push(addr);
        }
    }

    pub fn mark_verified(&mut self) {
        self.verified = true;
    }

    pub fn update_last_seen(&mut self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        self.last_seen = Some(now);
    }

    pub fn increase_reputation(&mut self, amount: i32) {
        self.reputation = (self.reputation + amount).min(100);
    }

    pub fn decrease_reputation(&mut self, amount: i32) {
        self.reputation = (self.reputation - amount).max(-100);
    }

    pub fn is_reachable(&self) -> bool {
        !self.addresses.is_empty()
    }

    pub fn is_trusted(&self) -> bool {
        self.verified && self.reputation > -50
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peer_creation() {
        let peer = Peer::new("peer123".to_string(), "pubkey".to_string());
        assert_eq!(peer.id, "peer123");
        assert!(!peer.verified);
        assert_eq!(peer.reputation, 0);
    }

    #[test]
    fn test_peer_addresses() {
        let mut peer = Peer::new("peer123".to_string(), "pubkey".to_string());
        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        peer.add_address(addr);

        assert!(peer.is_reachable());
        assert_eq!(peer.addresses.len(), 1);
    }

    #[test]
    fn test_peer_reputation() {
        let mut peer = Peer::new("peer123".to_string(), "pubkey".to_string());
        peer.increase_reputation(30);
        assert_eq!(peer.reputation, 30);

        peer.decrease_reputation(40);
        assert_eq!(peer.reputation, -10);
    }

    #[test]
    fn test_peer_trusted() {
        let mut peer = Peer::new("peer123".to_string(), "pubkey".to_string());
        assert!(!peer.is_trusted());

        peer.mark_verified();
        peer.increase_reputation(30);
        assert!(peer.is_trusted());
    }
}
