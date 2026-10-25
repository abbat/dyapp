//! Bounded plaintext codec for private interaction MLS application messages.
//! Sender and peer identities come from the verified MLS group, never this payload.

/// The exact size of a version-1 private interaction payload.
pub const ENCODED_LEN: usize = 26;
const VERSION: u8 = 1;

/// A private interaction sent inside an MLS application message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The sender likes the peer.
    Like,
    /// The sender withdraws its like.
    Unlike,
    /// The sender viewed the peer's profile.
    View,
}

impl Kind {
    const fn byte(self) -> u8 {
        match self {
            Self::Like => 1,
            Self::Unlike => 2,
            Self::View => 3,
        }
    }

    fn from_byte(byte: u8) -> Result<Self, DecodeError> {
        match byte {
            1 => Ok(Self::Like),
            2 => Ok(Self::Unlike),
            3 => Ok(Self::View),
            _ => Err(DecodeError::Kind),
        }
    }
}

/// One event with a random ID and HLC; `Store::interact` or `Store::draft_interaction` creates own ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interaction {
    /// Which action the sender took.
    pub kind: Kind,
    /// Random event ID, reused for retries.
    pub event_id: [u8; 16],
    /// Sender's persisted hybrid logical clock timestamp.
    pub hlc: u64,
}

/// Malformed or unsupported interaction plaintext.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    /// The payload is truncated or has trailing bytes.
    Length,
    /// The protocol version is unsupported.
    Version,
    /// The action kind is unsupported.
    Kind,
}

impl Interaction {
    /// Encode exactly one version-1 interaction. No peer identity or user text is included.
    pub fn encode(self) -> [u8; ENCODED_LEN] {
        let mut bytes = [0; ENCODED_LEN];
        bytes[0] = VERSION;
        bytes[1] = self.kind.byte();
        bytes[2..18].copy_from_slice(&self.event_id);
        bytes[18..].copy_from_slice(&self.hlc.to_be_bytes());
        bytes
    }

    /// Decode only the exact version-1 layout; never accept trailing data.
    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        if bytes.len() != ENCODED_LEN {
            return Err(DecodeError::Length);
        }
        if bytes[0] != VERSION {
            return Err(DecodeError::Version);
        }
        Ok(Self {
            kind: Kind::from_byte(bytes[1])?,
            event_id: bytes[2..18].try_into().map_err(|_| DecodeError::Length)?,
            hlc: u64::from_be_bytes(bytes[18..].try_into().map_err(|_| DecodeError::Length)?),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{DecodeError, Interaction, Kind, ENCODED_LEN};

    #[test]
    fn round_trip_every_kind() {
        for kind in [Kind::Like, Kind::Unlike, Kind::View] {
            let event = Interaction {
                kind,
                event_id: [0x42; 16],
                hlc: 0x0102_0304_0506_0708,
            };
            assert_eq!(Interaction::decode(&event.encode()), Ok(event));
        }
    }

    #[test]
    fn malformed_payload_never_decodes() {
        let valid = Interaction {
            kind: Kind::Like,
            event_id: [0; 16],
            hlc: 0,
        }
        .encode();
        for length in 0..ENCODED_LEN {
            assert_eq!(
                Interaction::decode(&valid[..length]),
                Err(DecodeError::Length)
            );
        }
        let mut extra = valid.to_vec();
        extra.push(0);
        assert_eq!(Interaction::decode(&extra), Err(DecodeError::Length));
        let mut bad = valid;
        bad[0] = 2;
        assert_eq!(Interaction::decode(&bad), Err(DecodeError::Version));
        bad = valid;
        bad[1] = 4;
        assert_eq!(Interaction::decode(&bad), Err(DecodeError::Kind));
    }
}
