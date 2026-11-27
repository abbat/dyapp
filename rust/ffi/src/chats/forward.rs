//! Stored text forwarding keeps core selection and committed delivery outcomes intact.
use super::{listed_own, ChatSession, ChatSubmission};
use crate::peer_photos::reachable_at;
use crate::profile_network::{active_network, connect_at, on_network_stack};
use crate::CoreError;
use dyapp_client::forwarding;

/// Full local source identity; the original author's identity is never forwarded.
#[derive(Clone, Debug, uniffi::Record)]
pub struct ChatForwardSource {
    /// The source conversation's peer.
    pub peer_id: String,
    /// The source MLS group.
    pub group_id: Vec<u8>,
    /// The stored author, used only for local lookup.
    pub author: String,
    /// The source logical message ID.
    pub message_id: Vec<u8>,
}
impl ChatForwardSource {
    fn borrowed(&self) -> forwarding::Source<'_> {
        forwarding::Source {
            peer: &self.peer_id,
            group: &self.group_id,
            author: &self.author,
            id: &self.message_id,
        }
    }
}

/// Stable forwarding refusal and backend errors, before a committed submission is returned.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum ChatForwardError {
    /// The target is own/invalid, the source peer, or not an open conversation.
    #[error("invalidTarget")]
    InvalidTarget,
    /// The source is this identity.
    #[error("ownPeer")]
    OwnPeer,
    /// The source conversation is unavailable.
    #[error("unknownPeer")]
    UnknownPeer,
    /// Source or target is blocked.
    #[error("blocked")]
    Blocked,
    /// No matching live current source text exists.
    #[error("unknownMessage")]
    UnknownMessage,
    /// No active network is configured.
    #[error("notConnected")]
    NotConnected,
    /// The own device or target profile is not published.
    #[error("unpublished")]
    Unpublished,
    /// Locking, network, storage or staging failed.
    #[error("{reason}")]
    Failed {
        /// Diagnostic text, not a stable policy code.
        reason: String,
    },
}
fn failed(error: impl std::fmt::Display) -> ChatForwardError {
    ChatForwardError::Failed {
        reason: error.to_string(),
    }
}
impl From<forwarding::Error> for ChatForwardError {
    fn from(error: forwarding::Error) -> Self {
        match error {
            forwarding::Error::InvalidTarget => Self::InvalidTarget,
            forwarding::Error::OwnPeer => Self::OwnPeer,
            forwarding::Error::UnknownPeer => Self::UnknownPeer,
            forwarding::Error::Blocked => Self::Blocked,
            forwarding::Error::UnknownMessage => Self::UnknownMessage,
            forwarding::Error::Store(error) => failed(error),
        }
    }
}
impl From<CoreError> for ChatForwardError {
    fn from(error: CoreError) -> Self {
        match error {
            CoreError::Failed(reason) if reason == "notConnected" => Self::NotConnected,
            CoreError::Failed(reason) if reason == "unpublished" => Self::Unpublished,
            error @ CoreError::Failed(_) => failed(error),
        }
    }
}

fn stage_error(error: dyapp_client::session::Error) -> ChatForwardError {
    match error {
        dyapp_client::session::Error::Store(dyapp_client::store::Error::BlockedPeer) => {
            ChatForwardError::Blocked
        }
        dyapp_client::session::Error::ConversationNotOpen
        | dyapp_client::session::Error::UnknownGroup => ChatForwardError::InvalidTarget,
        error => failed(error),
    }
}

