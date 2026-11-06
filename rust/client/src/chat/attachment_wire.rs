//! Shared descriptor encoding for chat media; validation stays at the decode boundary.

use super::{Attachment, AttachmentMessage};

// Keep protobuf field numbers stable: these are already used by tag-6 messages.
#[derive(Clone, PartialEq, Eq, prost::Message)]
pub(super) struct AttachmentWire {
    #[prost(bytes = "vec", tag = "1")]
    pub(super) key: Vec<u8>,
    #[prost(bytes = "vec", tag = "2")]
    pub(super) plaintext_sha256: Vec<u8>,
    #[prost(bytes = "vec", tag = "3")]
    pub(super) blob_sha256: Vec<u8>,
    #[prost(bytes = "vec", tag = "4")]
    pub(super) release_secret: Vec<u8>,
    #[prost(uint64, tag = "5")]
    pub(super) size: u64,
    #[prost(bytes = "vec", tag = "6")]
    pub(super) preview: Vec<u8>,
    #[prost(string, tag = "7")]
    pub(super) mime: String,
    #[prost(uint32, tag = "8")]
    pub(super) width: u32,
    #[prost(uint32, tag = "9")]
    pub(super) height: u32,
    #[prost(bytes = "vec", tag = "10")]
    pub(super) message_id: Vec<u8>,
    #[prost(uint64, optional, tag = "11")]
    pub(super) hlc: Option<u64>,
}

impl From<&AttachmentMessage> for AttachmentWire {
    fn from(message: &AttachmentMessage) -> Self {
        let attachment = &message.attachment;
        Self {
            key: attachment.key.to_vec(),
            plaintext_sha256: attachment.plaintext_sha256.to_vec(),
            blob_sha256: attachment.blob_sha256.to_vec(),
            release_secret: attachment.release_secret.to_vec(),
            size: attachment.size,
            preview: attachment.preview.clone(),
            mime: attachment.mime.clone(),
            width: attachment.width,
            height: attachment.height,
            message_id: message.id.to_vec(),
            hlc: Some(message.hlc),
        }
    }
}

impl AttachmentWire {
    pub(super) fn open(self) -> Option<AttachmentMessage> {
        let id = self.message_id.try_into().ok()?;
        let hlc = self.hlc.filter(|value| i64::try_from(*value).is_ok())?;
        let attachment = Attachment {
            key: self.key.try_into().ok()?,
            plaintext_sha256: self.plaintext_sha256.try_into().ok()?,
            blob_sha256: self.blob_sha256.try_into().ok()?,
            release_secret: self.release_secret.try_into().ok()?,
            size: self.size,
            preview: self.preview,
            mime: self.mime,
            width: self.width,
            height: self.height,
        };
        attachment.check().ok()?;
        Some(AttachmentMessage {
            id,
            hlc,
            attachment,
        })
    }
}
