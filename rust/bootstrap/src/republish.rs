//! Republish scanner: walks the stored mailboxes, profiles and media keeps in key order and
//! queues each key whose closest nodes changed since its last full hand-over (ADR 0009).

use dyapp_p2p_net::{replica_key, REPLICAS};
use libp2p::PeerId;
use std::collections::{HashMap, VecDeque};
use std::hash::{DefaultHasher, Hash, Hasher};

/// Keys read from one store per run.
pub const BATCH: u32 = 10_000;
/// Keys queued per store at most.
pub const QUEUE: usize = 4096;
/// Replica keys per media blob hash: the bound media inventory checks, which covers shard keys.
const MEDIA_REPLICAS: u8 = 10;

/// Keys in key order, each with its replica keys.
pub type Batch = Vec<(Vec<u8>, Vec<Vec<u8>>)>;

/// A store the scanner walks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Store {
    /// Mailboxes with unexpired envelopes; the key is the mailbox.
    Mailbox,
    /// Live profiles; the key is the peer ID, its replica keys come from the key hash.
    Profile,
    /// Signed media keeps; the key is the owner, its replica keys come from the blob hashes.
    Media,
}

/// The replica keys of `keys`: a mailbox or profile key hash, or every blob hash of a keep.
pub fn replica_keys(store: Store, keys: &[Vec<u8>]) -> Vec<Vec<u8>> {
    let count = if store == Store::Media {
        MEDIA_REPLICAS
    } else {
        REPLICAS
    };
    keys.iter()
        .flat_map(|key| (0..count).map(move |i| replica_key(key, i)))
        .collect()
}

/// A hash of the closest peers of every replica key in order, each set sorted. In memory only:
/// `DefaultHasher` may change between builds.
pub fn fingerprint(mut closest: impl FnMut(&[u8]) -> Vec<PeerId>, keys: &[Vec<u8>]) -> u64 {
    let mut hasher = DefaultHasher::new();
    for key in keys {
        let mut peers = closest(key);
        peers.sort();
        peers.hash(&mut hasher);
    }
    hasher.finish()
}

fn key_hash(key: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

#[derive(Default)]
struct Lane {
    cursor: Vec<u8>,
    cycle: u32,
    /// Key hash → fingerprint of the last full hand-over, if any, and the last cycle the key
    /// was scanned in.
    seen: HashMap<u64, (Option<u64>, u32)>,
    queue: VecDeque<(Vec<u8>, u64)>,
}

/// Per store: a cursor, the fingerprint each key was last handed over at in full, and the keys
/// to hand over.
#[derive(Default)]
pub struct Scanner {
    lanes: [Lane; 3],
}

impl Scanner {
    /// The key the next batch is read after; empty at the start of a cycle.
    pub fn cursor(&self, store: Store) -> &[u8] {
        &self.lanes[store as usize].cursor
    }

    /// Takes a batch read after the cursor, `(queue key, its replica keys)` in key order;
    /// `wrapped` when it holds the last keys of the store. Queues each key whose fingerprint
    /// differs from its last full hand-over, or that has none. A full queue stops the batch with
    /// the cursor before the first key left out. A wrapped batch taken to its end starts a new
    /// cycle and forgets the keys the finished cycle did not scan.
    pub fn scan(
        &mut self,
        store: Store,
        batch: Batch,
        mut closest: impl FnMut(&[u8]) -> Vec<PeerId>,
        wrapped: bool,
    ) {
        let lane = &mut self.lanes[store as usize];
        for (key, replicas) in batch {
            let fingerprint = fingerprint(&mut closest, &replicas);
            let hash = key_hash(&key);
            if lane.seen.get(&hash).and_then(|seen| seen.0) != Some(fingerprint) {
                if let Some(queued) = lane.queue.iter_mut().find(|queued| queued.0 == key) {
                    queued.1 = fingerprint;
                } else if lane.queue.len() >= QUEUE {
                    return;
                } else {
                    lane.queue.push_back((key.clone(), fingerprint));
                }
            }
            lane.seen.entry(hash).or_insert((None, lane.cycle)).1 = lane.cycle;
            lane.cursor = key;
        }
        if wrapped {
            let cycle = lane.cycle;
            lane.seen.retain(|_, seen| seen.1 == cycle);
            lane.cycle = cycle.wrapping_add(1);
            lane.cursor.clear();
        }
    }

    /// The next key to hand over and the fingerprint it was queued at.
    pub fn pop(&mut self, store: Store) -> Option<(Vec<u8>, u64)> {
        self.lanes[store as usize].queue.pop_front()
    }

    /// Records a full hand-over of `key` at `fingerprint`; call it only when every replica was
    /// sent. A key not committed is queued again when the scan next reaches it.
    pub fn commit(&mut self, store: Store, key: &[u8], fingerprint: u64) {
        if let Some(seen) = self.lanes[store as usize].seen.get_mut(&key_hash(key)) {
            seen.0 = Some(fingerprint);
        }
    }
}

#[cfg(test)]
mod tests;
