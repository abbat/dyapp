//! Per-protocol payload bytes since start and since the last take; excludes transport and gossip
//! control traffic.

use crate::{
    DIGEST_PROTOCOL, INVENTORY_PROTOCOL, MAILBOX_PROTOCOL, MAILBOX_PUSH_PROTOCOL, MEDIA_PROTOCOL,
    NODE_PROTOCOL, PROFILE_PROTOCOL, SEARCH_PROTOCOL,
};
use libp2p::StreamProtocol;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

const SLOTS: [&str; 10] = [
    "node",
    "profile",
    "mailbox",
    "push",
    "media",
    "search",
    "inventory",
    "digest",
    "gossip",
    "other",
];

/// Direction of a payload relative to this node.
#[derive(Clone, Copy)]
pub enum Dir {
    /// Received.
    In = 0,
    /// Sent.
    Out = 1,
}

/// Share one instance across the node's codecs and application gossip handlers.
#[derive(Default)]
pub struct WireBytes {
    /// Bytes since start, `[in, out]` per slot; never reset.
    totals: [[AtomicU64; 2]; SLOTS.len()],
    /// `totals` at the last take.
    taken: [[AtomicU64; 2]; SLOTS.len()],
}

/// Counts, `[in, out]` per slot. Displays as `name=in/out …`.
pub struct WireCounts(pub [[u64; 2]; SLOTS.len()]);

impl WireBytes {
    /// Slot of application gossip payloads.
    pub const GOSSIP: usize = 8;
    /// Slot of unknown protocols.
    pub const OTHER: usize = 9;
    /// Slot names, in slot order.
    pub const NAMES: [&'static str; 10] = SLOTS;

    /// Slot for a protobuf request-response protocol, or `OTHER` for an unknown one.
    pub fn protocol_slot(protocol: &StreamProtocol) -> usize {
        [
            NODE_PROTOCOL,
            PROFILE_PROTOCOL,
            MAILBOX_PROTOCOL,
            MAILBOX_PUSH_PROTOCOL,
            MEDIA_PROTOCOL,
            SEARCH_PROTOCOL,
            INVENTORY_PROTOCOL,
            DIGEST_PROTOCOL,
        ]
        .iter()
        .position(|p| p == protocol)
        .unwrap_or(Self::OTHER)
    }

    /// Slot for this configured network, without interpreting arbitrary protocol suffixes.
    pub fn protocol_slot_for(protocol: &StreamProtocol, protocols: &crate::Protocols) -> usize {
        [
            &protocols.node,
            &protocols.profile,
            &protocols.mailbox,
            &protocols.mailbox_push,
            &protocols.media,
            &protocols.search,
            &protocols.inventory,
            &protocols.digest,
        ]
        .iter()
        .position(|p| *p == protocol)
        .unwrap_or(Self::OTHER)
    }

    /// Adds payload bytes to a slot returned by `protocol_slot`, `GOSSIP` or `OTHER`.
    ///
    /// # Panics
    /// Panics if `slot` is outside the fixed slot table.
    pub fn add(&self, slot: usize, dir: Dir, n: usize) {
        self.totals[slot][dir as usize].fetch_add(n as u64, Ordering::Relaxed);
    }

    /// Bytes since start; not globally atomic across slots and directions.
    pub fn totals(&self) -> WireCounts {
        WireCounts(
            self.totals
                .each_ref()
                .map(|slot| slot.each_ref().map(|n| n.load(Ordering::Relaxed))),
        )
    }

    /// Bytes since the last take; the totals go on. Concurrent additions land in this or the next
    /// interval, and concurrent takes never count a byte twice.
    pub fn take(&self) -> WireCounts {
        let totals = self.totals();
        WireCounts(std::array::from_fn(|slot| {
            std::array::from_fn(|dir| {
                let total = totals.0[slot][dir];
                total.saturating_sub(self.taken[slot][dir].fetch_max(total, Ordering::Relaxed))
            })
        }))
    }
}

impl fmt::Display for WireCounts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, (name, [incoming, outgoing])) in SLOTS.iter().zip(self.0).enumerate() {
            let separator = if i == 0 { "" } else { " " };
            write!(f, "{separator}{name}={incoming}/{outgoing}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_protocols_use_fixed_slots_and_foreign_ids_are_other() {
        let configured = crate::Protocols::new("wire-test").unwrap();
        for (slot, protocol) in [
            &configured.node,
            &configured.profile,
            &configured.mailbox,
            &configured.mailbox_push,
            &configured.media,
            &configured.search,
            &configured.inventory,
            &configured.digest,
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(WireBytes::protocol_slot_for(protocol, &configured), slot);
            assert_eq!(WireBytes::protocol_slot(protocol), WireBytes::OTHER);
        }
        assert_eq!(
            WireBytes::protocol_slot_for(&MEDIA_PROTOCOL, &configured),
            WireBytes::OTHER
        );
        let other = crate::Protocols::new("different-test").unwrap();
        assert_eq!(
            WireBytes::protocol_slot_for(&other.media, &configured),
            WireBytes::OTHER
        );
        let public = crate::Protocols::public();
        assert_eq!(WireBytes::protocol_slot_for(&MEDIA_PROTOCOL, &public), 4);
    }

