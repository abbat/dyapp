//! Bounded pack-control payloads; decoded metadata alone never grants authorization.
use super::{attachment_wire::AttachmentWire, Attachment, AttachmentMessage};
use crate::Error;
use prost::Message;

const MAX_BYTES: usize = 2048;
fn bad() -> Error {
    Error::Reply("bad sticker pack control")
}

/// A request for a manifest digest, scoped by the authenticated chat carrying it.
#[derive(Clone, PartialEq, Eq)]
pub struct PackRequest {
    /// Immutable manifest digest, never a URL or a local filename.
    pub pack_id: [u8; 32],
}

/// An answer to an outstanding request; the pack is downloaded and validated separately.
#[derive(Clone, PartialEq, Eq)]
pub struct PackAnswer {
    /// Requested manifest digest, independent of the attachment plaintext hash.
    pub pack_id: [u8; 32],
    /// Peer descriptor without owner release capability.
    pub attachment: Attachment,
}

#[derive(Clone, PartialEq, Message)]
struct RequestWire {
    #[prost(bytes = "vec", tag = "1")]
    pack_id: Vec<u8>,
}
#[derive(Clone, PartialEq, Message)]
struct AnswerWire {
    #[prost(bytes = "vec", tag = "1")]
    pack_id: Vec<u8>,
    #[prost(message, optional, tag = "2")]
    attachment: Option<AttachmentWire>,
}

fn bounded(bytes: &[u8]) -> Result<(), Error> {
    if bytes.is_empty() || bytes.len() > MAX_BYTES {
        return Err(bad());
    }
    Ok(())
}

impl PackRequest {
    /// Encode the request payload without an outer chat tag.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        RequestWire {
            pack_id: self.pack_id.to_vec(),
        }
        .encode_to_vec()
    }
    /// Bound input before protobuf allocation, then require an exact digest.
    ///
    /// # Errors
    ///
    /// Returns an error if the payload is empty, oversized, or malformed, or the
    /// manifest digest is not exactly 32 bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        bounded(bytes)?;
        let wire = RequestWire::decode(bytes).map_err(|_| bad())?;
        Ok(Self {
            pack_id: wire.pack_id.try_into().map_err(|_| bad())?,
        })
    }
}

impl PackAnswer {
    fn check(&self) -> Result<(), Error> {
        let attachment = &self.attachment;
        attachment.check()?;
        let size = usize::try_from(attachment.size).map_err(|_| bad())?;
        if size == 0
            || attachment::sealed_size(size).is_none_or(|size| size > 6 * 1024 * 1024)
            || !attachment.preview.is_empty()
            || attachment.release_secret != [0; 32]
        {
            return Err(bad());
        }
        Ok(())
    }
    /// Encode a validated answer payload without an outer tag.
    ///
    /// # Errors
    ///
    /// Returns an error if attachment metadata is invalid, the sealed pack size
    /// exceeds its limit, a preview or release capability is present, or the
    /// encoded answer exceeds its byte limit.
    pub fn encode(&self) -> Result<Vec<u8>, Error> {
        self.check()?;
        let bytes = self.encode_unchecked();
        bounded(&bytes)?;
        Ok(bytes)
    }
    pub(super) fn encode_unchecked(&self) -> Vec<u8> {
        AnswerWire {
            pack_id: self.pack_id.to_vec(),
            attachment: Some(AttachmentWire::from(&AttachmentMessage {
                id: [0; 16],
                hlc: 0,
                attachment: self.attachment.clone(),
            })),
        }
        .encode_to_vec()
    }
    /// Parse bounded metadata only; this does not authorize fetching or saving a pack.
    ///
    /// # Errors
    ///
    /// Returns an error if the payload is empty, oversized, or malformed, its
    /// digest or attachment is missing or invalid, message context is nonzero,
    /// or attachment validation fails as described in [`Self::encode`].
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        bounded(bytes)?;
        let wire = AnswerWire::decode(bytes).map_err(|_| bad())?;
        let message = wire.attachment.ok_or_else(bad)?.open().ok_or_else(bad)?;
        if message.id != [0; 16] || message.hlc != 0 {
            return Err(bad());
        }
        let answer = Self {
            pack_id: wire.pack_id.try_into().map_err(|_| bad())?,
            attachment: message.attachment,
        };
        answer.check()?;
        Ok(answer)
    }
}

use crate::attachment;
#[cfg(test)]
mod tests;
