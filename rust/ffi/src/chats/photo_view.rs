//! Safe attachment metadata for native screens; fetch and release secrets stay in the store.

#[cfg(test)]
mod tests;

use super::ChatDelivery;
use dyapp_client::attachment::layout_size;
use dyapp_client::store::messages::ChatAttachment;

/// A stored photo message, without encryption keys, release secrets or fetch descriptors.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ChatPhoto {
    /// MLS group, part of the stable message identity.
    pub group_id: Vec<u8>,
    /// Verified author Peer ID.
    pub author: String,
    /// Message-scoped logical ID.
    pub id: Vec<u8>,
    /// Authenticated HLC; retain its logical component when merging history.
    pub hlc: u64,
    /// Authenticated wall time.
    pub sent_at_ms: u64,
    /// Claimed plaintext byte count, before downloading.
    pub size: u64,
    /// Bounded sender MIME type.
    pub mime: String,
    /// Bounded JPEG preview; native decoders must still validate it.
    pub preview: Vec<u8>,
    /// Layout width, aspect clamped by the core; not decoded dimensions.
    pub width: u32,
    /// Layout height, aspect clamped by the core.
    pub height: u32,
    /// Incoming and unread on this device.
    pub unread: bool,
    /// Forwarded media, without asserting an original author.
    pub forwarded: bool,
    /// Delivery state for own media.
    pub delivery: Option<ChatDelivery>,
}

impl From<ChatAttachment> for ChatPhoto {
    fn from(photo: ChatAttachment) -> Self {
        let sent_at_ms = photo.sent_at_ms();
        let media = photo.message.attachment;
        let (width, height) = layout_size(media.width, media.height);
        Self {
            group_id: photo.key.group,
            author: photo.key.author,
            id: photo.key.id.to_vec(),
            hlc: photo.message.hlc,
            sent_at_ms,
            size: media.size,
            mime: media.mime,
            preview: media.preview,
            width,
            height,
            unread: photo.unread,
            forwarded: photo.forwarded,
            delivery: photo.delivery.map(Into::into),
        }
    }
}
