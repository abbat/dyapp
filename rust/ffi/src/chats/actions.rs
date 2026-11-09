//! Clearing, reading, editing, deleting and blocking in one pair's chat, as on desktop.

use super::{failed, ChatLikes, ChatSession};
use crate::profile_network::{active_network, connect, on_network_stack};
use crate::CoreError;
use dyapp_client::chat::{PeerChange, MAX_TEXT_BYTES};
use dyapp_client::session::Session;
use dyapp_client::store::Conversation;
use std::time::{SystemTime, UNIX_EPOCH};

/// Whether the tokens a blocked peer holds are revoked on the node.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ChatRevocation {
    /// The peer holds no live token.
    NotNeeded,
    /// No node confirmed the revocation yet; blocking again repeats it.
    Pending,
    /// A node confirmed it.
    Confirmed,
}

/// The outcome of [`ChatSession::block`]; the block itself always stays.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ChatBlock {
    /// Whether the peer's tokens are revoked on the node.
    pub revocation: ChatRevocation,
    /// Why the revocation is pending.
    pub detail: Option<String>,
}

impl ChatBlock {
    fn pending(detail: impl std::fmt::Display) -> Self {
        Self {
            revocation: ChatRevocation::Pending,
            detail: Some(detail.to_string()),
        }
    }
}

fn utc_day() -> Result<u32, CoreError> {
    let today = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(failed)?
        .as_secs()
        / 86_400;
    u32::try_from(today).map_err(failed)
}

fn code(code: &str) -> CoreError {
    CoreError::Failed(code.into())
}

impl ChatSession {
    /// Refuse a chat the screen cannot show: `ownPeer`, `blocked` or `unknownPeer`.
    pub(super) fn check_visible(&self, session: &Session, peer_id: &str) -> Result<(), CoreError> {
        if peer_id == self.key.identity.peer_id() {
            return Err(code("ownPeer"));
        }
        let store = session.store();
        if store.blocked(peer_id).map_err(failed)? {
            return Err(code("blocked"));
        }
        if store.conversation(peer_id).map_err(failed)? == Conversation::None {
            return Err(code("unknownPeer"));
        }
        Ok(())
    }

    /// The live own text `message_id` in `group_id` of the chat with `peer_id`, or `ownPeer`,
    /// `blocked` or `unknownMessage`.
    fn own_target(
        &self,
        session: &Session,
        peer_id: &str,
        group_id: &[u8],
        message_id: &[u8],
    ) -> Result<[u8; 16], CoreError> {
        let own = self.key.identity.peer_id();
        if peer_id == own {
            return Err(code("ownPeer"));
        }
        let store = session.store();
        if store.blocked(peer_id).map_err(failed)? {
            return Err(code("blocked"));
        }
        let id: [u8; 16] = message_id.try_into().map_err(|_| code("unknownMessage"))?;
        let found = store
            .current_chat_messages(peer_id)
            .map_err(failed)?
            .into_iter()
            .any(|message| {
                message.key.group.as_slice() == group_id
                    && message.key.id == id
                    && message.key.author == own
                    && !message.current.deleted
            });
        if found {
            Ok(id)
        } else {
            Err(code("unknownMessage"))
        }
    }
}

