//! Thin adapters for common presence policy; only an explicit foreground call announces activity.
use super::{failed, ChatSession};
use crate::profile_network::{active_network, connect, on_network_stack};
use crate::CoreError;
use dyapp_client::session::Session;

#[uniffi::export]
impl ChatSession {
    /// Whether foreground presence may go to current matches; enabled by default.
    ///
    /// # Errors
    /// Returns session-lock or stored-setting failures.
    pub fn presence_enabled(&self) -> Result<bool, CoreError> {
        let session = self.session.lock().map_err(failed)?;
        session.store().send_presence().map_err(failed)
    }

    /// Choose matches or nobody without announcing activity or changing received marks.
    ///
    /// # Errors
    /// Returns session-lock or stored-setting failures.
    pub fn set_presence_enabled(&self, enabled: bool) -> Result<(), CoreError> {
        let session = self.session.lock().map_err(failed)?;
        session.store().set_send_presence(enabled).map_err(failed)
    }

    /// Explicit foreground use only, never mail passes or timers.
    /// Returns false without opening a connection when no match is due.
    /// Staging rechecks the common audience and throttle after connection.
    ///
    /// # Errors
    /// Returns session, preflight, active-network or send failures. Earlier writes may
    /// already be committed if a later group's staging or delivery fails.
    pub fn announce_presence(&self) -> Result<bool, CoreError> {
        let mut guard = self.session.lock().map_err(failed)?;
        let session: &mut Session = &mut guard;
        if !dyapp_client::presence::due_at(session.store(), dyapp_client::clock::now_ms())
            .map_err(failed)?
        {
            return Ok(false);
        }
        let network = active_network(&self.dir)?;
        on_network_stack(|| {
            self.runtime.block_on(async {
                let mut node = connect(&network).await?;
                session
                    .announce_presence(std::slice::from_mut(&mut node), &self.seal_key)
                    .await
                    .map_err(failed)?;
                Ok(true)
            })
        })
    }

    /// Current chat peers whose authenticated match presence is still fresh.
    ///
    /// Use only for presentation. Foreground sending is a separate operation; mail passes,
    /// reads and expiry refreshes must never announce that the user is active.
    ///
    /// # Errors
    /// Returns session or stored presence failures; unknown state is not a fresh mark.
    pub fn active_chat_peers(&self) -> Result<Vec<String>, CoreError> {
        let session = self.session.lock().map_err(failed)?;
        dyapp_client::presence::active_peers_at(session.store(), dyapp_client::clock::now_ms())
            .map_err(failed)
    }
}
