//! Fair queues for store and search work run off the swarm loop.

use libp2p::PeerId;
use std::collections::{HashMap, HashSet, VecDeque};

/// Store calls running at once; each store keeps one SQLite connection behind a mutex, so more
/// would only wait on it.
pub const STORE_WORKERS: usize = 4;
/// Requests a peer may have waiting; past it the request is dropped and the client retries.
pub const PEER_QUEUE: usize = 16;

/// Jobs per peer, run one at a time per peer and round robin between peers, so one peer
/// cannot hold every worker and its replies keep their order.
pub struct FairQueue<T> {
    queues: HashMap<PeerId, VecDeque<T>>,
    /// Peers with waiting jobs and none running, in turn order.
    ready: VecDeque<PeerId>,
    running: HashSet<PeerId>,
}

impl<T> Default for FairQueue<T> {
    fn default() -> Self {
        Self {
            queues: HashMap::new(),
            ready: VecDeque::new(),
            running: HashSet::new(),
        }
    }
}

impl<T> FairQueue<T> {
    /// Queues a job; gives it back when the peer already has [`PEER_QUEUE`] waiting.
    pub fn push(&mut self, peer: PeerId, job: T) -> Result<(), T> {
        let queue = self.queues.entry(peer).or_default();
        if queue.len() >= PEER_QUEUE {
            return Err(job);
        }
        queue.push_back(job);
        if queue.len() == 1 && !self.running.contains(&peer) {
            self.ready.push_back(peer);
        }
        Ok(())
    }

    /// The next job to run while fewer than [`STORE_WORKERS`] are running.
    pub fn next(&mut self) -> Option<(PeerId, T)> {
        if self.running.len() >= STORE_WORKERS {
            return None;
        }
        let peer = self.ready.pop_front()?;
        let queue = self.queues.get_mut(&peer)?;
        let job = queue.pop_front()?;
        if queue.is_empty() {
            self.queues.remove(&peer);
        }
        self.running.insert(peer);
        Some((peer, job))
    }

    /// Marks the peer's running job done; its next job waits for its turn.
    pub fn done(&mut self, peer: PeerId) {
        if self.running.remove(&peer) && self.queues.contains_key(&peer) {
            self.ready.push_back(peer);
        }
    }
}

/// Index reads waiting per IP group: one /24 or /48 cannot fill the queue.
pub const PER_GROUP: usize = 4;
/// Index reads waiting in all.
pub const QUEUED: usize = 256;

/// Index reads waiting for a search worker: search peers first, then one item per IP group in
/// turn, so a flooding group delays a quiet one by at most one item per worker.
pub struct GroupQueue<T> {
    peers: VecDeque<T>,
    groups: HashMap<String, VecDeque<T>>,
    /// Groups with waiting items, in turn order.
    turn: VecDeque<String>,
    len: usize,
}

impl<T> Default for GroupQueue<T> {
    fn default() -> Self {
        Self {
            peers: VecDeque::new(),
            groups: HashMap::new(),
            turn: VecDeque::new(),
            len: 0,
        }
    }
}

impl<T> GroupQueue<T> {
    /// Queues an item; `group = None` is a search peer, limited by [`QUEUED`] only. Gives the
    /// item back when its group has [`PER_GROUP`] waiting or the queue holds [`QUEUED`].
    pub fn push(&mut self, group: Option<&str>, item: T) -> Result<(), T> {
        if self.len >= QUEUED {
            return Err(item);
        }
        match group {
            None => self.peers.push_back(item),
            Some(group) => {
                let queue = self.groups.entry(group.to_owned()).or_default();
                if queue.len() >= PER_GROUP {
                    return Err(item);
                }
                if queue.is_empty() {
                    self.turn.push_back(group.to_owned());
                }
                queue.push_back(item);
            }
        }
        self.len += 1;
        Ok(())
    }

    pub fn pop(&mut self) -> Option<T> {
        let item = match self.peers.pop_front() {
            Some(item) => item,
            None => {
                let group = self.turn.pop_front()?;
                let queue = self.groups.get_mut(&group)?;
                let item = queue.pop_front()?;
                if queue.is_empty() {
                    self.groups.remove(&group);
                } else {
                    self.turn.push_back(group);
                }
                item
            }
        };
        self.len -= 1;
        Some(item)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quiet_group_is_served_among_the_first() {
        let mut queue = GroupQueue::default();
        for n in 0..PER_GROUP {
            queue.push(Some("a"), ("a", n)).unwrap();
        }
        for n in PER_GROUP..50 {
            assert!(queue.push(Some("a"), ("a", n)).is_err());
        }
        queue.push(Some("b"), ("b", 0)).unwrap();
        let first: Vec<_> = (0..2).map(|_| queue.pop().unwrap().0).collect();
        assert!(first.contains(&"b"));
        // Each group keeps its own order.
        assert_eq!(queue.pop(), Some(("a", 1)));
    }

    #[test]
    fn total_cap_and_search_peers_first() {
        let mut queue = GroupQueue::default();
        for group in 0..QUEUED / PER_GROUP {
            for n in 0..PER_GROUP {
                queue.push(Some(&group.to_string()), n).unwrap();
            }
        }
        assert!(queue.push(Some("new"), 0).is_err());
        assert!(queue.push(None, 99).is_err());
        queue.pop();
        queue.push(None, 99).unwrap();
        assert_eq!(queue.pop(), Some(99));
        let mut left = 0;
        while queue.pop().is_some() {
            left += 1;
        }
        assert_eq!(left, QUEUED - 1);
    }

    #[test]
    fn one_peer_cannot_take_every_worker_and_keeps_its_order() {
        let mut queue = FairQueue::default();
        let (flood, other) = (PeerId::random(), PeerId::random());
        for n in 0..PEER_QUEUE {
            queue.push(flood, n).unwrap();
        }
        assert_eq!(queue.push(flood, 99), Err(99));
        queue.push(other, 100).unwrap();
        assert_eq!(queue.next(), Some((flood, 0)));
        // The flooding peer waits for its running job; the other peer is served at once.
        assert_eq!(queue.next(), Some((other, 100)));
        assert_eq!(queue.next(), None);
        queue.done(flood);
        assert_eq!(queue.next(), Some((flood, 1)));
        assert_eq!(queue.queues[&flood].len(), PEER_QUEUE - 2);
    }

    #[test]
    fn at_most_store_workers_run() {
        let mut queue = FairQueue::default();
        let peers: Vec<PeerId> = (0..STORE_WORKERS + 1).map(|_| PeerId::random()).collect();
        for peer in &peers {
            queue.push(*peer, ()).unwrap();
        }
        let running: Vec<_> = std::iter::from_fn(|| queue.next()).collect();
        assert_eq!(running.len(), STORE_WORKERS);
        queue.done(running[0].0);
        assert_eq!(queue.next(), Some((peers[STORE_WORKERS], ())));
    }
}
