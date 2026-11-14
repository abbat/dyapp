//! Shared received-pack status and reconciliation; adapters only project these typed outcomes.
use crate::sticker_media;
use crate::store::messages::{ChatSticker, MessageKey};
use crate::store::sticker_packs::{
    exchange::{
        download::{PackDownloadFailure, PackDownloadJob, PackDownloadState},
        PendingPack,
    },
    Error as PackError,
};
use crate::store::{Conversation, Store};

/// Current received-pack save state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// No current intent or readable saved pack.
    New,
    /// The readable saved pack contains the clicked sticker.
    Saved,
    /// Waiting for a peer descriptor.
    Waiting,
    /// A descriptor is admitted for download.
    Downloading,
    /// A typed local failure is available.
    Failed,
}

/// Display-safe failure category; never raw peer content or a capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    /// Transport/attachment fetch failed.
    Fetch,
    /// Received pack validation failed.
    Invalid,
    /// Saved-file quota is full.
    Quota,
    /// Saved content/storage cannot be read.
    Storage,
    /// A readable installed pack does not contain the clicked sticker.
    StickerMissing,
}
impl From<PackDownloadFailure> for Failure {
    fn from(value: PackDownloadFailure) -> Self {
        match value {
            PackDownloadFailure::Fetch => Self::Fetch,
            PackDownloadFailure::Invalid => Self::Invalid,
            PackDownloadFailure::Quota => Self::Quota,
            PackDownloadFailure::Storage => Self::Storage,
        }
    }
}

/// Identity-local status without encryption keys or raw answer descriptors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    /// Authoritative state for the selected received sticker.
    pub state: State,
    /// Current intent generation, absent for new/saved.
    pub generation: Option<[u8; 16]>,
    /// Typed failure when failed.
    pub failure: Option<Failure>,
    /// Core retry deadline only when a descriptor exists.
    pub retry_at_ms: Option<u64>,
}
impl Status {
    fn plain(state: State, generation: Option<[u8; 16]>) -> Self {
        Self {
            state,
            generation,
            failure: None,
            retry_at_ms: None,
        }
    }
    fn failed(reason: Failure, generation: Option<[u8; 16]>) -> Self {
        Self {
            failure: Some(reason),
            ..Self::plain(State::Failed, generation)
        }
    }
    /// Project stored retry state, preserving its exact intent generation and deadline.
    #[must_use]
    pub fn from_download(value: &PackDownloadState, token: [u8; 16]) -> Self {
        let failure = value.failure.map(Into::into);
        Self {
            state: if failure.is_some() {
                State::Failed
            } else if value.has_answer {
                State::Downloading
            } else {
                State::Waiting
            },
            generation: Some(token),
            failure,
            retry_at_ms: value.has_answer.then_some(value.retry_at_ms),
        }
    }
}

/// Selection, reconciliation or persistent-state failure.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Complete current-message selection refused.
    #[error(transparent)]
    Media(#[from] sticker_media::Error),
    /// Persistent request metadata failed.
    #[error(transparent)]
    Store(#[from] crate::store::Error),
    /// Saved-file reconciliation failed.
    #[error(transparent)]
    Pack(#[from] PackError),
    /// The generation disappeared during status lookup.
    #[error("requestChanged")]
    RequestChanged,
}

fn reconcile(
    store: &mut Store,
    seal: &[u8; 32],
    peer: &str,
    item: &ChatSticker,
    pack: &crate::sticker_pack::Pack,
    pending: &PendingPack,
) -> Result<(), Error> {
    if pack
        .stickers()
        .iter()
        .any(|entry| entry.sha256 == pending.sticker_hash)
        && store.conversation(peer)? == Conversation::Open
    {
        let job = PackDownloadJob {
            peer: peer.into(),
            pack_id: *pack.id(),
            sticker_hash: pending.sticker_hash,
            token: pending.token,
        };
        store.save_requested_sticker_pack(seal, &item.key.group, &job, pack)?;
    }
    Ok(())
}

fn installed(
    store: &mut Store,
    seal: &[u8; 32],
    peer: &str,
    item: &ChatSticker,
    pending: Option<&PendingPack>,
) -> Result<Option<Status>, Error> {
    let generation = pending.map(|value| value.token);
    let pack = match store.saved_sticker_pack(seal, &item.sticker.pack_id) {
        Ok(Some(pack)) => pack,
        Ok(None) => return Ok(None),
        Err(_) => return Ok(Some(Status::failed(Failure::Storage, generation))),
    };
    if !pack
        .stickers()
        .iter()
        .any(|entry| entry.sha256 == item.sticker.sha256)
    {
        return Ok(Some(Status::failed(Failure::StickerMissing, generation)));
    }
    if let Some(pending) = pending {
        reconcile(store, seal, peer, item, &pack, pending)?;
    }
    Ok(Some(Status::plain(State::Saved, None)))
}

/// Read status for a full current received-message key; reconcile a valid already-saved intent.
/// This can complete local request state and therefore requires guarded mutable storage access.
///
/// # Errors
/// Returns selection failures (including an own-authored message), request/storage failures,
/// or reconciliation failure. An unreadable saved pack remains explicit Failed/Storage.
pub fn status(
    store: &mut Store,
    seal: &[u8; 32],
    peer: &str,
    key: &MessageKey,
) -> Result<Status, Error> {
    let item = sticker_media::select(store, peer, key)?.message().clone();
    if item.key.author != peer {
        return Err(sticker_media::Error::UnknownMessage.into());
    }
    let pending = store.pending_sticker_pack(peer, &item.sticker.pack_id)?;
    if let Some(status) = installed(store, seal, peer, &item, pending.as_ref())? {
        return Ok(status);
    }
    let Some(pending) = pending else {
        return Ok(Status::plain(State::New, None));
    };
    let state = store
        .sticker_pack_download_state(peer, &item.sticker.pack_id)?
        .ok_or(Error::RequestChanged)?;
    Ok(Status::from_download(&state, pending.token))
}

#[cfg(test)]
mod tests;