    #[test]
    fn protocols_directions_display_and_reset() {
        let bytes = WireBytes::default();
        for (slot, protocol) in [
            NODE_PROTOCOL,
            PROFILE_PROTOCOL,
            MAILBOX_PROTOCOL,
            MAILBOX_PUSH_PROTOCOL,
            MEDIA_PROTOCOL,
            SEARCH_PROTOCOL,
            INVENTORY_PROTOCOL,
            DIGEST_PROTOCOL,
        ]
        .iter()
        .enumerate()
        {
            assert_eq!(WireBytes::protocol_slot(protocol), slot);
        }
        assert_eq!(
            WireBytes::protocol_slot(&crate::KAD_PROTOCOL),
            WireBytes::OTHER
        );
        let unknown = StreamProtocol::new("/unknown");
        assert_eq!(WireBytes::protocol_slot(&unknown), WireBytes::OTHER);

        let inventory = WireBytes::protocol_slot(&INVENTORY_PROTOCOL);
        bytes.add(inventory, Dir::In, 7);
        bytes.add(inventory, Dir::In, 5);
        bytes.add(inventory, Dir::Out, 9);
        bytes.add(WireBytes::GOSSIP, Dir::Out, 3);
        bytes.add(WireBytes::OTHER, Dir::In, 2);
        let counts = bytes.take();
        assert_eq!(counts.0[inventory], [12, 9]);
        assert_eq!(counts.0[WireBytes::GOSSIP], [0, 3]);
        assert_eq!(counts.0[WireBytes::OTHER], [2, 0]);
        assert_eq!(
            counts.to_string(),
            "node=0/0 profile=0/0 mailbox=0/0 push=0/0 media=0/0 search=0/0 inventory=12/9 digest=0/0 gossip=0/3 other=2/0"
        );
        assert!(bytes.take().0.iter().all(|slot| *slot == [0, 0]));
        assert_eq!(bytes.totals().0[inventory], [12, 9]);
    }

    #[test]
    fn concurrent_additions_and_takes_conserve_bytes() {
        let bytes = WireBytes::default();
        let slot = WireBytes::protocol_slot(&INVENTORY_PROTOCOL);
        let mut total = [0, 0];
        std::thread::scope(|scope| {
            let writers: Vec<_> = (0..4)
                .map(|_| {
                    scope.spawn(|| {
                        for _ in 0..1000 {
                            bytes.add(slot, Dir::In, 2);
                            bytes.add(slot, Dir::Out, 3);
                        }
                    })
                })
                .collect();
            for _ in 0..1000 {
                let counts = bytes.take().0[slot];
                total[0] += counts[0];
                total[1] += counts[1];
                std::thread::yield_now();
            }
            for writer in writers {
                writer.join().unwrap();
            }
        });
        let counts = bytes.take().0[slot];
        assert_eq!([total[0] + counts[0], total[1] + counts[1]], [8000, 12000]);
        assert!(bytes.take().0.iter().all(|slot| *slot == [0, 0]));
    }
}
