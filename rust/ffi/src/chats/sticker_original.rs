//! Stored sticker retrieval projects only core-validated RGBA, never compressed peer media.
use super::{failed, ChatSession};
use crate::peer_photos::reachable;
use crate::profile_network::{active_network, on_network_stack};
use crate::{network_metered, CoreError};
use dyapp_client::store::messages::MessageKey;
use dyapp_client::{attachment, sticker_media};

/// Bounded actual first-frame pixels of the selected current chat sticker.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ChatStickerFrame {
    /// Actual decoded width.
    pub width: u32,
    /// Actual decoded height.
    pub height: u32,
    /// Row-major RGBA8 bytes, exactly width * height * 4.
    pub pixels: Vec<u8>,
}
impl From<dyapp_client::sticker_webp::decode::Frame> for ChatStickerFrame {
    fn from(frame: dyapp_client::sticker_webp::decode::Frame) -> Self {
        Self {
            width: frame.width,
            height: frame.height,
            pixels: frame.pixels,
        }
    }
}

#[uniffi::export]
impl ChatSession {
    /// Read a current stored sticker by full peer/group/author/logical-ID identity.
    /// Local originals/saved packs precede network access; manual is explicit tap permission.
    /// Blocks off the UI thread and retains session/identity ownership through completion.
    /// Callers must keep their presentation generation guard after UI cancellation.
    ///
    /// # Errors
    /// Returns current-message/core errors, onTap when automatic download is refused,
    /// notFound without a usable descriptor, or network/transport failures.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn chat_sticker_frame(
        &self,
        peer_id: String,
        group_id: Vec<u8>,
        author: String,
        id: Vec<u8>,
        manual: bool,
    ) -> Result<ChatStickerFrame, CoreError> {
        let key = MessageKey {
            group: group_id,
            author,
            id: id.try_into().map_err(|_| failed("unknownMessage"))?,
        };
        on_network_stack(|| {
            let session = self.session.lock().map_err(failed)?;
            let store = session.store();
            let selected = sticker_media::select(store, &peer_id, &key).map_err(failed)?;
            if let Some(frame) = selected
                .local(store, &self.dir, &self.seal_key)
                .map_err(failed)?
            {
                return Ok(frame.into());
            }
            let descriptor = selected.download().ok_or_else(|| failed("notFound"))?;
            if !manual
                && !selected
                    .auto_download(store, network_metered())
                    .map_err(failed)?
            {
                return Err(failed("onTap"));
            }
            let network = active_network(&self.dir)?;
            let bytes = self.runtime.block_on(async {
                let mut nodes = reachable(&network).await?;
                if nodes.is_empty() {
                    return Err(failed("notConnected"));
                }
                attachment::fetch(&mut nodes, descriptor)
                    .await
                    .map_err(super::original::fetch_error)
            })?;
            selected
                .finish(store, &self.dir, &self.seal_key, &bytes)
                .map(Into::into)
                .map_err(failed)
        })
    }
}
