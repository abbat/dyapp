//! Picked TAR input delegates format and persistence rules to their existing core owners.
use super::{failed, ChatSession};
use crate::CoreError;
use dyapp_client::{sticker_pack, sticker_tar, store::Store};

/// Authoritative picked-file bound for OS stream reading before crossing the native boundary.
#[uniffi::export]
#[must_use]
pub fn sticker_import_byte_limit() -> u64 {
    sticker_tar::MAX_ARCHIVE as u64
}

/// A successful save acknowledgement; failure does not prove that no file was published.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct SavedStickerPackImport {
    /// Validated manifest identity.
    pub pack_id: Vec<u8>,
    /// True for a newly saved pack; false when the same readable pack already existed.
    pub added: bool,
}

#[uniffi::export]
impl ChatSession {
    /// Import a picked TAR into this identity's sealed collection, without network publication.
    /// Validation precedes storage work. The same core validator/store are used by desktop.
    ///
    /// # Errors
    /// Returns core archive/pack validation, quota, key or persistence failures.
    /// A persistence error can follow file publication: refresh the authoritative collection
    /// instead of assuming rollback or automatically importing another copy.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn import_sticker_pack(
        &self,
        archive: Vec<u8>,
    ) -> Result<SavedStickerPackImport, CoreError> {
        let pack = sticker_pack::import_tar(&archive).map_err(failed)?;
        let _guard = self.session.lock().map_err(failed)?;
        let mut store =
            Store::open(&self.dir.join("client.db"), &self.key.identity).map_err(failed)?;
        let added = store
            .save_sticker_pack(&self.seal_key, &pack)
            .map_err(failed)?;
        Ok(SavedStickerPackImport {
            pack_id: pack.id().to_vec(),
            added,
        })
    }
}
