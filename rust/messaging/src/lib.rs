pub mod encryption;
pub mod lamport_clock;
pub mod message;
pub mod queue;

pub use encryption::{E2EEncryption, EncryptionConfig};
pub use lamport_clock::LamportClock;
pub use message::{Message, MessageStatus};
pub use queue::{MessageQueue, QueueEntry};

/// Main messaging service
pub struct MessagingService {
    queue: MessageQueue,
    #[allow(dead_code)]
    crypto: E2EEncryption,
    clock: LamportClock,
}

impl MessagingService {
    pub fn new(crypto: E2EEncryption) -> Self {
        Self {
            queue: MessageQueue::new(),
            crypto,
            clock: LamportClock::new(),
        }
    }

    pub fn send_message(&mut self, peer_id: String, text: String) -> Result<String, String> {
        let timestamp = self.clock.increment();
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

    pub fn get_queue_size(&self) -> usize {
        self.queue.size()
    }

    pub fn get_pending_messages(&self) -> Vec<Message> {
        self.queue.get_pending()
    }

    pub fn mark_delivered(&mut self, message_id: &str) -> Result<(), String> {
        self.queue.mark_delivered(message_id)
    }
}
