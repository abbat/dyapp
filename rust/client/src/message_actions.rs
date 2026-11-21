//! Read-only full-key preflight for own text/sticker operations across native and desktop adapters.
use crate::store::messages::{ChatItem, MessageKey};
use crate::store::Store;

/// The adapter's existing media contract; deletion can admit either type without widening text APIs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Live own text, for text edit/delete compatibility.
    Text,
    /// Live own sticker, never text edit or forwarding.
    Sticker,
    /// Desktop deletion's existing live text-or-sticker contract.
    TextOrSticker,
}

/// Exact displayed selection. An author is never inferred from a foreign message's logical ID.
#[derive(Clone, Copy)]
pub struct Source<'a> {
    /// Viewed conversation peer.
    pub peer: &'a str,
    /// Selected MLS group.
    pub group: &'a [u8],
    /// Selected verified message author.
    pub author: &'a str,
    /// Selected logical message ID.
    pub id: &'a [u8],
}

/// Local selection failures before networking or staging.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The viewed peer is this store's owner.
    #[error("ownPeer")]
    OwnPeer,
    /// The viewed peer is blocked.
    #[error("blocked")]
    Blocked,
    /// No current live own target matches the exact selection and required media kind.
    #[error("unknownMessage")]
    UnknownMessage,
    /// Reading persistent state failed.
    #[error(transparent)]
    Store(#[from] crate::store::Error),
}

fn matches(item: &ChatItem, key: &MessageKey, kind: Kind) -> bool {
    match item {
        ChatItem::Text(message) => {
            kind != Kind::Sticker && message.key == *key && !message.current.deleted
        }
        ChatItem::Sticker(message) => kind != Kind::Text && message.key == *key,
        ChatItem::Attachment(_) => false,
    }
}

/// Resolve one live own selection without writes or networking. Call under the operation's store/session lease.
///
/// # Errors
/// Returns ownPeer/blocked/unknownMessage or storage failure. Ownership comes from the admitted [`Store`];
/// an adapter cannot substitute an author, wrong group, cleared message or a different media kind.
/// Actual session staging still revalidates its group and live target.
pub fn prepare(store: &Store, source: Source<'_>, kind: Kind) -> Result<[u8; 16], Error> {
    let own = store.owner_peer_id();
    if source.peer == own {
        return Err(Error::OwnPeer);
    }
    if store.blocked(source.peer)? {
        return Err(Error::Blocked);
    }
    if source.author != own {
        return Err(Error::UnknownMessage);
    }
    let id = source.id.try_into().map_err(|_| Error::UnknownMessage)?;
    let key = MessageKey {
        group: source.group.to_vec(),
        author: source.author.to_owned(),
        id,
    };
    let text = kind != Kind::Sticker
        && store
            .current_chat_messages(source.peer)?
            .into_iter()
            .any(|item| matches(&ChatItem::Text(item), &key, kind));
    let sticker = !text
        && kind != Kind::Text
        && store
            .current_chat_stickers(source.peer)?
            .into_iter()
            .any(|item| matches(&ChatItem::Sticker(item), &key, kind));
    if text || sticker {
        Ok(id)
    } else {
        Err(Error::UnknownMessage)
    }
}

#[cfg(test)]
mod fixture;
#[cfg(test)]
mod tests;