#[uniffi::export]
impl ChatSession {
    /// Remove every message with `peer_id` on this device and return how many went; the peer
    /// keeps its copies. Errors are `ownPeer`, `blocked`, `unknownPeer` or a storage message.
    ///
    /// # Errors
    ///
    /// Returns an error for the own, blocked or unknown peer, or if session locking, history
    /// clearing, original-file cleanup or result-count conversion fails.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn clear_chat(&self, peer_id: String) -> Result<u32, CoreError> {
        let mut session = self.session.lock().map_err(failed)?;
        self.check_visible(&session, &peer_id)?;
        let cleared = session.clear_chat_pair(&peer_id).map_err(failed)?;
        session.store().sweep_originals(&self.dir).map_err(failed)?;
        u32::try_from(cleared).map_err(failed)
    }

    /// Mark the texts from `peer_id` read here and return how many were unread. Local only:
    /// `send_reads` sends the receipts. Errors as in [`Self::clear_chat`].
    ///
    /// # Errors
    ///
    /// Returns an error for the own, blocked or unknown peer, or if session locking, read-state
    /// storage or result-count conversion fails.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn mark_read(&self, peer_id: String) -> Result<u32, CoreError> {
        let mut session = self.session.lock().map_err(failed)?;
        self.check_visible(&session, &peer_id)?;
        let read = session.mark_chat_read(&peer_id).map_err(failed)?;
        u32::try_from(read).map_err(failed)
    }

    /// Replace the live own text `message_id` in `group_id` with `text` and return the edit's
    /// ID. A failed send stays staged. Errors are `invalidText`, `ownPeer`, `blocked`,
    /// `unknownMessage`, `notConnected` or a node or storage message.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid text, the own or blocked peer or an unknown own message,
    /// or if session locking, network access, encrypted sending or persistence fails.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn edit_text(
        &self,
        peer_id: String,
        group_id: Vec<u8>,
        message_id: Vec<u8>,
        text: String,
    ) -> Result<Vec<u8>, CoreError> {
        if text.is_empty() || text.len() > MAX_TEXT_BYTES {
            return Err(code("invalidText"));
        }
        self.operate(&peer_id, &group_id, &message_id, Some(&text))
    }

    /// Delete the live own text `message_id` in `group_id` for both sides and return the
    /// delete's ID. Errors as in [`Self::edit_text`] without `invalidText`.
    ///
    /// # Errors
    ///
    /// Returns an error for the own or blocked peer or an unknown own message, or if session
    /// locking, network access, encrypted sending or persistence fails.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn delete_text(
        &self,
        peer_id: String,
        group_id: Vec<u8>,
        message_id: Vec<u8>,
    ) -> Result<Vec<u8>, CoreError> {
        self.operate(&peer_id, &group_id, &message_id, None)
    }

    /// Block `peer_id` here and for the owner's other devices, then revoke the write tokens it
    /// holds. The block stays whatever the revocation does; blocking again repeats it. Errors
    /// are `invalidPeer` (not a Peer ID, or this identity's own) or a storage message.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid or own peer, or if session locking, storing the block,
    /// worker startup or clock conversion fails. Revocation failures are returned as pending.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn block(&self, peer_id: String) -> Result<ChatBlock, CoreError> {
        let change = PeerChange::Blocked(true);
        if peer_id == self.key.identity.peer_id() || !change.valid(&peer_id) {
            return Err(code("invalidPeer"));
        }
        let mut guard = self.session.lock().map_err(failed)?;
        // The guard stays on this thread; the session it lends moves to the network thread.
        let session: &mut Session = &mut guard;
        session
            .store()
            .set_blocked(&peer_id, true)
            .map_err(failed)?;
        on_network_stack(|| {
            self.runtime.block_on(async {
                // With no nodes the change waits in the outbox for the next receive.
                if let Err(error) = session
                    .change_peer(&mut [], &peer_id, change, &self.seal_key)
                    .await
                {
                    return Ok(ChatBlock::pending(error));
                }
                let today = utc_day()?;
                match session.store().revoke_issued(&peer_id, today) {
                    Ok(keys) if keys.is_empty() => {
                        return Ok(ChatBlock {
                            revocation: ChatRevocation::NotNeeded,
                            detail: None,
                        })
                    }
                    Ok(_) => {}
                    Err(error) => return Ok(ChatBlock::pending(error)),
                }
                let revoked = async {
                    let mut node = connect(&active_network(&self.dir)?).await?;
                    session
                        .block(
                            std::slice::from_mut(&mut node),
                            &self.device_key.identity,
                            &peer_id,
                        )
                        .await
                        .map_err(failed)
                };
                Ok(match revoked.await {
                    Ok(()) => ChatBlock {
                        revocation: ChatRevocation::Confirmed,
                        detail: None,
                    },
                    Err(error) => ChatBlock::pending(error),
                })
            })
        })
    }

    /// Unblock `peer_id` here and for the owner's other devices. Revoked tokens stay revoked.
    /// Errors are `invalidPeer` or a storage message.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid or own peer, or if session locking, worker startup,
    /// encryption or storage of the own-device change fails.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn unblock(&self, peer_id: String) -> Result<(), CoreError> {
        self.change_own(&peer_id, PeerChange::Blocked(false))
    }

    /// Mute `peer_id` here and for the owner's other devices, or end that. Only notifications
    /// change: the mail, the chat and its unread count stay. Errors as for `unblock`.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid or own peer, or if session locking, worker startup,
    /// encryption or storage of the own-device change fails.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn set_muted(&self, peer_id: String, muted: bool) -> Result<(), CoreError> {
        self.change_own(&peer_id, PeerChange::Muted(muted))
    }

    /// Give `peer_id`, whose chat is visible, a local name only this identity sees, shared with
    /// own devices; the name is trimmed and an empty one removes it. Errors are `ownPeer`,
    /// `blocked`, `unknownPeer`, `invalidName` (over 64 characters or control text) or a
    /// network message.
    ///
    /// # Errors
    ///
    /// Returns an error for the own, blocked or unknown peer or an invalid name, or if session
    /// locking, worker startup, encryption or storage of the own-device change fails.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn set_contact_name(&self, peer_id: String, name: String) -> Result<(), CoreError> {
        // `&*` keeps `?` from expecting a `Session`; the guard drops before `change_own` locks.
        self.check_visible(&*self.session.lock().map_err(failed)?, &peer_id)?;
        let change = PeerChange::Name(name.trim().to_owned());
        if !change.valid(&peer_id) {
            return Err(code("invalidName"));
        }
        self.change_own(&peer_id, change)
    }

    /// Pass on `peer_id` here and for the owner's other devices, or undo that; no signal goes to
    /// the peer. A pass hides the peer from search and Liked you. Returns the like state after it.
    /// Errors as for `unblock`.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid or own peer, or if session locking, worker startup,
    /// change persistence or the resulting like/pass lookup fails.
    pub fn set_passed(&self, peer_id: String, passed: bool) -> Result<ChatLikes, CoreError> {
        self.change_own(&peer_id, PeerChange::Passed(passed))?;
        self.likes(peer_id)
    }

    /// The peers this profile blocks, in Peer ID order.
    ///
    /// # Errors
    ///
    /// Returns an error if the session lock or stored blocked-peer lookup fails.
    pub fn blocked_peers(&self) -> Result<Vec<String>, CoreError> {
        let session = self.session.lock().map_err(failed)?;
        session.store().blocked_peers().map_err(failed)
    }

    /// The peers this profile passes on, in Peer ID order.
    ///
    /// # Errors
    ///
    /// Returns an error if the session lock or stored passed-peer lookup fails.
    pub fn passed_peers(&self) -> Result<Vec<String>, CoreError> {
        let session = self.session.lock().map_err(failed)?;
        session.store().passed_peers().map_err(failed)
    }
}

