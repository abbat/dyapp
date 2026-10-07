use crate::peer::PeerId;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Authenticated,
    Failed,
}

#[derive(Clone, Debug)]
pub struct PeerConnection {
    pub peer_id: PeerId,
    pub state: ConnectionState,
    pub created_at: i64,
    pub last_activity: i64,
    pub messages_sent: u64,
    pub messages_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
}

impl PeerConnection {
    pub fn new(peer_id: PeerId) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        Self {
            peer_id,
            state: ConnectionState::Disconnected,
            created_at: now,
            last_activity: now,
            messages_sent: 0,
            messages_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
        }
    }

    pub fn is_active(&self) -> bool {
        self.state == ConnectionState::Connected || self.state == ConnectionState::Authenticated
    }

    pub fn set_state(&mut self, state: ConnectionState) {
        self.state = state;
        self.update_activity();
    }

    pub fn record_message_sent(&mut self, bytes: u64) {
        self.messages_sent += 1;
        self.bytes_sent += bytes;
        self.update_activity();
    }

    pub fn record_message_received(&mut self, bytes: u64) {
        self.messages_received += 1;
        self.bytes_received += bytes;
        self.update_activity();
    }

    pub fn update_activity(&mut self) {
        self.last_activity = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
    }

    pub fn is_idle(&self, timeout_ms: i64) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        (now - self.last_activity) >= timeout_ms
    }

    pub fn connection_duration_ms(&self) -> i64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        now - self.created_at
    }

    pub fn bandwidth_up_kbps(&self) -> f64 {
        let duration_ms = self.connection_duration_ms();
        if duration_ms > 0 {
            (self.bytes_sent as f64 * 8.0) / (duration_ms as f64)
        } else {
            0.0
        }
    }

    pub fn bandwidth_down_kbps(&self) -> f64 {
        let duration_ms = self.connection_duration_ms();
        if duration_ms > 0 {
            (self.bytes_received as f64 * 8.0) / (duration_ms as f64)
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_creation() {
        let conn = PeerConnection::new("peer1".to_string());
        assert_eq!(conn.state, ConnectionState::Disconnected);
        assert_eq!(conn.messages_sent, 0);
    }

    #[test]
    fn test_connection_state_transition() {
        let mut conn = PeerConnection::new("peer1".to_string());
        assert!(!conn.is_active());

        conn.set_state(ConnectionState::Connected);
        assert!(conn.is_active());

        conn.set_state(ConnectionState::Failed);
        assert!(!conn.is_active());
    }

    #[test]
    fn test_record_messages() {
        let mut conn = PeerConnection::new("peer1".to_string());
        conn.record_message_sent(100);
        conn.record_message_sent(200);

        assert_eq!(conn.messages_sent, 2);
        assert_eq!(conn.bytes_sent, 300);
    }

    #[test]
    fn test_idle_detection() {
        let conn = PeerConnection::new("peer1".to_string());
        assert!(!conn.is_idle(100)); // Should not be idle (just created)
        assert!(conn.is_idle(0)); // But idle with 0 timeout
    }

    #[test]
    fn test_duration() {
        let conn = PeerConnection::new("peer1".to_string());
        let duration = conn.connection_duration_ms();
        assert!(duration >= 0);
    }
}
