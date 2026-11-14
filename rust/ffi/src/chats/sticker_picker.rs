//! Thin picker records; bounds, pixels, labels and recent resolution stay in shared core.
use super::{failed, ChatSession};
use crate::CoreError;
use dyapp_client::sticker_picker as core;

/// One core-selected manifest choice with bounded RGBA, never compressed peer bytes.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct StickerPickerEntry {
    /// Manifest position for saved-sticker submission.
    pub index: u32,
    /// Validated selected grapheme.
    pub emoji: String,
    /// Core thumbnail width, at most 96.
    pub width: u32,
    /// Core thumbnail height, at most 96.
    pub height: u32,
    /// Row-major RGBA8, exactly width * height * 4 bytes.
    pub pixels: Vec<u8>,
}
impl TryFrom<core::Choice> for StickerPickerEntry {
    type Error = CoreError;
    fn try_from(value: core::Choice) -> Result<Self, Self::Error> {
        Ok(Self {
            index: u32::try_from(value.index).map_err(failed)?,
            emoji: value.emoji,
            width: value.frame.width,
            height: value.frame.height,
            pixels: value.frame.pixels,
        })
    }
}

/// One immutable saved pack in core manifest order, with at most eight thumbnails.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct SavedStickerPickerPage {
    /// Authenticated 32-byte manifest identity.
    pub pack_id: Vec<u8>,
    /// Validated pack name.
    pub name: String,
    /// Total entries in this immutable pack.
    pub total: u32,
    /// At most eight core-selected choices.
    pub entries: Vec<StickerPickerEntry>,
    /// Exclusive next manifest offset.
    pub next: Option<u32>,
}
impl TryFrom<core::Page> for SavedStickerPickerPage {
    type Error = CoreError;
    fn try_from(value: core::Page) -> Result<Self, Self::Error> {
        Ok(Self {
            pack_id: value.pack_id.to_vec(),
            name: value.name,
            total: u32::try_from(value.total).map_err(failed)?,
            entries: value
                .entries
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            next: value.next.map(u32::try_from).transpose().map_err(failed)?,
        })
    }
}

/// One available recent choice, resolved by exact hash and emoji in an authenticated saved pack.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct RecentStickerPickerEntry {
    /// Authenticated manifest identity.
    pub pack_id: Vec<u8>,
    /// Validated pack name.
    pub pack_name: String,
    /// Safe selected label, manifest position and pixels.
    pub sticker: StickerPickerEntry,
}

/// Core-filtered recent choices; the cursor refers to the original bounded recent list.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct RecentStickerPickerPage {
    /// At most eight available choices.
    pub entries: Vec<RecentStickerPickerEntry>,
    /// Original-list offset of the next available choice, not the filtered count.
    pub next: Option<u32>,
}
impl TryFrom<core::recents::Page> for RecentStickerPickerPage {
    type Error = CoreError;
    fn try_from(value: core::recents::Page) -> Result<Self, Self::Error> {
        let entries = value
            .entries
            .into_iter()
            .map(|entry| {
                Ok(RecentStickerPickerEntry {
                    pack_id: entry.pack_id.to_vec(),
                    pack_name: entry.pack_name,
                    sticker: entry.sticker.try_into()?,
                })
            })
            .collect::<Result<_, CoreError>>()?;
        Ok(Self {
            entries,
            next: value.next.map(u32::try_from).transpose().map_err(failed)?,
        })
    }
}

#[uniffi::export]
impl ChatSession {
    /// Read a bounded saved-pack picker page offline. Core authenticates and prepares the pixels.
    ///
    /// # Errors
    /// Returns invalidPackId for malformed identity, notFound for absent content, or locking,
    /// authentication, invalidIndex, decoding or projection errors.
    pub fn saved_sticker_page(
        &self,
        pack_id: Vec<u8>,
        after: u32,
    ) -> Result<SavedStickerPickerPage, CoreError> {
        let id: [u8; 32] = pack_id.try_into().map_err(|_| failed("invalidPackId"))?;
        let guard = self.session.lock().map_err(failed)?;
        let pack = guard
            .store()
            .saved_sticker_pack(&self.seal_key, &id)
            .map_err(failed)?
            .ok_or_else(|| failed("notFound"))?;
        core::page(&pack, usize::try_from(after).map_err(failed)?)
            .map_err(failed)?
            .try_into()
    }

    /// Read available recent choices offline, without deleting unavailable files or metadata.
    /// Call off the UI thread; the native identity/session owner remains held through completion.
    ///
    /// # Errors
    /// Returns locking, invalidIndex, storage/decode or projection failures, never a partial page.
    pub fn recent_sticker_page(&self, after: u32) -> Result<RecentStickerPickerPage, CoreError> {
        let guard = self.session.lock().map_err(failed)?;
        // The synchronous guarded call cannot outlive its owner; core still bounds work and pages.
        core::recents::page(
            guard.store(),
            &self.seal_key,
            usize::try_from(after).map_err(failed)?,
            || false,
        )
        .map_err(failed)?
        .try_into()
    }
}
