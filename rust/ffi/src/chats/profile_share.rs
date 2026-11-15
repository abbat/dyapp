//! Profile sharing uses core admission in the same session that commits the ordinary link text.
use super::{failed, listed_own, ChatSession, ChatSubmission};
use crate::profile_network::{active_network, connect, on_network_stack};
use crate::CoreError;
use dyapp_client::session::{Error, Session};

fn share_error(error: Error) -> CoreError {
    match error {
        Error::InvalidProfileShare => failed("invalidTarget"),
        Error::Store(dyapp_client::store::Error::BlockedPeer) => failed("blocked"),
        error => failed(error),
    }
}

#[uniffi::export]
impl ChatSession {
    /// Share a profile only into a distinct, unblocked, existing open pair.
    /// Queued delivery retains the committed ID and bytes; never resubmit it as generic text.
    ///
    /// # Errors
    /// Returns invalidTarget, blocked, unpublished or underlying storage/network failures before
    /// commitment. An established eligible pair can queue without an active network.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn submit_profile_share(
        &self,
        peer_id: String,
        shared_peer_id: String,
    ) -> Result<ChatSubmission, CoreError> {
        let mut guard = self.session.lock().map_err(failed)?;
        let session: &mut Session = &mut guard;
        session
            .prepare_profile_share(&peer_id, &shared_peer_id)
            .map_err(share_error)?;
        let own = listed_own(session, &self.key.identity)?;
        let network = active_network(&self.dir);
        on_network_stack(|| {
            self.runtime.block_on(async {
                let connected = async { connect(&network?).await }.await;
                let Ok(mut node) = connected else {
                    return session
                        .submit_established_profile_share(
                            &mut [],
                            &peer_id,
                            &shared_peer_id,
                            &self.seal_key,
                        )
                        .await
                        .map(ChatSubmission::from)
                        .map_err(share_error);
                };
                let peer = node.get_profile(&peer_id).await.map_err(failed)?;
                if peer.profile.deleted || peer.profile.version == 0 {
                    return Err(failed("unpublished"));
                }
                session
                    .submit_profile_share_to(
                        std::slice::from_mut(&mut node),
                        &[own, peer],
                        &shared_peer_id,
                        &self.seal_key,
                    )
                    .await
                    .map(ChatSubmission::from)
                    .map_err(share_error)
            })
        })
    }
}
