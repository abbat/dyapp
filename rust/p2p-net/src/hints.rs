//! What a client may take from a node's self-reported `NodeInfo`
//! (docs/decisions/0016-protocol-evolution.md).

use crate::proto::NodeInfo;

/// Highest profile PoW a client computes for a node's hint; a higher hint skips the node.
pub const MAX_HINT_POW_BITS: u32 = 20;
/// A profile size hint below this skips the node.
pub const MIN_PROFILE_HINT_BYTES: u64 = 64 << 10;
/// A media size hint below this skips the node.
pub const MIN_MEDIA_HINT_BYTES: u64 = 256 << 10;

/// The values a client uses against one node: its compiled floors, tightened but never
/// loosened by the node's hints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Profile PoW to compute, in leading zero bits.
    pub pow_bits: u32,
    /// Largest profile record to send.
    pub profile_bytes: u64,
    /// Largest media blob to send.
    pub media_bytes: u64,
}

impl Limits {
    /// `self` are the client's compiled floors. A hint may raise PoW (up to
    /// [`MAX_HINT_POW_BITS`]) and lower sizes; zero means absent. `None` skips the node: it asks
    /// for more PoW than a client computes or for sizes no current client fits in.
    #[must_use]
    pub fn with_hints(self, info: &NodeInfo) -> Option<Self> {
        let size = |hint: u64, own: u64, min: u64| match hint {
            0 => Some(own),
            h if h < min => None,
            h => Some(h.min(own)),
        };
        let pow_bits = self.pow_bits.max(info.min_profile_pow);
        if pow_bits > MAX_HINT_POW_BITS.max(self.pow_bits) {
            return None;
        }
        Some(Self {
            pow_bits,
            profile_bytes: size(
                info.max_profile_bytes,
                self.profile_bytes,
                MIN_PROFILE_HINT_BYTES,
            )?,
            media_bytes: size(info.max_media_bytes, self.media_bytes, MIN_MEDIA_HINT_BYTES)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::{Capability, Role};
    use prost::Message;

    const OWN: Limits = Limits {
        pow_bits: 8,
        profile_bytes: 1 << 20,
        media_bytes: 6 << 20,
    };

    #[test]
    fn hostile_hints_cannot_lower_pow_or_raise_limits() {
        let hostile = NodeInfo {
            min_profile_pow: 0,
            max_profile_bytes: u64::MAX,
            max_media_bytes: u64::MAX,
            ..NodeInfo::default()
        };
        assert_eq!(OWN.with_hints(&hostile), Some(OWN));
        assert_eq!(OWN.with_hints(&NodeInfo::default()), Some(OWN));
        let strict = NodeInfo {
            min_profile_pow: 12,
            max_profile_bytes: 64 << 10,
            ..NodeInfo::default()
        };
        let tightened = Limits {
            pow_bits: 12,
            profile_bytes: 64 << 10,
            ..OWN
        };
        assert_eq!(OWN.with_hints(&strict), Some(tightened));
        for skip in [
            NodeInfo {
                min_profile_pow: 21,
                ..NodeInfo::default()
            },
            NodeInfo {
                max_profile_bytes: 1,
                ..NodeInfo::default()
            },
            NodeInfo {
                max_media_bytes: 1,
                ..NodeInfo::default()
            },
        ] {
            assert_eq!(OWN.with_hints(&skip), None);
        }
    }

    /// `NodeInfo` as nodes sent it before capabilities.
    #[derive(Clone, PartialEq, Message)]
    struct OldNodeInfo {
        #[prost(enumeration = "Role", repeated, tag = "1")]
        roles: Vec<i32>,
        #[prost(uint32, tag = "11")]
        puzzle_bits: u32,
    }

    #[test]
    fn old_and_new_node_info_decode_both_ways() {
        let new = NodeInfo {
            roles: vec![Role::Store.into()],
            puzzle_bits: 3,
            capabilities: vec![Capability::ProfileHeartbeat.into(), 99],
            ..NodeInfo::default()
        };
        let old = OldNodeInfo::decode(new.encode_to_vec().as_slice()).unwrap();
        assert_eq!((old.roles.clone(), old.puzzle_bits), (new.roles.clone(), 3));
        let back = NodeInfo::decode(old.encode_to_vec().as_slice()).unwrap();
        assert_eq!((&back.roles, back.puzzle_bits), (&new.roles, 3));
        assert!(back.capabilities.is_empty());
        // An unknown capability from a newer node stays a number the client ignores.
        let newer = NodeInfo::decode(new.encode_to_vec().as_slice()).unwrap();
        assert_eq!(newer.capabilities[1], 99);
        assert!(Capability::try_from(99).is_err());
    }
}
