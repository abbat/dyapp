//! Search catch-up from one peer: compare bucket digests, then sub-bucket digests, then fetch the
//! inventory of the differing key ranges only. Any digest failure falls back to the inventory of
//! the whole range the digest covered.

use crate::search::{slot_start, SearchIndex, SLOTS};
use dyapp_p2p_net::proto::{self, DigestResponse, Status};
use dyapp_p2p_net::Behaviour;
use libp2p::request_response::{Event, Message, OutboundRequestId};
use libp2p::{PeerId, Swarm};
use std::collections::{HashMap, VecDeque};

/// Keys per inventory page, as the serving node caps it.
pub const PAGE: usize = 10_000;
/// More differing buckets than this fetch the whole topic instead of their sub-digests.
const MAX_BUCKETS: usize = 64;
/// More runs of differing sub-buckets than this fetch the whole bucket in one range.
const MAX_RUNS: usize = 32;
/// Digest and inventory requests in flight at once.
const IN_FLIGHT: usize = 8;

/// One catch-up request to make.
#[derive(Debug)]
pub enum Ask {
    /// The topic's bucket hashes, or with `bucket` that bucket's sub-bucket hashes.
    Digest {
        /// Search topic.
        topic: u32,
        /// Bucket of the topic; `None` asks the topic digest.
        bucket: Option<u8>,
    },
    /// The inventory of a key range.
    Range(Range),
}

/// Keys of `topic` after `after` and, unless empty, before `before`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Range {
    /// Search topic.
    pub topic: u32,
    /// Exclusive lower bound; empty = from the start.
    pub after: Vec<u8>,
    /// Exclusive upper bound; empty = none.
    pub before: Vec<u8>,
}

/// The key just below `key` (big-endian); empty for the all-zero key.
fn decrement(key: &[u8; 32]) -> Vec<u8> {
    if *key == [0; 32] {
        return Vec::new();
    }
    let mut key = *key;
    for byte in key.iter_mut().rev() {
        let (value, borrow) = byte.overflowing_sub(1);
        *byte = value;
        if !borrow {
            break;
        }
    }
    key.to_vec()
}

/// Slots `from..to` of `topic` as an inventory range.
fn range(topic: u32, from: u32, to: u32) -> Range {
    Range {
        topic,
        after: decrement(&slot_start(topic, from)),
        before: if to == SLOTS {
            Vec::new()
        } else {
            slot_start(topic, to).to_vec()
        },
    }
}

/// What to ask next after a digest answer (`None`: the request failed). `bucket` is the bucket
/// the request named; `local` is this node's answer to the same request.
pub fn on_digest(
    local: &[[u8; 32]],
    topic: u32,
    bucket: Option<u8>,
    answer: Option<&DigestResponse>,
) -> Vec<Ask> {
    let first = bucket.map_or(0, |b| u32::from(b) << 8);
    let remote = answer.filter(|a| {
        a.status == i32::from(Status::Ok)
            && a.hashes.len() == 256
            && a.hashes.iter().all(|h| h.len() == 32)
    });
    let Some(remote) = remote else {
        let last = bucket.map_or(SLOTS, |_| first + 256);
        return vec![Ask::Range(range(topic, first, last))];
    };
    let differs = |i: usize| local.get(i).is_none_or(|l| remote.hashes[i] != l[..]);
    let Some(_) = bucket else {
        let buckets: Vec<u8> = (0..=255u8).filter(|b| differs(usize::from(*b))).collect();
        // Too many to compare one by one (e.g. a partial sample): fetch the whole topic.
        if buckets.len() > MAX_BUCKETS {
            return vec![Ask::Range(range(topic, 0, SLOTS))];
        }
        return buckets
            .into_iter()
            .map(|b| Ask::Digest {
                topic,
                bucket: Some(b),
            })
            .collect();
    };
    let mut asks = Vec::new();
    let mut i = 0;
    while i < 256 {
        if !differs(i) {
            i += 1;
            continue;
        }
        let start = i;
        while i < 256 && differs(i) {
            i += 1;
        }
        asks.push(Ask::Range(range(
            topic,
            first + start as u32,
            first + i as u32,
        )));
    }
    // Too many separate ranges: one request for the whole bucket is cheaper.
    if asks.len() > MAX_RUNS {
        return vec![Ask::Range(range(topic, first, first + 256))];
    }
    asks
}

