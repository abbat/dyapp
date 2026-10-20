//! A hash puzzle a loaded search node asks of clients' queries and gets, see
//! docs/architecture/bootstrap.md#search.

use dyapp_p2p_net::proto;
use std::collections::HashSet;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Difficulty the puzzle starts at once the node is loaded, in leading zero bits.
const FIRST_BITS: u32 = 12;
/// How often the difficulty follows the median queue wait.
pub const TICK: Duration = Duration::from_secs(10);
/// How long a challenge is current; it is accepted for one more rotation after it.
const ROTATE: Duration = Duration::from_secs(60);

/// The node's puzzle: difficulty from the queue waits, the challenge and the used solutions.
pub struct Puzzle {
    inner: Mutex<State>,
}

struct State {
    wait_ms: u64,
    max_bits: u32,
    bits: u32,
    /// Queue waits in milliseconds since the last tick.
    waits: Vec<u64>,
    challenge: [u8; 16],
    previous: [u8; 16],
    /// Admitted solutions as (peer, nonce): peers solving from 0 often find the same nonce.
    used: HashSet<(String, u64)>,
    used_previous: HashSet<(String, u64)>,
    rotated: Instant,
}

fn fresh() -> [u8; 16] {
    crate::random()
}

fn solved(challenge: &[u8], peer: &str, nonce: u64, bits: u32) -> bool {
    let mut data = b"dyapp-search-puzzle".to_vec();
    data.extend_from_slice(challenge);
    data.extend_from_slice(peer.as_bytes());
    data.extend_from_slice(&nonce.to_be_bytes());
    let hash = dyapp_identity::sha256(&data);
    let zeros = u128::from_be_bytes(hash[..16].try_into().unwrap_or_default()).leading_zeros();
    zeros >= bits
}

/// A nonce solving `challenge` for `peer` at `bits`.
pub fn solve(challenge: &[u8; 16], peer: &str, bits: u32) -> u64 {
    solve_from(challenge, peer, bits, 0)
}

/// The first nonce from `start` solving `challenge` for `peer` at `bits`.
pub fn solve_from(challenge: &[u8; 16], peer: &str, bits: u32, start: u64) -> u64 {
    let mut nonce = start;
    while !solved(challenge, peer, nonce, bits) {
        nonce += 1;
    }
    nonce
}

impl Puzzle {
    /// Off until the median queue wait passes `wait_ms`; at most `max_bits`.
    pub fn new(wait_ms: u64, max_bits: u32, now: Instant) -> Self {
        Self {
            inner: Mutex::new(State {
                wait_ms,
                max_bits,
                bits: 0,
                waits: Vec::new(),
                challenge: fresh(),
                previous: fresh(),
                used: HashSet::new(),
                used_previous: HashSet::new(),
                rotated: now,
            }),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    /// Current (challenge, bits); bits 0 = off.
    pub fn current(&self) -> ([u8; 16], u32) {
        let state = self.state();
        (state.challenge, state.bits)
    }

    /// Valid and unused: a current or previous challenge, enough bits, nonce not consumed.
    pub fn check(&self, peer: &str, puzzle: &proto::Puzzle) -> bool {
        let state = self.state();
        if state.bits == 0 {
            return true;
        }
        let used = (peer.to_owned(), puzzle.nonce);
        let fresh = if puzzle.challenge == state.challenge {
            !state.used.contains(&used)
        } else {
            puzzle.challenge == state.previous && !state.used_previous.contains(&used)
        };
        fresh && solved(&puzzle.challenge, peer, puzzle.nonce, state.bits)
    }

    /// Marks `peer`'s solution used; call only on admission to the search queue, so a solution
    /// whose request was refused there still counts on retry.
    pub fn consume(&self, peer: &str, puzzle: &proto::Puzzle) {
        let mut state = self.state();
        let used = (peer.to_owned(), puzzle.nonce);
        // Both sets hold at most the queue admissions of two minutes.
        if puzzle.challenge == state.challenge {
            state.used.insert(used);
        } else if puzzle.challenge == state.previous {
            state.used_previous.insert(used);
        }
    }

    /// One queue wait, from the node.
    pub fn record_wait(&self, waited: Duration) {
        let ms = u64::try_from(waited.as_millis()).unwrap_or(u64::MAX);
        self.state().waits.push(ms);
    }

    /// Every [`TICK`]: the median wait sets the difficulty; every minute a new challenge.
    pub fn tick(&self, now: Instant) {
        let mut state = self.state();
        let mut waits = std::mem::take(&mut state.waits);
        waits.sort_unstable();
        let median = waits.get(waits.len() / 2).copied().unwrap_or(0);
        if median > state.wait_ms {
            state.bits = match state.bits {
                0 => FIRST_BITS,
                bits => (bits + 1).min(state.max_bits),
            };
        } else if median < state.wait_ms / 2 {
            state.bits = match state.bits {
                bits if bits <= FIRST_BITS => 0,
                bits => bits - 1,
            };
        }
        if now.duration_since(state.rotated) >= ROTATE {
            state.previous = std::mem::replace(&mut state.challenge, fresh());
            state.used_previous = std::mem::take(&mut state.used);
            state.rotated = now;
        }
    }

    /// Sets the difficulty, for tests.
    #[cfg(test)]
    pub fn set_bits(&self, bits: u32) {
        self.state().bits = bits;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loaded(p: &Puzzle, t: Instant, secs: u64, ms: u64) -> Instant {
        let mut now = t;
        for _ in 0..secs / 10 {
            for _ in 0..20 {
                p.record_wait(Duration::from_millis(ms));
            }
            now += TICK;
            p.tick(now);
        }
        now
    }

    #[test]
    fn difficulty_ramps_and_decays() {
        let t = Instant::now();
        let p = Puzzle::new(100, 20, t);
        assert_eq!(p.current().1, 0);
        let t = loaded(&p, t, 10, 500);
        assert_eq!(p.current().1, 12);
        let t = loaded(&p, t, 200, 500);
        assert_eq!(p.current().1, 20);
        // Between half the threshold and the threshold: hold.
        let t = loaded(&p, t, 10, 70);
        assert_eq!(p.current().1, 20);
        loaded(&p, t, 100, 10);
        assert_eq!(p.current().1, 0);
    }

    #[test]
    fn solutions_are_bound_to_peer_challenge_and_use() {
        let t = Instant::now();
        let p = Puzzle::new(100, 20, t);
        let t = loaded(&p, t, 10, 500);
        let (challenge, bits) = p.current();
        let solution = |peer, start| proto::Puzzle {
            challenge: challenge.to_vec(),
            nonce: solve_from(&challenge, peer, bits, start),
        };
        let mine = solution("me", 0);
        assert!(p.check("me", &mine));
        assert!(!p.check("other", &mine));
        // Not consumed until admitted.
        assert!(p.check("me", &mine));
        p.consume("me", &mine);
        assert!(!p.check("me", &mine));
        // Another peer that found the same nonce is not affected.
        assert!(p.check("you", &solution("you", 0)));
        // The previous challenge is accepted for one rotation, then refused.
        let fresh = solution("me", mine.nonce + 1);
        // A tick without waits would also lower the difficulty; keep it.
        let t = t + ROTATE;
        p.tick(t);
        p.set_bits(bits);
        assert!(p.check("me", &fresh));
        p.tick(t + ROTATE);
        p.set_bits(bits);
        assert!(!p.check("me", &fresh));
    }
}
