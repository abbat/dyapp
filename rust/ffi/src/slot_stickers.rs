//! Explicit saved-pack removal needs identity admission, never the media sealing secret.
use crate::{CoreError, MobileIdentityOperation};
use dyapp_client::store::Store;

#[uniffi::export]
impl MobileIdentityOperation {
    /// Remove only the selected saved pack after the caller's explicit confirmation.
    /// Retain this operation until blocking completion; no chat/media key is opened.
    /// Missing or corrupt pack content does not require decryption.
    ///
    /// # Errors
    /// Returns invalidPackId unless the selector is exactly 32 bytes, or a core storage failure.
    /// An error after unlink can mean removal happened: refresh before deciding whether to retry.
    /// This never erases peer copies, chat history or other packs.
    pub fn remove_saved_sticker_pack(&self, pack_id: Vec<u8>) -> Result<bool, CoreError> {
        let pack_id: [u8; 32] = pack_id
            .try_into()
            .map_err(|_| CoreError::Failed("invalidPackId".into()))?;
        let mut store = Store::open(
            &self.operation.slot().dir.join("client.db"),
            self.operation.identity(),
        )
        .map_err(|error| CoreError::Failed(error.to_string()))?;
        store
            .remove_saved_sticker_pack(&pack_id)
            .map_err(|error| CoreError::Failed(error.to_string()))
    }
}