#[uniffi::export]
impl ChatSession {
    /// List other open chats eligible to receive the selected current local text.
    /// Read-only; a later forward still validates its source and destination again.
    ///
    /// # Errors
    /// Returns typed source/selection refusals or a session/storage failure.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn forward_targets(
        &self,
        source: ChatForwardSource,
    ) -> Result<Vec<String>, ChatForwardError> {
        let session = self.session.lock().map_err(failed)?;
        forwarding::targets(session.store(), source.borrowed()).map_err(ChatForwardError::from)
    }

    /// List other open chats eligible to receive the selected current photo.
    ///
    /// # Errors
    /// Returns typed source/selection refusals or a session/storage failure.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn forward_photo_targets(
        &self,
        source: ChatForwardSource,
    ) -> Result<Vec<String>, ChatForwardError> {
        let session = self.session.lock().map_err(failed)?;
        forwarding::photo_targets(session.store(), source.borrowed())
            .map_err(ChatForwardError::from)
    }

    /// Forward a current photo whose original is still retrievable from the network.
    /// Queued delivery retains its committed ID.
    ///
    /// # Errors
    /// Returns typed preflight/profile/network errors or pre-commit backend failure.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn forward_photo(
        &self,
        source: ChatForwardSource,
        target_peer_id: String,
    ) -> Result<ChatSubmission, ChatForwardError> {
        let mut session = self.session.lock().map_err(failed)?;
        let photo = forwarding::prepare_photo(session.store(), source.borrowed(), &target_peer_id)?;
        let _own = listed_own(&session, &self.key.identity).map_err(ChatForwardError::from)?;
        let network = active_network(&self.dir).map_err(ChatForwardError::from)?;
        on_network_stack(|| {
            Ok(self.runtime.block_on(async {
                let mut node = connect_at(&self.dir, &network)
                    .await
                    .map_err(ChatForwardError::from)?;
                let target = node.get_profile(&target_peer_id).await.map_err(failed)?;
                if target.profile.deleted || target.profile.version == 0 {
                    return Err(ChatForwardError::Unpublished);
                }
                let mut nodes = reachable_at(&self.dir, &network).await?;
                if nodes.is_empty() {
                    return Err(ChatForwardError::NotConnected);
                }
                let scores = self.contact_context.lock().map_err(failed)?.scores();
                let original = {
                    let mut scores = scores.lock().await;
                    dyapp_client::attachment::fetch_with_scores(
                        &mut nodes,
                        photo.attachment(),
                        &mut scores,
                    )
                    .await
                    .map_err(super::original::fetch_error)?
                };
                session
                    .submit_forwarded_photo(
                        &mut nodes,
                        &self.key.identity,
                        &photo,
                        &original,
                        &target_peer_id,
                        &self.seal_key,
                    )
                    .await
                    .map(ChatSubmission::from)
                    .map_err(stage_error)
            }))
        })
        .map_err(failed)?
    }

    /// Forward a selected current local text as this identity into another open chat.
    /// Queued delivery retains its committed ID: do not submit a second copy.
    ///
    /// # Errors
    /// Returns typed preflight/profile/network errors or pre-commit backend failure.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn forward_text(
        &self,
        source: ChatForwardSource,
        target_peer_id: String,
    ) -> Result<ChatSubmission, ChatForwardError> {
        let mut guard = self.session.lock().map_err(failed)?;
        let session: &mut dyapp_client::session::Session = &mut guard;
        let text = forwarding::prepare(session.store(), source.borrowed(), &target_peer_id)?;
        let _own = listed_own(session, &self.key.identity).map_err(ChatForwardError::from)?;
        let network = active_network(&self.dir).map_err(ChatForwardError::from)?;
        on_network_stack(|| {
            Ok(self.runtime.block_on(async {
                let mut node = connect_at(&self.dir, &network)
                    .await
                    .map_err(ChatForwardError::from)?;
                let target = node.get_profile(&target_peer_id).await.map_err(failed)?;
                if target.profile.deleted || target.profile.version == 0 {
                    return Err(ChatForwardError::Unpublished);
                }
                session
                    .submit_forwarded_text_to(
                        std::slice::from_mut(&mut node),
                        &target_peer_id,
                        &text,
                        &self.seal_key,
                    )
                    .await
                    .map(ChatSubmission::from)
                    .map_err(stage_error)
            }))
        })
        .map_err(failed)?
    }
}
