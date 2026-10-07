use std::sync::{Arc, Mutex};

/// Lamport clock for causality tracking in distributed messages
pub struct LamportClock {
    counter: Arc<Mutex<u64>>,
}

impl LamportClock {
    pub fn new() -> Self {
        Self {
            counter: Arc::new(Mutex::new(0)),
        }
    }

    /// Increment clock and return new timestamp
    pub fn increment(&self) -> u64 {
        let mut c = self.counter.lock().unwrap();
        *c += 1;
        *c
    }

    /// Update clock when receiving remote timestamp
    pub fn observe(&self, remote_timestamp: u64) {
        let mut c = self.counter.lock().unwrap();
        *c = (*c).max(remote_timestamp) + 1;
    }

    /// Get current value without incrementing
    pub fn current(&self) -> u64 {
        *self.counter.lock().unwrap()
    }

    /// Reset to zero
    pub fn reset(&self) {
        let mut c = self.counter.lock().unwrap();
        *c = 0;
    }

    /// Merge two clocks
    pub fn merge(&self, other: &LamportClock) {
        let mut c = self.counter.lock().unwrap();
        let other_c = *other.counter.lock().unwrap();
        *c = (*c).max(other_c) + 1;
    }
}

impl Clone for LamportClock {
    fn clone(&self) -> Self {
        Self {
            counter: Arc::clone(&self.counter),
        }
    }
}

impl Default for LamportClock {
    fn default() -> Self {
        Self::new()
    }
}

/// Message ordering using Lamport timestamps
pub struct MessageOrdering {
    messages: Vec<(u64, String)>,
}

impl MessageOrdering {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
        }
    }

    /// Add message with Lamport timestamp
    pub fn add(&mut self, timestamp: u64, message_id: String) {
        self.messages.push((timestamp, message_id));
        self.messages.sort_by_key(|m| m.0);
    }

    /// Get ordered message IDs
    pub fn get_ordered(&self) -> Vec<String> {
        self.messages.iter().map(|m| m.1.clone()).collect()
    }

    /// Check if message is out of order
    pub fn is_out_of_order(&self, timestamp: u64, position: usize) -> bool {
        if position == 0 {
            return false;
        }
        if position >= self.messages.len() {
            return false;
        }
        self.messages[position - 1].0 > timestamp
    }
}

impl Default for MessageOrdering {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lamport_clock_increment() {
        let clock = LamportClock::new();
        assert_eq!(clock.increment(), 1);
        assert_eq!(clock.increment(), 2);
        assert_eq!(clock.increment(), 3);
    }

    #[test]
    fn test_lamport_clock_observe() {
        let clock = LamportClock::new();
        clock.increment(); // 1

        clock.observe(10);
        assert_eq!(clock.increment(), 12);
    }

    #[test]
    fn test_lamport_clock_current() {
        let clock = LamportClock::new();
        clock.increment();
        clock.increment();

        assert_eq!(clock.current(), 2);
    }

    #[test]
    fn test_message_ordering() {
        let mut ordering = MessageOrdering::new();
        ordering.add(1, "msg1".to_string());
        ordering.add(3, "msg3".to_string());
        ordering.add(2, "msg2".to_string());

        let ordered = ordering.get_ordered();
        assert_eq!(ordered, vec!["msg1", "msg2", "msg3"]);
    }

    #[test]
    fn test_out_of_order_detection() {
        let mut ordering = MessageOrdering::new();
        ordering.add(1, "msg1".to_string());
        ordering.add(3, "msg3".to_string());

        // Insert msg2 at position 1 (between msg1 and msg3)
        // But msg2 has timestamp 2, which is after msg1 (1) but before msg3 (3)
        assert!(!ordering.is_out_of_order(2, 1));
    }

    #[test]
    fn test_clock_clone_shared_state() {
        let clock1 = LamportClock::new();
        let clock2 = clock1.clone();

        clock1.increment();
        assert_eq!(clock2.current(), 1);
    }
}
