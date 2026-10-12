use crate::hlc::Timestamp;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum MessageStatus {
    Pending,
    Sent,
    Delivered,
    Read,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub sender_id: String,
    pub recipient_id: String,
    pub text: String,
    /// Hybrid logical clock; the wire name predates the switch from a Lamport clock.
    #[serde(rename = "lamport_timestamp")]
    pub timestamp: Timestamp,
    pub created_at: i64,
    pub status: MessageStatus,
    pub encrypted_payload: Option<Vec<u8>>,
    pub signature: Option<Vec<u8>>,
}

impl Message {
    pub fn new(
        id: String,
        sender_id: String,
        recipient_id: String,
        text: String,
        timestamp: Timestamp,
    ) -> Self {
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        Self {
            id,
            sender_id,
            recipient_id,
            text,
            timestamp,
            created_at,
            status: MessageStatus::Pending,
            encrypted_payload: None,
            signature: None,
        }
    }

    pub fn set_encrypted(&mut self, payload: Vec<u8>, signature: Vec<u8>) {
        self.encrypted_payload = Some(payload);
        self.signature = Some(signature);
    }

    pub fn mark_sent(&mut self) {
        self.status = MessageStatus::Sent;
    }

    pub fn mark_delivered(&mut self) {
        self.status = MessageStatus::Delivered;
    }

    pub fn mark_read(&mut self) {
        self.status = MessageStatus::Read;
    }

    pub fn mark_failed(&mut self) {
        self.status = MessageStatus::Failed;
    }

    pub fn is_pending(&self) -> bool {
        self.status == MessageStatus::Pending
    }

    pub fn is_delivered(&self) -> bool {
        matches!(self.status, MessageStatus::Delivered | MessageStatus::Read)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_creation() {
        let msg = Message::new(
            "msg1".to_string(),
            "sender".to_string(),
            "recipient".to_string(),
            "hello".to_string(),
            1.into(),
        );

        assert_eq!(msg.id, "msg1");
        assert_eq!(msg.status, MessageStatus::Pending);
        assert!(msg.is_pending());
    }

    #[test]
    fn test_message_lifecycle() {
        let mut msg = Message::new(
            "msg1".to_string(),
            "sender".to_string(),
            "recipient".to_string(),
            "hello".to_string(),
            1.into(),
        );

        msg.mark_sent();
        assert_eq!(msg.status, MessageStatus::Sent);

        msg.mark_delivered();
        assert!(msg.is_delivered());

        msg.mark_read();
        assert_eq!(msg.status, MessageStatus::Read);
    }

    #[test]
    fn test_message_encryption() {
        let mut msg = Message::new(
            "msg1".to_string(),
            "sender".to_string(),
            "recipient".to_string(),
            "hello".to_string(),
            1.into(),
        );

        msg.set_encrypted(vec![1, 2, 3], vec![4, 5, 6]);

        assert!(msg.encrypted_payload.is_some());
        assert!(msg.signature.is_some());
    }
}
