//! Native projections of core-authenticated, bounded saved-pack collection pages.
use super::{failed, ChatSession};
use crate::CoreError;
use dyapp_client::store::sticker_packs::list;

/// Authenticated pack labels, or a recoverable entry failure without trusted display text.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum SavedStickerPackState {
    /// Validated metadata from the identity-bound saved file.
    Ready {
        /// Display-safe manifest name.
        name: String,
        /// Number of validated entries.
        stickers: u32,
    },
    /// The file remains saved but cannot currently be authenticated/read.
    Unreadable {
        /// Diagnostic text; not a trusted pack label.
        detail: String,
    },
}

/// One core-selected saved filename with its actual encrypted size.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct SavedStickerPackEntry {
    /// Exactly 32 bytes of manifest identity.
    pub pack_id: Vec<u8>,
    /// Actual encrypted file size, including its local header.
    pub bytes: u64,
    /// Authenticated metadata or a recoverable unreadable result.
    pub state: SavedStickerPackState,
}

/// A bounded page in core `PackID` order; counts include files outside the current page.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct SavedStickerPackPage {
    /// Entries in the order selected by core storage.
    pub entries: Vec<SavedStickerPackEntry>,
    /// Exclusive core cursor; absent when no further page was found.
    pub next: Option<Vec<u8>>,
    /// All regular-file bytes, including interrupted writes.
    pub used_bytes: u64,
    /// Files without canonical `PackID` names.
    pub unrecognized_files: u64,
}

fn entry(value: list::Entry) -> Result<SavedStickerPackEntry, CoreError> {
    let state = match value.details {
        Ok(details) => SavedStickerPackState::Ready {
            name: details.name,
            stickers: u32::try_from(details.stickers).map_err(failed)?,
        },
        Err(error) => SavedStickerPackState::Unreadable {
            detail: error.to_string(),
        },
    };
    Ok(SavedStickerPackEntry {
        pack_id: value.id.to_vec(),
        bytes: value.bytes,
        state,
    })
}

#[uniffi::export]
impl ChatSession {
    /// Read one identity-local collection page without networking or removing unreadable files.
    /// Core owns bounds, ordering, authentication, accounting and cursor selection.
    ///
    /// # Errors
    /// Returns invalidCursor unless a supplied cursor is exactly 32 bytes; otherwise locking,
    /// directory/enumeration or core page errors. Entry failures remain explicit entry states.
    pub fn saved_sticker_packs(
        &self,
        after: Option<Vec<u8>>,
    ) -> Result<SavedStickerPackPage, CoreError> {
        let cursor: Option<[u8; 32]> = after
            .map(TryInto::try_into)
            .transpose()
            .map_err(|_| failed("invalidCursor"))?;
        let guard = self.session.lock().map_err(failed)?;
        let page = guard
            .store()
            .saved_sticker_packs(&self.seal_key, cursor.as_ref())
            .map_err(failed)?;
        Ok(SavedStickerPackPage {
            entries: page
                .entries
                .into_iter()
                .map(entry)
                .collect::<Result<_, _>>()?,
            next: page.next.map(|id| id.to_vec()),
            used_bytes: page.used_bytes,
            unrecognized_files: page.unrecognized_files,
        })
    }
}
