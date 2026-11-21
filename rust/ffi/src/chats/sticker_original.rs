//! Stored sticker retrieval projects only core-validated RGBA, never compressed peer media.
use super::{failed, ChatSession};
use crate::peer_photos::reachable;
use crate::profile_network::{active_network, on_network_stack};
use crate::{network_metered, CoreError};
use dyapp_client::store::messages::MessageKey;
use dyapp_client::{attachment, sticker_media, Error as ClientError};
use dyapp_p2p_net::proto::Status;

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
/// Typed display outcome; compressed media and retrieval descriptors never cross this boundary.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ChatStickerImage {
    /// Core-validated bounded first-frame pixels.
    Ready {
        /// Actual decoded RGBA8 image.
        frame: ChatStickerFrame,
    },
    /// Automatic retrieval was refused; an explicit load may try again.
    OnTap,
    /// No usable local/origin data is available for this current message.
    Unavailable,
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
    /// Compatibility frame API, preserving the existing onTap/notFound error codes.
    ///
    /// # Errors
    /// Returns typed read failures and the established onTap/notFound compatibility errors.
    pub fn chat_sticker_frame(
        &self,
        peer_id: String,
        group_id: Vec<u8>,
        author: String,
        id: Vec<u8>,
        manual: bool,
    ) -> Result<ChatStickerFrame, CoreError> {
        match self.chat_sticker_image(peer_id, group_id, author, id, manual)? {
            ChatStickerImage::Ready { frame } => Ok(frame),
            ChatStickerImage::OnTap => Err(failed("onTap")),
            ChatStickerImage::Unavailable => Err(failed("notFound")),
        }
    }

    /// Read a current stored sticker by full peer/group/author/logical-ID identity.
    /// Local originals/saved packs precede network access; manual is explicit tap permission.
    /// Blocks off the UI thread and retains session/identity ownership through completion.
    /// Callers must keep their presentation generation guard after UI cancellation.
    ///
    /// # Errors
    /// Returns current-message/core or network/transport failures. On-tap refusal and missing
    /// origin/descriptor data are explicit outcomes rather than strings for native UI to parse.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn chat_sticker_image(
        &self,
        peer_id: String,
        group_id: Vec<u8>,
        author: String,
        id: Vec<u8>,
        manual: bool,
    ) -> Result<ChatStickerImage, CoreError> {
        let key = MessageKey {
            group: group_id,
            author,
            id: id.try_into().map_err(|_| failed("unknownMessage"))?,
        };
        on_network_stack(|| {
            let session = self.session.lock().map_err(failed)?;
            let store = session.store();
            let selected = sticker_media::select(store, &peer_id, &key).map_err(failed)?;
            let descriptor = match selected
                .access(store, &self.dir, &self.seal_key, network_metered(), manual)
                .map_err(failed)?
            {
                sticker_media::Access::Local(frame) => {
                    return Ok(ChatStickerImage::Ready {
                        frame: frame.into(),
                    });
                }
                sticker_media::Access::OnTap => return Ok(ChatStickerImage::OnTap),
                sticker_media::Access::Unavailable => return Ok(ChatStickerImage::Unavailable),
                sticker_media::Access::Download(descriptor) => descriptor,
            };
            let network = active_network(&self.dir)?;
            let scores = self.contact_context.lock().map_err(failed)?.scores();
            let mut scores = self.runtime.block_on(scores.lock());
            let bytes = self.runtime.block_on(async {
                let mut nodes = reachable(&network).await?;
                if nodes.is_empty() {
                    return Err(failed("notConnected"));
                }
                match attachment::fetch_with_scores(&mut nodes, descriptor, &mut scores).await {
                    Ok(bytes) => Ok(Some(bytes)),
                    Err(ClientError::Status(Status::NotFound)) => Ok(None),
                    Err(error) => Err(super::original::fetch_error(error)),
                }
            })?;
            let Some(bytes) = bytes else {
                return Ok(ChatStickerImage::Unavailable);
            };
            selected
                .finish(store, &self.dir, &self.seal_key, &bytes)
                .map(|frame| ChatStickerImage::Ready {
                    frame: frame.into(),
                })
                .map_err(failed)
        })
    }
}