/// Keeps the page's keys below `range.before` and returns the next page's range when the page
/// was full and nothing was past the bound (an old peer ignores `before`).
pub fn on_inventory(
    range: &Range,
    mut have: Vec<([u8; 32], u64)>,
) -> (Vec<([u8; 32], u64)>, Option<Range>) {
    let full = have.len() == PAGE;
    let before = have.len();
    if !range.before.is_empty() {
        have.retain(|(key, _)| key[..] < range.before[..]);
    }
    let next = (full && have.len() == before).then(|| Range {
        after: have.last().map(|h| h.0.to_vec()).unwrap_or_default(),
        ..range.clone()
    });
    (have, next)
}

/// Catch-up asks waiting and in flight, at most `IN_FLIGHT` out.
#[derive(Default)]
pub struct Asks {
    queue: VecDeque<(PeerId, Ask)>,
    digests: HashMap<OutboundRequestId, (u32, Option<u8>)>,
    ranges: HashMap<OutboundRequestId, Range>,
}

impl Asks {
    /// Nothing waits or is in flight.
    pub fn idle(&self) -> bool {
        self.queue.is_empty() && self.digests.is_empty() && self.ranges.is_empty()
    }

    /// Queues the topic digest of `topic` from `peer`.
    pub fn topic(&mut self, peer: PeerId, topic: u32) {
        let ask = Ask::Digest {
            topic,
            bucket: None,
        };
        self.queue.push_back((peer, ask));
    }

    /// Drops the waiting asks to a peer that is gone.
    pub fn forget(&mut self, peer: &PeerId) {
        self.queue.retain(|(p, _)| p != peer);
    }

    /// Handles an outbound digest event: an answer asks what differs, a failure the inventory of
    /// the range it covered. True when a catch-up digest came back OK.
    pub fn digest(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        index: &SearchIndex,
        event: Event<proto::DigestRequest, DigestResponse>,
    ) -> bool {
        let (peer, id, answer) = match event {
            Event::Message {
                peer,
                message:
                    Message::Response {
                        request_id,
                        response,
                    },
                ..
            } => (peer, request_id, Some(response)),
            Event::OutboundFailure {
                peer, request_id, ..
            } => (peer, request_id, None),
            _ => return false,
        };
        let Some((topic, bucket)) = self.digests.remove(&id) else {
            return false;
        };
        let local = match (&answer, bucket) {
            (None, _) => Ok(Vec::new()),
            (Some(_), None) => index.digest(topic),
            (Some(_), Some(bucket)) => index.sub_digest(topic, bucket).map(|(hashes, _)| hashes),
        };
        match local {
            // A failed request to a peer that is gone queues nothing for it.
            Ok(_) if answer.is_none() && !swarm.is_connected(&peer) => {}
            Ok(local) => {
                let asks = on_digest(&local, topic, bucket, answer.as_ref());
                self.queue.extend(asks.into_iter().map(|ask| (peer, ask)));
            }
            Err(error) => tracing::error!(%error, "search catch-up failed"),
        }
        self.send(swarm);
        answer.is_some_and(|a| a.status == i32::from(Status::Ok))
    }

    /// The range of an inventory request that ended, if it was a catch-up request.
    pub fn range(&mut self, id: &OutboundRequestId) -> Option<Range> {
        self.ranges.remove(id)
    }

    /// Asks `peer` for the next page of a range before anything else.
    pub fn next_page(&mut self, peer: PeerId, range: Range) {
        self.queue.push_front((peer, Ask::Range(range)));
    }

    /// Sends waiting asks while fewer than `IN_FLIGHT` are out.
    pub fn send(&mut self, swarm: &mut Swarm<Behaviour>) {
        while self.digests.len() + self.ranges.len() < IN_FLIGHT {
            let Some((peer, ask)) = self.queue.pop_front() else {
                return;
            };
            match ask {
                Ask::Digest { topic, bucket } => {
                    let request = proto::DigestRequest {
                        topic,
                        bucket: bucket.map(u32::from),
                    };
                    let id = swarm.behaviour_mut().digest.send_request(&peer, request);
                    self.digests.insert(id, (topic, bucket));
                }
                Ask::Range(range) => {
                    let request = proto::InventoryRequest {
                        topic: range.topic,
                        after: range.after.clone(),
                        before: range.before.clone(),
                    };
                    let id = swarm.behaviour_mut().inventory.send_request(&peer, request);
                    self.ranges.insert(id, range);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
