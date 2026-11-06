//! Sticker payloads, not yet registered as an outer chat body.

use super::{attachment_wire::AttachmentWire, Attachment, AttachmentMessage};
use crate::sticker_manifest::json::{clean_label, valid_emoji};
use crate::sticker_manifest::MAX_STICKER_BYTES;
use crate::Error;
use prost::Message as _;

/// Maximum encoded metadata and descriptor, checked before protobuf allocation.
pub const MAX_PAYLOAD_BYTES: usize = 2048;

/// Metadata for an MLS-carried sticker; the compressed WebP travels separately.
#[derive(Clone, PartialEq, Eq)]
pub struct Sticker {
    /// Logical message ID, shared across recipient devices.
    pub id: [u8; 16],
    /// Original sender HLC, within the signed SQLite clock range.
    pub hlc: u64,
    /// SHA-256 of the original pack manifest.
    pub pack_id: [u8; 32],
    /// SHA-256 of the individual plaintext WebP.
    pub sha256: [u8; 32],
    /// One clean grapheme, shown when media is unavailable.
    pub emoji: String,
    /// Clean pack name, bounded like the manifest.
    pub pack_name: String,
    /// Optional media descriptor; a mismatched hash permits only the fallback.
    pub attachment: Option<Attachment>,
}

#[derive(Clone, PartialEq, Eq, prost::Message)]
struct Wire {
    #[prost(bytes = "vec", tag = "1")]
    id: Vec<u8>,
    #[prost(uint64, optional, tag = "2")]
    hlc: Option<u64>,
    #[prost(bytes = "vec", tag = "3")]
    pack_id: Vec<u8>,
    #[prost(bytes = "vec", tag = "4")]
    sha256: Vec<u8>,
    #[prost(string, tag = "5")]
    emoji: String,
    #[prost(string, tag = "6")]
    pack_name: String,
    #[prost(message, optional, tag = "7")]
    attachment: Option<AttachmentWire>,
}

fn bad() -> Error {
    Error::Reply("bad sticker")
}

impl Sticker {
    fn check(&self) -> Result<(), Error> {
        if i64::try_from(self.hlc).is_err()
            || !clean_label(&self.pack_name)
            || !valid_emoji(&self.emoji)
        {
            return Err(bad());
        }
        if let Some(attachment) = &self.attachment {
            attachment.check()?;
            if attachment.size == 0
                || attachment.size > MAX_STICKER_BYTES as u64
                || !attachment.preview.is_empty()
                || attachment.release_secret != [0; 32]
            {
                return Err(bad());
            }
        }
        Ok(())
    }

    /// Encodes a payload only; no outer chat tag is assigned here.
    pub fn encode(&self) -> Result<Vec<u8>, Error> {
        self.check()?;
        let wire = Wire {
            id: self.id.to_vec(),
            hlc: Some(self.hlc),
            pack_id: self.pack_id.to_vec(),
            sha256: self.sha256.to_vec(),
            emoji: self.emoji.clone(),
            pack_name: self.pack_name.clone(),
            attachment: self.attachment.as_ref().map(|attachment| {
                AttachmentWire::from(&AttachmentMessage {
                    id: self.id,
                    hlc: self.hlc,
                    attachment: attachment.clone(),
                })
            }),
        };
        let bytes = wire.encode_to_vec();
        if bytes.len() > MAX_PAYLOAD_BYTES {
            return Err(bad());
        }
        Ok(bytes)
    }

    /// Bounds allocations and validates metadata/descriptor; never decodes image bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.is_empty() || bytes.len() > MAX_PAYLOAD_BYTES {
            return Err(bad());
        }
        let wire = Wire::decode(bytes).map_err(|_| bad())?;
        let id = wire.id.try_into().map_err(|_| bad())?;
        let hlc = wire.hlc.ok_or_else(bad)?;
        let attachment = wire
            .attachment
            .map(|wire| {
                let message = wire.open().ok_or_else(bad)?;
                // Reuse tag-6 descriptors without accepting conflicting message context.
                if message.id != id || message.hlc != hlc {
                    return Err(bad());
                }
                Ok(message.attachment)
            })
            .transpose()?;
        let sticker = Self {
            id,
            hlc,
            pack_id: wire.pack_id.try_into().map_err(|_| bad())?,
            sha256: wire.sha256.try_into().map_err(|_| bad())?,
            emoji: wire.emoji,
            pack_name: wire.pack_name,
            attachment,
        };
        sticker.check()?;
        Ok(sticker)
    }

    /// A descriptor eligible for fetching; image hash and WebP validation still follow fetch.
    pub fn download(&self) -> Option<&Attachment> {
        self.check().ok()?;
        self.attachment
            .as_ref()
            .filter(|attachment| attachment.plaintext_sha256 == self.sha256)
    }
}

#[cfg(test)]
mod tests;
