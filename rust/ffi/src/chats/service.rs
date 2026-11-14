//! Existing native chat commands; registry admission lives in creation.

use super::{
    failed, likes_in, listed_own, own_profile, publish_failed, reads, refill_if_due, ChatHistory,
    ChatLikeAttempt, ChatLikes, ChatMessage, ChatRow, ChatSession, ChatState, ChatSubmission,
    KEY_PACKAGES,
};
use crate::contact_names::fetch_profiles;
use crate::profile_network::{
    active_network, connect, on_network_stack, publishable_attempt, send_held, ProfilePublished,
};
use crate::{CoreError, FoundProfile};
use dyapp_client::{chat::MAX_TEXT_BYTES, interactions::Kind, session::Session};
use std::time::{SystemTime, UNIX_EPOCH};

#[uniffi::export]
impl ChatSession {
    /// Publish the solved draft listing this device, check the signed readback, then publish
    /// `KeyPackages` so peers can invite the device. Errors as in `ProfilePublisher::publish`.
    ///
    /// # Errors
    ///
    /// Returns an error if the session lock, draft validation, device listing, active network,
    /// profile publication, readback, key-package publication or held-photo update fails.
    pub fn publish_profile(&self, attested: bool) -> Result<ProfilePublished, CoreError> {
        let mut guard = self.session.lock().map_err(failed)?;
        // The guard stays on this thread; the session it lends moves to the network thread.
        let session: &mut Session = &mut guard;
        let attempt = publishable_attempt(session.store(), &self.key.identity, attested)?;
        let network = active_network(&self.dir)?;
        let published = on_network_stack(|| {
            self.runtime.block_on(async {
                let mut node = connect(&network).await?;
                let published = session
                    .publish_profile(
                        &mut node,
                        &self.key.identity,
                        &self.device_key.identity,
                        attempt,
                    )
                    .await
                    .map_err(|error| match error {
                        dyapp_client::session::Error::Publish(failure) => publish_failed(failure),
                        error => failed(error),
                    })?
                    .published;
                // shortcut: a fixed batch per publish, as on desktop; peers can exhaust it until
                // the next publish, so refill it when the receive loop lands.
                session
                    .publish_key_packages(
                        &mut node,
                        &self.device_key.identity,
                        KEY_PACKAGES,
                        &self.seal_key,
                    )
                    .await
                    .map_err(failed)?;
                send_held(session.store(), &mut node, &network).await?;
                session
                    .store()
                    .profile_publication_state(&self.key.identity, &published)
                    .map_err(failed)
            })
        })?;
        Ok(ProfilePublished::from_outcome(&published))
    }

