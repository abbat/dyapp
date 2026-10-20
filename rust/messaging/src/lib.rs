//! Message records, shared logical time, an in-memory plaintext queue and MLS groups.

/// Hybrid logical timestamps and deterministic ordering.
pub mod hlc;
/// Serializable message fields and caller-assigned status.
pub mod message;
/// End-to-end encrypted groups over MLS.
pub mod mls;
/// In-memory FIFO and retry bookkeeping.
pub mod queue;

pub use hlc::{HybridClock, Timestamp};
pub use message::{Message, MessageStatus};
pub use queue::{MessageQueue, QueueEntry};

/// Main messaging service: an empty queue and clock by default.
#[derive(Default)]
pub struct MessagingService {
    queue: MessageQueue,
    clock: HybridClock,
}

impl MessagingService {
    /// Queues plaintext with a fresh UUID and sender `self_id`; does not encrypt or send.
    pub fn send_message(&mut self, peer_id: String, text: String) -> Result<String, String> {
        let timestamp = self.clock.now();
        let message_id = format!("msg-{}", uuid::Uuid::new_v4());

        let message = Message::new(
            message_id.clone(),
            "self_id".to_string(),
            peer_id,
            text,
            timestamp,
        );

        self.queue.enqueue(message)?;
        Ok(message_id)
    }

    /// Counts all retained queue entries, including delivered ones not yet removed.
    pub fn get_queue_size(&self) -> usize {
        self.queue.size()
    }

    /// Clones messages currently marked pending.
    pub fn get_pending_messages(&self) -> Vec<Message> {
        self.queue.get_pending()
    }

    /// Marks the first matching queued message delivered, or returns a lookup error.
    pub fn mark_delivered(&mut self, message_id: &str) -> Result<(), String> {
        self.queue.mark_delivered(message_id)
    }
}
