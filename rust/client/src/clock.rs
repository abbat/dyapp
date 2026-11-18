//! Median node-clock correction for signed requests, never for monotonic scheduling.

use dyapp_p2p_net::proto::Status;
use libp2p::PeerId;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const MIN_HINTS: usize = 3;
const MAX_HINTS: usize = 64;

#[derive(Default)]
struct Hints {
    offsets: HashMap<PeerId, i128>,
}

impl Hints {
    fn observe(&mut self, peer: PeerId, server_time: u64, local_time: u64) {
        if server_time == 0
            || (self.offsets.len() >= MAX_HINTS && !self.offsets.contains_key(&peer))
        {
            return;
        }
        self.offsets.insert(
            peer,
            i128::from(server_time).saturating_sub(i128::from(local_time)),
        );
    }

    fn median(&self) -> Option<i128> {
        if self.offsets.len() < MIN_HINTS {
            return None;
        }
        let mut offsets = self.offsets.values().copied().collect::<Vec<_>>();
        offsets.sort_unstable();
        offsets.get(offsets.len().saturating_sub(1) / 2).copied()
    }

    fn corrected(&self, local_time: u64) -> u64 {
        self.median().map_or(local_time, |offset| {
            let corrected = i128::from(local_time)
                .saturating_add(offset)
                .clamp(0, i128::from(u64::MAX));
            u64::try_from(corrected).unwrap_or(local_time)
        })
    }
}

static HINTS: OnceLock<Mutex<Hints>> = OnceLock::new();

fn hints() -> &'static Mutex<Hints> {
    HINTS.get_or_init(Mutex::default)
}

fn with_hints<T>(read: impl FnOnce(&Hints) -> T) -> T {
    let guard = hints()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    read(&guard)
}

/// Unix seconds from the local system clock.
#[must_use]
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

pub(crate) fn now_ms() -> u64 {
    now().saturating_mul(1000)
}

pub(crate) fn clock_range(_: std::num::TryFromIntError) -> crate::store::Error {
    crate::store::Error::ClockRange
}

/// Whether enough distinct nodes supplied hints to install a median offset.
#[must_use]
pub fn has_median() -> bool {
    with_hints(|hints| hints.median().is_some())
}

/// `local_time` corrected by the median node offset, or unchanged with fewer than three hints.
#[must_use]
pub fn corrected(local_time: u64) -> u64 {
    with_hints(|hints| hints.corrected(local_time))
}

pub(crate) fn observe(peer: PeerId, server_time: u64, local_time: u64) {
    let mut guard = hints()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    guard.observe(peer, server_time, local_time);
}

pub(crate) trait ClockResponse {
    fn status(&self) -> i32;
    fn server_time(&self) -> u64;
}

impl ClockResponse for dyapp_p2p_net::proto::ProfileResponse {
    fn status(&self) -> i32 {
        self.status
    }

    fn server_time(&self) -> u64 {
        self.server_time
    }
}

impl ClockResponse for dyapp_p2p_net::proto::MediaResponse {
    fn status(&self) -> i32 {
        self.status
    }

    fn server_time(&self) -> u64 {
        self.server_time
    }
}

impl ClockResponse for dyapp_p2p_net::proto::SearchResponse {
    fn status(&self) -> i32 {
        self.status
    }

    fn server_time(&self) -> u64 {
        self.server_time
    }
}

pub(crate) fn observe_invalid(peer: PeerId, signed_time: bool, response: &impl ClockResponse) {
    if signed_time && response.status() == i32::from(Status::Invalid) {
        observe(peer, response.server_time(), now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::identity::Keypair;

    fn peer() -> PeerId {
        Keypair::generate_ed25519().public().to_peer_id()
    }

    #[test]
    fn fewer_than_three_distinct_nodes_cannot_install_an_offset() {
        let mut hints = Hints::default();
        let first = peer();
        hints.observe(first, 3_700, 100);
        hints.observe(first, 3_800, 200);
        hints.observe(peer(), 3_700, 100);
        assert_eq!(hints.median(), None);
        assert_eq!(hints.corrected(100), 100);
        hints.observe(peer(), 3_700, 100);
        assert_eq!(hints.median(), Some(3_600));
        assert_eq!(hints.corrected(100), 3_700);
    }

    #[test]
    fn one_lying_node_does_not_move_the_median() {
        let mut hints = Hints::default();
        for server_time in [3_700, 3_701, 3_699] {
            hints.observe(peer(), server_time, 100);
        }
        let before = hints.corrected(100);
        hints.observe(peer(), 3_600_100, 100);
        assert_eq!(hints.corrected(100), before);
    }

    #[test]
    fn legacy_zero_hints_and_bound_overflow_are_safe() {
        let mut hints = Hints::default();
        hints.observe(peer(), 0, 100);
        assert_eq!(hints.median(), None);
        for server_time in [1, 2, u64::MAX] {
            hints.observe(peer(), server_time, 100);
        }
        assert_eq!(hints.corrected(100), 2);
        assert_eq!(hints.corrected(0), 0);
        assert_eq!(Hints::default().corrected(u64::MAX), u64::MAX);
    }

    #[test]
    fn state_is_bounded_and_existing_peers_can_refresh() {
        let mut hints = Hints::default();
        let first = peer();
        hints.observe(first, 1_000, 100);
        for _ in 1..MAX_HINTS {
            hints.observe(peer(), 1_000, 100);
        }
        hints.observe(peer(), 9_000, 100);
        assert_eq!(hints.offsets.len(), MAX_HINTS);
        hints.observe(first, 2_000, 100);
        assert_eq!(hints.offsets[&first], 1_900);
    }
}
