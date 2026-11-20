//! Read-only projection of common presence policy; it never announces activity.
use super::{failed, ChatSession};
use crate::CoreError;

#[uniffi::export]
impl ChatSession {
    /// Current chat peers whose authenticated match presence is still fresh.
    ///
    /// Use only for presentation. Foreground sending is a separate operation; mail passes,
    /// reads and expiry refreshes must never announce that the user is active.
    ///
    /// # Errors
    /// Returns session or stored presence failures; unknown state is not a fresh mark.
    pub fn active_chat_peers(&self) -> Result<Vec<String>, CoreError> {
        let session = self.session.lock().map_err(failed)?;
        dyapp_client::presence::active_peers(session.store()).map_err(failed)
    }
}
