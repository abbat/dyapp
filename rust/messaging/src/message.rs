//! Serializable plaintext messages with optional ciphertext and signature fields.

use crate::hlc::Timestamp;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Caller-assigned local status; these variants do not verify remote receipts.
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum MessageStatus {
    /// Awaiting a sending attempt.
    Pending,
    /// Marked sent by the caller.
    Sent,
    /// Marked delivered by the caller.
    Delivered,
    /// Marked read by the caller.
    Read,
    /// Marked failed, potentially eligible for local retry bookkeeping.
    Failed,
}

/// Message record that retains plaintext even when encrypted fields are attached.
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[derive(Clone, Serialize, Deserialize)]
pub struct Message {
    /// Caller-supplied message identifier.
    pub id: String,
    /// Caller-supplied sender label; not authenticated by this type.
    pub sender_id: String,
    /// Caller-supplied recipient label.
    pub recipient_id: String,
    /// Plaintext body, included by the derived serializer.
    pub text: String,
    /// Hybrid logical clock; the wire name predates the switch from a Lamport clock.
    #[serde(rename = "lamport_timestamp")]
    pub timestamp: Timestamp,
    /// Creation time in wall-clock milliseconds since the Unix epoch.
    pub created_at: i64,
    /// Local delivery status, independent of the optional encrypted fields.
    pub status: MessageStatus,
    /// Attached ciphertext bytes; this type does not encrypt or verify them.
    pub encrypted_payload: Option<Vec<u8>>,
    /// Attached signature bytes; this type does not verify them.
    pub signature: Option<Vec<u8>>,
}

/// Leaves out the text and both parties.
impl std::fmt::Debug for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Message")
            .field("id", &self.id)
            .field("timestamp", &self.timestamp)
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

impl Message {
    /// Creates a pending plaintext record using the current wall clock.
    ///
    /// # Panics
    /// Panics if the system clock is before the Unix epoch.
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

    /// Attaches bytes without verification or removing the plaintext body.
    pub fn set_encrypted(&mut self, payload: Vec<u8>, signature: Vec<u8>) {
        self.encrypted_payload = Some(payload);
        self.signature = Some(signature);
    }

    /// Assigns the local sent status without validating a transition.
    pub fn mark_sent(&mut self) {
        self.status = MessageStatus::Sent;
    }

    /// Assigns the local delivered status without validating a transition.
    pub fn mark_delivered(&mut self) {
        self.status = MessageStatus::Delivered;
    }

    /// Assigns the local read status without validating a transition.
    pub fn mark_read(&mut self) {
        self.status = MessageStatus::Read;
    }

    /// Assigns the local failed status without validating a transition.
    pub fn mark_failed(&mut self) {
        self.status = MessageStatus::Failed;
    }

    /// Whether the local status is pending.
    pub fn is_pending(&self) -> bool {
        self.status == MessageStatus::Pending
    }

    /// Whether the local status is delivered or read.
    pub fn is_delivered(&self) -> bool {
        matches!(self.status, MessageStatus::Delivered | MessageStatus::Read)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_shows_no_text_or_parties() {
        let msg = Message::new(
            "msg1".to_string(),
            "alice".to_string(),
            "bob".to_string(),
            "secret text".to_string(),
            1.into(),
        );
        let debug = format!("{msg:?}");
        assert!(debug.contains("msg1"), "{debug}");
        for hidden in ["alice", "bob", "secret"] {
            assert!(!debug.contains(hidden), "{debug}");
        }
        let received = crate::mls::Received::Message {
            sender: "alice".into(),
            device: vec![7; 32],
            plaintext: b"secret".to_vec(),
        };
        assert_eq!(format!("{received:?}"), "Message { .. }");
    }

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
