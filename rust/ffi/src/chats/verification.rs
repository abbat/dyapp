//! Thin native contact verification over the shared core context and authority.
use super::{failed, ChatSession};
use crate::profile_network::{active_network, on_network_stack};
use crate::profile_search::FoundProfile;
use crate::CoreError;
use dyapp_client::contact_verification;
use dyapp_identity::network::Network;
use dyapp_profile::VerifiedProfile;

/// The shared contact mark and device-change notice, after observing cached authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct ContactVerificationState {
    /// Current cached devices remain verified.
    pub verified: bool,
    /// An observed device change still needs acknowledgement.
    pub new_device: bool,
}

impl From<contact_verification::State> for ContactVerificationState {
    fn from(state: contact_verification::State) -> Self {
        Self {
            verified: state.verified,
            new_device: state.new_device,
        }
    }
}

#[uniffi::export]
impl ChatSession {
    /// Confirm a completed safety-number/QR comparison against the highest observed profile.
    /// The Session owns the identity/directory; no platform profile can authorize the mark.
    ///
    /// # Errors
    /// Returns target, active-context, signed-authority, reachability or storage failures.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned foreign arguments"
    )]
    pub fn confirm_contact(&self, peer_id: String) -> Result<FoundProfile, CoreError> {
        self.confirm_contact_with(&peer_id, |network, peer| {
            on_network_stack(|| {
                let scores = self.contact_context.lock().map_err(failed)?.scores();
                let mut scores = self.runtime.block_on(scores.lock());
                Ok(self.runtime.block_on(contact_verification::fetch(
                    &self.dir,
                    network,
                    peer,
                    &mut scores,
                )))
            })?
            .map_err(failed)
        })
    }

    /// Read the mark/notice through the same core context used by native search workers.
    ///
    /// # Errors
    /// Rejects invalid/own targets and propagates context/session/storage failures.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned foreign arguments"
    )]
    pub fn contact_verification(
        &self,
        peer_id: String,
    ) -> Result<ContactVerificationState, CoreError> {
        let session = self.session.lock().map_err(failed)?;
        let context = self.contact_context.lock().map_err(failed)?;
        session
            .contact_verification(&context, &peer_id)
            .map(Into::into)
            .map_err(failed)
    }

    /// Remove a mark while retaining remembered devices and any pending device notice.
    ///
    /// # Errors
    /// Rejects invalid/own targets and propagates session/storage failures.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned foreign arguments"
    )]
    pub fn unverify_contact(&self, peer_id: String) -> Result<(), CoreError> {
        self.session
            .lock()
            .map_err(failed)?
            .unverify_contact(&peer_id)
            .map_err(failed)
    }

    /// Acknowledge a device-change notice without restoring the verified mark.
    ///
    /// # Errors
    /// Rejects invalid/own targets and propagates session/storage failures.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned foreign arguments"
    )]
    pub fn acknowledge_contact_devices(&self, peer_id: String) -> Result<(), CoreError> {
        self.session
            .lock()
            .map_err(failed)?
            .acknowledge_contact_devices(&peer_id)
            .map_err(failed)
    }
}

impl ChatSession {
    fn confirm_contact_with(
        &self,
        peer: &str,
        fetch: impl FnOnce(&Network, &str) -> Result<VerifiedProfile, CoreError>,
    ) -> Result<FoundProfile, CoreError> {
        let session = self.session.lock().map_err(failed)?;
        let (ticket, network) = {
            let context = self.contact_context.lock().map_err(failed)?;
            let ticket = context.begin(session.store(), peer).map_err(failed)?;
            (ticket, active_network(&self.dir)?)
        };
        // Activation may run while the network callback is in flight.
        let fetched = fetch(&network, peer)?;
        let mut context = self.contact_context.lock().map_err(failed)?;
        let accepted = session
            .confirm_contact(&mut context, ticket, fetched)
            .map_err(failed)?;
        Ok(FoundProfile::from(&accepted))
    }
}

#[cfg(test)]
mod tests;
