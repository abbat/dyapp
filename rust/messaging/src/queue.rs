use crate::message::{Message, MessageStatus};
use std::collections::VecDeque;

#[derive(Clone, Debug)]
pub struct QueueEntry {
    pub message: Message,
    pub retry_count: u32,
    pub next_retry_at: i64,
}

/// Offline message queue with retry logic
pub struct MessageQueue {
    entries: VecDeque<QueueEntry>,
    max_retries: u32,
}

impl MessageQueue {
    pub fn new() -> Self {
        Self {
            entries: VecDeque::new(),
            max_retries: 5,
        }
    }

    pub fn enqueue(&mut self, message: Message) -> Result<(), String> {
        let entry = QueueEntry {
            message,
            retry_count: 0,
            next_retry_at: 0,
        };
        self.entries.push_back(entry);
        Ok(())
    }

    pub fn dequeue(&mut self) -> Option<QueueEntry> {
        self.entries.pop_front()
    }

    pub fn peek(&self) -> Option<&QueueEntry> {
        self.entries.front()
    }

    pub fn size(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get_pending(&self) -> Vec<Message> {
        self.entries
            .iter()
            .filter(|e| e.message.is_pending())
            .map(|e| e.message.clone())
            .collect()
    }

    pub fn mark_delivered(&mut self, message_id: &str) -> Result<(), String> {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.message.id == message_id) {
            entry.message.mark_delivered();
            Ok(())
        } else {
            Err(format!("Message {} not found", message_id))
        }
    }

    pub fn retry_failed(&mut self) -> Vec<QueueEntry> {
        let mut retryable = Vec::new();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        for entry in self.entries.iter_mut() {
            if entry.message.status == MessageStatus::Failed
                && entry.retry_count < self.max_retries
                && entry.next_retry_at <= now
            {
                entry.retry_count += 1;
                entry.message.status = MessageStatus::Pending;
                entry.next_retry_at = now + (1000 * (2_i64.pow(entry.retry_count - 1)));
                retryable.push(entry.clone());
            }
        }

        retryable
    }

    pub fn remove_delivered(&mut self) {
        self.entries.retain(|e| {
            !matches!(
                e.message.status,
                MessageStatus::Delivered | MessageStatus::Read
            )
        });
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

impl Default for MessageQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_queue_enqueue_dequeue() {
        let mut queue = MessageQueue::new();
        let msg = Message::new(
            "msg1".to_string(),
            "sender".to_string(),
            "recipient".to_string(),
            "hello".to_string(),
            1,
        );

        queue.enqueue(msg.clone()).ok();
        assert_eq!(queue.size(), 1);

        let entry = queue.dequeue();
        assert!(entry.is_some());
        assert_eq!(queue.size(), 0);
    }

    #[test]
    fn test_queue_pending_filter() {
        let mut queue = MessageQueue::new();
        let msg1 = Message::new(
            "msg1".to_string(),
            "sender".to_string(),
            "recipient".to_string(),
            "hello".to_string(),
            1,
        );

        let mut msg2 = Message::new(
            "msg2".to_string(),
            "sender".to_string(),
            "recipient".to_string(),
            "world".to_string(),
            2,
        );
        msg2.mark_delivered();

        queue.enqueue(msg1).ok();
        queue.enqueue(msg2).ok();

        let pending = queue.get_pending();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, "msg1");
    }

    #[test]
    fn test_queue_mark_delivered() {
        let mut queue = MessageQueue::new();
        let msg = Message::new(
            "msg1".to_string(),
            "sender".to_string(),
            "recipient".to_string(),
            "hello".to_string(),
            1,
        );

        queue.enqueue(msg).ok();
        queue.mark_delivered("msg1").ok();

        let entry = queue.peek().unwrap();
        assert_eq!(entry.message.status, MessageStatus::Delivered);
    }

    #[test]
    fn test_queue_remove_delivered() {
        let mut queue = MessageQueue::new();
        let mut msg1 = Message::new(
            "msg1".to_string(),
            "sender".to_string(),
            "recipient".to_string(),
            "hello".to_string(),
            1,
        );
        msg1.mark_delivered();

        let msg2 = Message::new(
            "msg2".to_string(),
            "sender".to_string(),
            "recipient".to_string(),
            "world".to_string(),
            2,
        );

        queue.enqueue(msg1).ok();
        queue.enqueue(msg2).ok();
        assert_eq!(queue.size(), 2);

        queue.remove_delivered();
        assert_eq!(queue.size(), 1);
    }
}