impl ChatSession {
    fn change_own(&self, peer_id: &str, change: PeerChange) -> Result<(), CoreError> {
        if peer_id == self.key.identity.peer_id() || !change.valid(peer_id) {
            return Err(code("invalidPeer"));
        }
        let mut guard = self.session.lock().map_err(failed)?;
        let session: &mut Session = &mut guard;
        on_network_stack(|| {
            self.runtime.block_on(async {
                session
                    .change_peer(&mut [], peer_id, change, &self.seal_key)
                    .await
                    .map_err(failed)
            })
        })
    }

    fn operate(
        &self,
        peer_id: &str,
        group_id: &[u8],
        message_id: &[u8],
        text: Option<&str>,
    ) -> Result<Vec<u8>, CoreError> {
        let mut guard = self.session.lock().map_err(failed)?;
        // The guard stays on this thread; the session it lends moves to the network thread.
        let session: &mut Session = &mut guard;
        let target = self.own_target(session, peer_id, group_id, message_id)?;
        let network = active_network(&self.dir)?;
        on_network_stack(|| {
            self.runtime.block_on(async {
                let mut node = connect(&network).await?;
                let nodes = std::slice::from_mut(&mut node);
                let sent = match text {
                    Some(text) => {
                        session
                            .send_edit(nodes, group_id, target, text.as_bytes(), &self.seal_key)
                            .await
                    }
                    None => {
                        session
                            .send_delete(nodes, group_id, target, &self.seal_key)
                            .await
                    }
                };
                sent.map(|id| id.to_vec()).map_err(failed)
            })
        })
    }
}