    /// Where the chat with `peer_id` stands.
    ///
    /// # Errors
    ///
    /// Returns an error if the session lock or stored conversation lookup fails.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn state(&self, peer_id: String) -> Result<ChatState, CoreError> {
        let session = self.session.lock().map_err(failed)?;
        Ok(session.conversation(&peer_id).map_err(failed)?.into())
    }

    /// Strict compatibility send: queued delivery remains an error.
    ///
    /// # Errors
    /// Returns pre-commit errors or the committed operation's immediate delivery failure.
    pub fn send_text(&self, peer_id: String, text: String) -> Result<Vec<u8>, CoreError> {
        self.submit_text(peer_id, text)?.into_accepted()
    }

    /// Submit `text` to `peer_id`, whose verified profile comes from the active network, and return
    /// its committed logical ID and delivery outcome. The first text invites the peer; a text to
    /// a pending request opens the chat. Queued writes retry with the same bytes. Errors
    /// are `ownPeer`, `invalidText`, `blocked`, `unpublished` (this device or the peer has no
    /// published profile), `notConnected` or a node or storage message.
    ///
    /// # Errors
    ///
    /// Returns an error for the own peer, invalid text, a blocked or unpublished peer, or if
    /// session locking, network access, message encryption or persistence fails.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn submit_text(&self, peer_id: String, text: String) -> Result<ChatSubmission, CoreError> {
        if peer_id == self.key.identity.peer_id() {
            return Err(CoreError::Failed("ownPeer".into()));
        }
        if text.is_empty() || text.len() > MAX_TEXT_BYTES {
            return Err(CoreError::Failed("invalidText".into()));
        }
        let mut guard = self.session.lock().map_err(failed)?;
        // The guard stays on this thread; the session it lends moves to the network thread.
        let session: &mut Session = &mut guard;
        if session.store().blocked(&peer_id).map_err(failed)? {
            return Err(CoreError::Failed("blocked".into()));
        }
        let own = listed_own(session, &self.key.identity)?;
        let network = active_network(&self.dir);
        on_network_stack(|| {
            self.runtime.block_on(async {
                let connected = async { connect(&network?).await }.await;
                let mut node = match connected {
                    Ok(node) => node,
                    Err(error) => {
                        return session
                            .try_submit_established_text(
                                &mut [],
                                &peer_id,
                                text.as_bytes(),
                                &self.seal_key,
                            )
                            .await
                            .map_err(failed)?
                            .map(ChatSubmission::from)
                            .ok_or(error);
                    }
                };
                let peer = node.get_profile(&peer_id).await.map_err(failed)?;
                if peer.profile.deleted || peer.profile.version == 0 {
                    return Err(CoreError::Failed("unpublished".into()));
                }
                session
                    .submit_text_to(
                        std::slice::from_mut(&mut node),
                        &[own, peer],
                        text.as_bytes(),
                        &self.seal_key,
                    )
                    .await
                    .map(ChatSubmission::from)
                    .map_err(failed)
            })
        })
    }

    /// This identity's chats, newest first, without blocked peers.
    ///
    /// # Errors
    ///
    /// Returns an error if the session lock or stored chat, block or mute lookup fails.
    pub fn chat_rows(&self) -> Result<Vec<ChatRow>, CoreError> {
        let session = self.session.lock().map_err(failed)?;
        let store = session.store();
        let mut rows = Vec::new();
        for row in store.chat_rows().map_err(failed)? {
            if !store.blocked(&row.peer_id).map_err(failed)? {
                let muted = store.muted(&row.peer_id).map_err(failed)?;
                rows.push(ChatRow::new(row, muted));
            }
        }
        Ok(rows)
    }

    /// The current messages with `peer_id`; `blocked` for a blocked peer.
    ///
    /// # Errors
    ///
    /// Returns an error for a blocked peer, or if the session lock, history, read marks or
    /// conversation settings cannot be read.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn chat_history(&self, peer_id: String) -> Result<ChatHistory, CoreError> {
        let session = self.session.lock().map_err(failed)?;
        let store = session.store();
        if store.blocked(&peer_id).map_err(failed)? {
            return Err(CoreError::Failed("blocked".into()));
        }
        let mut messages: Vec<ChatMessage> = store
            .current_chat_messages(&peer_id)
            .map_err(failed)?
            .into_iter()
            .map(Into::into)
            .collect();
        let own = self.key.identity.peer_id();
        reads::project_reads(store, &peer_id, &own, &mut messages)?;
        Ok(ChatHistory {
            state: store.conversation(&peer_id).map_err(failed)?.into(),
            messages,
            muted: store.muted(&peer_id).map_err(failed)?,
        })
    }

    /// The like state with `peer_id`.
    ///
    /// # Errors
    ///
    /// Returns an error if the session lock or stored like/pass lookup fails.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn likes(&self, peer_id: String) -> Result<ChatLikes, CoreError> {
        let session = self.session.lock().map_err(failed)?;
        likes_in(session.store(), &peer_id)
    }

    /// The Liked you peers: each likes this profile, is not liked back and is not blocked, in
    /// Peer ID order. Local: the screen fetches their profiles.
    ///
    /// # Errors
    ///
    /// Returns an error if the session lock or stored incoming-like lookup fails.
    pub fn liked_you(&self) -> Result<Vec<String>, CoreError> {
        let session = self.session.lock().map_err(failed)?;
        session.store().liked_by().map_err(failed)
    }

    /// The verified profiles of `peer_ids` from the active network, in that order; a peer no node
    /// returns is left out. No peers means no connection. Errors are `notConnected` or a node
    /// message.
    ///
    /// # Errors
    ///
    /// Returns an error if active-network configuration, worker startup or node connection fails.
    /// Individual unavailable peer profiles are omitted.
    pub fn profiles(&self, peer_ids: Vec<String>) -> Result<Vec<FoundProfile>, CoreError> {
        let found = fetch_profiles(&self.runtime, &self.dir, peer_ids)?;
        Ok(found.iter().map(FoundProfile::from).collect())
    }

    /// Strict compatibility like: delivery errors take precedence over projection read errors.
    ///
    /// # Errors
    /// Returns pre-commit errors, immediate delivery errors, or a failed subsequent state read.
    pub fn like(&self, peer_id: String, liked: bool) -> Result<ChatLikes, CoreError> {
        self.submit_like(peer_id, liked)?.into_accepted()
    }

    /// Like `peer_id` (`liked`) or take the like back, as a signal in the pair group on the active
    /// network; the first like invites the peer. Committed signals report delivery separately
    /// from the subsequent state read. Errors are `unpublished` (no own profile lists this device),
    /// `notConnected` or a node or storage message.
    ///
    /// # Errors
    ///
    /// Returns lock, own-profile, network, peer-profile or interaction-staging errors.
    /// Post-commit delivery and projection failures stay in the result.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn submit_like(&self, peer_id: String, liked: bool) -> Result<ChatLikeAttempt, CoreError> {
        let mut guard = self.session.lock().map_err(failed)?;
        // The guard stays on this thread; the session it lends moves to the network thread.
        let session: &mut Session = &mut guard;
        let own = listed_own(session, &self.key.identity)?;
        let network = active_network(&self.dir)?;
        let kind = if liked { Kind::Like } else { Kind::Unlike };
        on_network_stack(|| {
            self.runtime.block_on(async {
                let mut node = connect(&network).await?;
                let peer = node.get_profile(&peer_id).await.map_err(failed)?;
                let submission = session
                    .submit_interaction_with(
                        std::slice::from_mut(&mut node),
                        &[own, peer],
                        kind,
                        &self.seal_key,
                    )
                    .await
                    .map_err(failed)?;
                Ok(ChatLikeAttempt::new(
                    submission,
                    likes_in(session.store(), &peer_id),
                ))
            })
        })
    }

    /// Resend the staged writes that are due, apply this device's mailbox and refill tokens when
    /// due. Completes authorized core pack answers/downloads and counts saved packs for refresh.
    /// Returns chat-event and saved-pack changes. An envelope from an owner the node has no
    /// profile for waits in the mailbox for the next call. Errors are `notConnected` or a node
    /// or storage message.
    ///
    /// # Errors
    ///
    /// Returns an error if the session lock, own profile, clock, active network, staged-write retry
    /// or mailbox/pack-job processing fails. Token refill and photo renewal are best effort.
    pub fn receive(&self) -> Result<u32, CoreError> {
        let mut guard = self.session.lock().map_err(failed)?;
        // The guard stays on this thread; the session it lends moves to the network thread.
        let session: &mut Session = &mut guard;
        let own = own_profile(session.store(), &self.key.identity)?;
        let network = active_network(&self.dir)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(failed)?;
        let now_ms = i64::try_from(now.as_millis()).map_err(failed)?;
        on_network_stack(|| {
            self.runtime.block_on(async {
                let mut node = connect(&network).await?;
                session
                    .retry_due(std::slice::from_mut(&mut node), now_ms, &self.seal_key)
                    .await
                    .map_err(failed)?;
                let changed = self.receive_mailbox(session, &mut node, own).await?;
                refill_if_due(session, &mut node, now.as_secs(), &self.seal_key).await?;
                // Best effort like the refill, as on the desktop: once the first publish made a
                // last-resort package it rotates or repeats, and the photo hold renews daily.
                if matches!(session.store().last_resort(), Ok(Some(_))) {
                    let _ = session
                        .keep_last_resort(
                            &mut node,
                            &self.device_key.identity,
                            now.as_secs(),
                            &self.seal_key,
                        )
                        .await;
                }
                let _ = dyapp_client::publish::refresh_photos(
                    session.store(),
                    &mut node,
                    &self.key.identity,
                    now.as_secs(),
                )
                .await;
                Ok(changed)
            })
        })
    }
}
