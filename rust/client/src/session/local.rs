//! Chat housekeeping on this device only, for an app that keeps the Store inside the session.

use super::{Error, Session};

impl Session {
    /// Clear one conversation from this device, retaining marks against later redelivery.
    /// No message is sent to peers or nodes; the chat row and relationship state remain.
    pub fn clear_chat(&mut self, group_id: &[u8]) -> Result<usize, Error> {
        Ok(self.store.clear_chat(group_id)?)
    }

    /// [`crate::store::Store::clear_chat_pair`]: every group with `peer_id`.
    pub fn clear_chat_pair(&mut self, peer_id: &str) -> Result<usize, Error> {
        Ok(self.store.clear_chat_pair(peer_id)?)
    }

    /// [`crate::store::Store::mark_chat_read`]: no read receipt is sent.
    pub fn mark_chat_read(&mut self, peer_id: &str) -> Result<usize, Error> {
        Ok(self.store.mark_chat_read(peer_id)?)
    }
}
