//! Authenticated saved-entry pixels; compressed WebP stays behind the core decoder.
use super::{failed, ChatSession};
use crate::CoreError;

/// One immutable saved entry decoded to bounded, row-major RGBA8.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct SavedStickerFrame {
    /// Authenticated manifest identity.
    pub pack_id: Vec<u8>,
    /// Selected entry's manifest position.
    pub index: u32,
    /// Authenticated entry label.
    pub emoji: String,
    /// Authenticated compressed-content digest, not a transport descriptor.
    pub sha256: Vec<u8>,
    /// Actual decoded width.
    pub width: u32,
    /// Actual decoded height.
    pub height: u32,
    /// Exactly width * height * 4 RGBA8 bytes.
    pub pixels: Vec<u8>,
}

#[uniffi::export]
impl ChatSession {
    /// Read one selected saved frame without network work or an OS WebP decoder.
    /// Core authenticates the pack and bounds the decoder/output; the UI only displays RGBA.
    ///
    /// # Errors
    /// Returns invalidPackId, invalidIndex or notFound for invalid selections, corrupted for
    /// a refused frame, or the core saved-file/locking failure without removing the file.
    pub fn saved_sticker_frame(
        &self,
        pack_id: Vec<u8>,
        index: u32,
    ) -> Result<SavedStickerFrame, CoreError> {
        let pack_id: [u8; 32] = pack_id.try_into().map_err(|_| failed("invalidPackId"))?;
        let selected = usize::try_from(index).map_err(|_| failed("invalidIndex"))?;
        let guard = self.session.lock().map_err(failed)?;
        let pack = guard
            .store()
            .saved_sticker_pack(&self.seal_key, &pack_id)
            .map_err(failed)?
            .ok_or_else(|| failed("notFound"))?;
        let entry = pack
            .stickers()
            .get(selected)
            .ok_or_else(|| failed("invalidIndex"))?;
        let bytes = pack
            .sticker_bytes(selected)
            .ok_or_else(|| failed("invalidIndex"))?;
        let frame = dyapp_client::sticker_webp::decode::first_frame(bytes)
            .map_err(|_| failed("corrupted"))?;
        Ok(SavedStickerFrame {
            pack_id: pack_id.to_vec(),
            index,
            emoji: entry.emoji.clone(),
            sha256: entry.sha256.to_vec(),
            width: frame.width,
            height: frame.height,
            pixels: frame.pixels,
        })
    }
}
