//! Identify addresses a peer claims on another IP than its connection, waiting to be proved by a
//! dial with the peer's ID. Pure state with explicit times and connection IDs; the node loop
//! dials and routes.

use libp2p::swarm::ConnectionId;
use libp2p::{Multiaddr, PeerId};
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

/// Claimed addresses queued or dialled for one peer.
pub const PER_PEER: usize = 4;
/// Claimed addresses queued or dialled for all peers.
pub const TOTAL: usize = 64;
/// An address is dialled at most once in this time.
pub const COOLDOWN: Duration = Duration::from_secs(3600);
/// Dialled addresses remembered for the cooldown; the oldest gives way.
pub const HISTORY: usize = 4096;

/// Claims waiting for a dial, the dial in flight per peer, and the recently dialled addresses.
#[derive(Default)]
pub struct Claims {
    queued: HashMap<PeerId, VecDeque<Multiaddr>>,
    active: HashMap<ConnectionId, (PeerId, Multiaddr)>,
    /// Every queued or dialled address, for the global dedup.
    pending: HashSet<Multiaddr>,
    /// When each address was last dialled, and the addresses oldest first.
    dialled: HashMap<Multiaddr, Instant>,
    order: VecDeque<Multiaddr>,
}

impl Claims {
    /// Queues `address` claimed by `peer`. False when it is queued or dialled for any peer, was
    /// dialled within the cooldown, or the peer or total bound is full.
    pub fn claim(&mut self, now: Instant, peer: PeerId, address: Multiaddr) -> bool {
        let cooling = self
            .dialled
            .get(&address)
            .is_some_and(|at| now.saturating_duration_since(*at) < COOLDOWN);
        if cooling || self.pending.contains(&address) || self.pending.len() >= TOTAL {
            return false;
        }
        let active = self.active.values().filter(|(p, _)| *p == peer).count();
        let queue = self.queued.entry(peer).or_default();
        if queue.len() + active >= PER_PEER {
            return false;
        }
        queue.push_back(address.clone());
        self.pending.insert(address);
        true
    }

    /// Starts one dial for each peer with a queued claim and none in flight. `dial` returns the
    /// dial's connection ID, or `None` when the swarm refuses it at once, which frees the slot
    /// and moves on to that peer's next claim.
    pub fn start(
        &mut self,
        now: Instant,
        mut dial: impl FnMut(PeerId, &Multiaddr) -> Option<ConnectionId>,
    ) {
        let busy: HashSet<PeerId> = self.active.values().map(|(p, _)| *p).collect();
        let peers: Vec<PeerId> = self
            .queued
            .keys()
            .filter(|p| !busy.contains(p))
            .copied()
            .collect();
        for peer in peers {
            while let Some(address) = self.queued.get_mut(&peer).and_then(VecDeque::pop_front) {
                self.remember(now, address.clone());
                match dial(peer, &address) {
                    Some(id) => {
                        self.active.insert(id, (peer, address));
                        break;
                    }
                    None => {
                        self.pending.remove(&address);
                    }
                }
            }
            if self.queued.get(&peer).is_some_and(VecDeque::is_empty) {
                self.queued.remove(&peer);
            }
        }
    }

    /// A connection was established: the proved address when it is the dial of a claim by the
    /// same peer. A connection of another peer drops the claim; an unknown ID is ignored.
    pub fn confirm(&mut self, id: ConnectionId, peer: PeerId) -> Option<Multiaddr> {
        let (claimed_by, address) = self.active.remove(&id)?;
        self.pending.remove(&address);
        (claimed_by == peer).then_some(address)
    }

    /// The dial with this connection ID failed: its claim is dropped.
    pub fn fail(&mut self, id: ConnectionId) {
        if let Some((_, address)) = self.active.remove(&id) {
            self.pending.remove(&address);
        }
    }

    /// The peer's last connection closed: its queued claims are dropped. A dial in flight ends
    /// through `confirm` or `fail`; the cooldown stays.
    pub fn disconnected(&mut self, peer: &PeerId) {
        for address in self.queued.remove(peer).unwrap_or_default() {
            self.pending.remove(&address);
        }
    }

    /// Queued and dialled claims.
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    /// Whether no claim is queued or dialled.
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    fn remember(&mut self, now: Instant, address: Multiaddr) {
        if self.dialled.insert(address.clone(), now).is_some() {
            self.order.retain(|a| *a != address);
        }
        self.order.push_back(address);
        while self.order.len() > HISTORY {
            if let Some(oldest) = self.order.pop_front() {
                self.dialled.remove(&oldest);
            }
        }
    }
}

#[cfg(test)]
mod tests;
