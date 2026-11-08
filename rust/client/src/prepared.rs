//! Durable signed mailbox writes and envelope preparation.

use crate::{chat, Error};
use dyapp_identity::{Domain, Identity, SignedRecord};
use dyapp_p2p_net::proto;
use prost::Message as _;

/// A signed mailbox write that can be persisted and retried without changing its ID or bytes.
#[derive(Clone)]
pub struct PreparedEnvelope {
    mailbox: [u8; 32],
    id: [u8; 16],
    pub(super) record: SignedRecord,
    bytes: Vec<u8>,
}

impl PreparedEnvelope {
    /// Reopen exact bytes saved before the first network attempt.
    ///
    /// # Errors
    /// Returns [`Error::PreparedEnvelope`] for noncanonical record bytes, an invalid
    /// signature, an undecodable envelope, or a mailbox/ID with an invalid length.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, Error> {
        let record = SignedRecord::decode(bytes.as_slice()).map_err(|_| Error::PreparedEnvelope)?;
        if record.encode_to_vec() != bytes || record.verify(Domain::Envelope).is_err() {
            return Err(Error::PreparedEnvelope);
        }
        let envelope = proto::Envelope::decode(record.payload.as_slice())
            .map_err(|_| Error::PreparedEnvelope)?;
        let mailbox = envelope
            .mailbox
            .try_into()
            .map_err(|_| Error::PreparedEnvelope)?;
        let id = envelope
            .id
            .try_into()
            .map_err(|_| Error::PreparedEnvelope)?;
        Ok(Self {
            mailbox,
            id,
            record,
            bytes,
        })
    }

    /// The exact signed record bytes for the durable outbox.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The destination device mailbox.
    #[must_use]
    pub fn mailbox(&self) -> &[u8; 32] {
        &self.mailbox
    }

    /// The stable envelope ID, also used for idempotent retry.
    #[must_use]
    pub fn id(&self) -> &[u8; 16] {
        &self.id
    }

    /// The envelope this record signs; [`Self::from_bytes`] checked it decodes.
    pub(crate) fn envelope(&self) -> proto::Envelope {
        proto::Envelope::decode(self.record.payload.as_slice()).unwrap_or_default()
    }
}

impl std::fmt::Debug for PreparedEnvelope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedEnvelope")
            .field("bytes_len", &self.bytes.len())
            .finish_non_exhaustive()
    }
}

/// Sign an ordinary MLS mailbox message with a fresh one-off key, before any network write.
///
/// # Errors
/// Returns [`Error::Input`] for an invalid mailbox length, [`Error::Request`] if
/// envelope ID generation fails, or [`Error::PreparedEnvelope`] if reopening fails.
pub fn prepare_put(mailbox: &[u8], ciphertext: Vec<u8>) -> Result<PreparedEnvelope, Error> {
    prepare(mailbox, ciphertext, proto::EnvelopeClass::Message, None)
}

/// Like [`prepare_put`], but carrying `token`, a signed `WriteToken` for `mailbox`, and signed by
/// its `token_key` so the node bills it to the mailbox quota rather than the tokenless pool.
///
/// # Errors
/// Returns [`Error::Input`] for an invalid mailbox length or malformed token,
/// [`Error::Request`] if ID generation fails, or [`Error::PreparedEnvelope`] if
/// reopening the prepared record fails.
pub fn prepare_tokened_put(
    mailbox: &[u8],
    ciphertext: Vec<u8>,
    token_key: &Identity,
    token: &[u8],
) -> Result<PreparedEnvelope, Error> {
    prepare(
        mailbox,
        ciphertext,
        proto::EnvelopeClass::Message,
        Some((token_key, token)),
    )
}

/// A put of `class`, tokenless or with `(token_key, token)`. A signal whose signed payload is
/// over [`chat::MAX_SIGNAL_BYTES`] is refused here, never truncated.
pub(crate) fn prepare(
    mailbox: &[u8],
    ciphertext: Vec<u8>,
    class: proto::EnvelopeClass,
    token: Option<(&Identity, &[u8])>,
) -> Result<PreparedEnvelope, Error> {
    let one_off;
    let (signer, write_token) = if let Some((key, token)) = token {
        (
            key,
            Some(SignedRecord::decode(token).map_err(|_| Error::Input("bad write token"))?),
        )
    } else {
        one_off = Identity::generate();
        (&one_off, None)
    };
    let mailbox: [u8; 32] = mailbox
        .try_into()
        .map_err(|_| Error::Input("mailbox must be 32 bytes"))?;
    let mut id = [0u8; 16];
    getrandom::fill(&mut id).map_err(|error| Error::Request(error.to_string()))?;
    let envelope = proto::Envelope {
        id: id.to_vec(),
        mailbox: mailbox.to_vec(),
        ciphertext,
        class: class.into(),
        write_token,
    };
    if class == proto::EnvelopeClass::Signal && envelope.encoded_len() > chat::MAX_SIGNAL_BYTES {
        return Err(Error::Input("signal envelope over 1 KiB"));
    }
    let record = signer.sign(Domain::Envelope, envelope.encode_to_vec());
    PreparedEnvelope::from_bytes(record.encode_to_vec())
}
