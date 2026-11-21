//! Forward selected current local text without exporting original-author claims.
use crate::chat::MAX_TEXT_BYTES;
use crate::store::messages::ChatMessage;
use crate::store::{Conversation, Store};

/// A complete stored-message selector, never caller-supplied forwarded content.
#[derive(Clone, Copy)]
pub struct Source<'a> {
    /// The source conversation's peer.
    pub peer: &'a str,
    /// The source MLS group.
    pub group: &'a [u8],
    /// The stored message author, used only to select local content.
    pub author: &'a str,
    /// The source logical message ID.
    pub id: &'a [u8],
}

/// Forwarding preflight failures, before a new message is staged.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Source and target coincide, target is own/invalid, or target chat is not open.
    #[error("invalidTarget")]
    InvalidTarget,
    /// This identity cannot be used as a source conversation.
    #[error("ownPeer")]
    OwnPeer,
    /// The source conversation is unavailable.
    #[error("unknownPeer")]
    UnknownPeer,
    /// A source or target conversation is blocked.
    #[error("blocked")]
    Blocked,
    /// The full source key does not identify a current, live, bounded text.
    #[error("unknownMessage")]
    UnknownMessage,
    /// Reading persistent state failed.
    #[error(transparent)]
    Store(#[from] crate::store::Error),
}

fn text(messages: &[ChatMessage], source: Source<'_>, id: [u8; 16]) -> Option<Vec<u8>> {
    messages
        .iter()
        .find(|message| {
            message.key.group.as_slice() == source.group
                && message.key.author == source.author
                && message.key.id == id
                && !message.current.deleted
        })
        .and_then(|message| message.current.text.clone())
        .filter(|text| !text.is_empty() && text.len() <= MAX_TEXT_BYTES)
}

/// Read a frozen current text after source visibility and target-chat checks.
///
/// # Errors
/// Returns typed source/target/block errors, an absent/deleted/non-text message error,
/// or storage failure. This performs no network access or writes.
pub fn prepare(store: &Store, source: Source<'_>, target: &str) -> Result<Vec<u8>, Error> {
    if source.peer == target || crate::private_peer::check(store, target).is_err() {
        return Err(Error::InvalidTarget);
    }
    source_identity(store, source.peer)?;
    if store.blocked(source.peer)? || store.blocked(target)? {
        return Err(Error::Blocked);
    }
    source_conversation(store, source.peer)?;
    if store.conversation(target)? != Conversation::Open {
        return Err(Error::InvalidTarget);
    }
    selected_text(store, source)
}

fn source_identity(store: &Store, peer: &str) -> Result<(), Error> {
    crate::private_peer::check(store, peer).map_err(|error| match error {
        crate::private_peer::TargetError::Invalid => Error::UnknownPeer,
        crate::private_peer::TargetError::Own => Error::OwnPeer,
    })
}

fn source_conversation(store: &Store, peer: &str) -> Result<(), Error> {
    if store.conversation(peer)? == Conversation::None {
        return Err(Error::UnknownPeer);
    }
    Ok(())
}

fn selected_text(store: &Store, source: Source<'_>) -> Result<Vec<u8>, Error> {
    let id = source.id.try_into().map_err(|_| Error::UnknownMessage)?;
    text(&store.current_chat_messages(source.peer)?, source, id).ok_or(Error::UnknownMessage)
}

/// Project other open destinations for a current local text without staging any message.
/// The returned IDs are presentation choices, not authority for a later submission.
///
/// # Errors
/// Returns the existing source/block/selection errors or a persistent-state read failure.
pub fn targets(store: &Store, source: Source<'_>) -> Result<Vec<String>, Error> {
    source_identity(store, source.peer)?;
    if store.blocked(source.peer)? {
        return Err(Error::Blocked);
    }
    source_conversation(store, source.peer)?;
    selected_text(store, source)?;
    let mut targets = Vec::new();
    for row in store.chat_rows()? {
        if row.peer_id != source.peer
            && row.state == Conversation::Open
            && crate::private_peer::check(store, &row.peer_id).is_ok()
            && !store.blocked(&row.peer_id)?
        {
            targets.push(row.peer_id);
        }
    }
    Ok(targets)
}

#[cfg(test)]
pub(crate) mod tests;
