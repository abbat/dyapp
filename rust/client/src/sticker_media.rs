//! Current-message sticker selection, local sources and validated completion shared by clients.
use crate::attachment::{self, Attachment};
use crate::sticker_webp::decode::Frame;
use crate::store::messages::{ChatSticker, MessageKey};
use crate::store::{originals, Conversation, Store};
use std::path::Path;

/// Current sticker read refusal or storage failure.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The identity cannot be its own conversation.
    #[error("ownPeer")]
    OwnPeer,
    /// The conversation is blocked.
    #[error("blocked")]
    Blocked,
    /// No conversation is available.
    #[error("unknownPeer")]
    UnknownPeer,
    /// The complete current message key/content no longer matches.
    #[error("unknownMessage")]
    UnknownMessage,
    /// A snapshot cannot be completed under another identity.
    #[error("Identity changed")]
    IdentityChanged,
    /// Downloaded/plaintext bytes fail the sticker hash or decoder.
    #[error("corrupted")]
    Corrupted,
    /// Persistent state is unavailable or invalid.
    #[error(transparent)]
    Store(#[from] crate::store::Error),
}

/// Immutable selected metadata bound to its originating identity and conversation.
pub struct Snapshot {
    owner: String,
    peer: String,
    item: ChatSticker,
}

/// Select a complete current message key without networking or cache mutation.
///
/// # Errors
/// Returns own/blocked/unknown conversation or message refusals, or a storage error.
pub fn select(store: &Store, peer: &str, key: &MessageKey) -> Result<Snapshot, Error> {
    if peer == store.owner_peer_id() {
        return Err(Error::OwnPeer);
    }
    if store.blocked(peer)? {
        return Err(Error::Blocked);
    }
    if store.conversation(peer)? == Conversation::None {
        return Err(Error::UnknownPeer);
    }
    let item = store
        .current_chat_stickers(peer)?
        .into_iter()
        .find(|item| &item.key == key)
        .ok_or(Error::UnknownMessage)?;
    Ok(Snapshot {
        owner: store.owner_peer_id().into(),
        peer: peer.into(),
        item,
    })
}

impl Snapshot {
    /// Stored message for trusted adapters; project only safe identity/labels across FFI.
    #[must_use]
    pub fn message(&self) -> &ChatSticker {
        &self.item
    }

    /// Validated optional retrieval descriptor, never a caller-supplied capability.
    #[must_use]
    pub fn download(&self) -> Option<&Attachment> {
        self.item.sticker.download()
    }

    fn current(&self, store: &Store) -> Result<(), Error> {
        if store.owner_peer_id() != self.owner {
            return Err(Error::IdentityChanged);
        }
        let latest = select(store, &self.peer, &self.item.key)?;
        if latest.item.sticker != self.item.sticker {
            return Err(Error::UnknownMessage);
        }
        Ok(())
    }

    /// Read authenticated originals or a matching saved pack before considering network access.
    /// Unreadable/missing saved packs retain the existing network-fallback behavior.
    ///
    /// # Errors
    /// Returns current-message/identity/storage failures, or corruption of selected cached pixels.
    pub fn local(
        &self,
        store: &Store,
        dir: &Path,
        seal: &[u8; 32],
    ) -> Result<Option<Frame>, Error> {
        self.current(store)?;
        if let Some(bytes) = self
            .download()
            .and_then(|item| originals::read(dir, seal, item))
        {
            return self.finish(store, dir, seal, &bytes).map(Some);
        }
        let Some(pack) = store
            .saved_sticker_pack(seal, &self.item.sticker.pack_id)
            .ok()
            .flatten()
        else {
            return Ok(None);
        };
        let Some(index) = pack
            .stickers()
            .iter()
            .position(|entry| entry.sha256 == self.item.sticker.sha256)
        else {
            return Ok(None);
        };
        let bytes = pack.sticker_bytes(index).ok_or(Error::Corrupted)?;
        self.finish(store, dir, seal, bytes).map(Some)
    }

    /// Apply shared automatic-download policy using the adapter's current OS network report.
    /// Explicit manual permission is handled separately by the caller.
    ///
    /// # Errors
    /// Returns identity/current-message or storage failures.
    pub fn auto_download(&self, store: &Store, metered: bool) -> Result<bool, Error> {
        self.current(store)?;
        Ok(attachment::auto_download(
            store.auto_download()?,
            metered,
            store.conversation(&self.peer)? == Conversation::Open,
        ))
    }

    /// Revalidate the current stored message before decoding and best-effort cache retention.
    /// A failed cache write never discards validated display pixels.
    ///
    /// # Errors
    /// Returns identity/current-message, hash/decoder or storage/configuration failures.
    pub fn finish(
        &self,
        store: &Store,
        dir: &Path,
        seal: &[u8; 32],
        bytes: &[u8],
    ) -> Result<Frame, Error> {
        self.current(store)?;
        let frame = self
            .item
            .sticker
            .first_frame(bytes)
            .map_err(|_| Error::Corrupted)?;
        if let Some(descriptor) = self.download() {
            if store.media_cache_config()?.enabled
                && originals::keep(dir, seal, descriptor, bytes).is_ok()
            {
                let _ = store.sweep_originals(dir);
            }
        }
        Ok(frame)
    }
}

#[cfg(test)]
pub(crate) mod tests;
