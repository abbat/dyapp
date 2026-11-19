use libp2p::{Multiaddr, PeerId};
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

const MAX_NODES: usize = 128;
const COST_WINDOW: usize = 16;
const OUTCOME_WINDOW: usize = 10;
const ALLOCATION_TICKETS: usize = 10_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Operation {
    MailboxWrite,
    MailboxRead,
    MailboxCrossCheck,
    ProfilePublish,
    ProfileRead,
    Search,
    Media,
}

impl Operation {
    fn limits(self) -> (usize, usize) {
        if self == Self::Media {
            (2, 4)
        } else {
            (3, 5)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Outcome {
    Normal,
    Slow,
    HardFault,
    AddressFault,
    Invalid,
    NotFound,
    ZeroYield,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum State {
    Sufficient,
    Degraded,
    Open,
    HalfOpen,
}

#[derive(Clone, Debug, Default)]
struct AddressHealth {
    open_until: Option<Instant>,
}

#[derive(Clone, Debug)]
struct NodeHealth {
    group: String,
    addresses: HashMap<Multiaddr, AddressHealth>,
    costs: VecDeque<Duration>,
    hard: VecDeque<bool>,
    slow: VecDeque<bool>,
    consecutive_hard: u8,
    healthy_streak: u8,
    confidence: u8,
    open_until: Option<Instant>,
    open_count: u8,
    recovery_successes: u8,
    degraded: bool,
    zero_yield: u8,
    incumbent: bool,
    probation: bool,
}

impl NodeHealth {
    fn new(group: String, address: Multiaddr) -> Self {
        Self {
            group,
            addresses: HashMap::from([(address, AddressHealth::default())]),
            costs: VecDeque::new(),
            hard: VecDeque::new(),
            slow: VecDeque::new(),
            consecutive_hard: 0,
            healthy_streak: 0,
            confidence: 0,
            open_until: None,
            open_count: 0,
            recovery_successes: 0,
            degraded: false,
            zero_yield: 0,
            incumbent: false,
            probation: false,
        }
    }

    fn push<T>(window: &mut VecDeque<T>, value: T, limit: usize) {
        window.push_back(value);
        if window.len() > limit {
            window.pop_front();
        }
    }

    fn mean_cost(&self) -> f64 {
        if self.costs.is_empty() {
            return 1.0;
        }
        self.costs.iter().map(Duration::as_secs_f64).sum::<f64>() * ratio(1, self.costs.len())
    }

    fn open(&mut self, now: Instant) {
        self.open_count = self.open_count.saturating_add(1);
        let seconds = 60u64
            .checked_shl(self.open_count.saturating_sub(1).into())
            .unwrap_or(u64::MAX);
        self.open_until = Some(now + Duration::from_secs(seconds.min(1800)));
        self.recovery_successes = 0;
        self.consecutive_hard = 0;
        self.healthy_streak = 0;
        self.hard.clear();
        self.slow.clear();
        self.degraded = false;
    }

    fn valid(&mut self, cost: Duration, now: Instant) {
        Self::push(&mut self.costs, cost, COST_WINDOW);
        Self::push(&mut self.hard, false, OUTCOME_WINDOW);
        self.consecutive_hard = 0;
        self.confidence = self.confidence.saturating_add(1);
        if self.open_until.is_some_and(|until| now >= until) {
            self.recovery_successes += 1;
            if self.recovery_successes == 2 {
                self.open_until = None;
                self.hard.clear();
                self.slow.clear();
            }
        }
        if self.probation && self.confidence >= 3 {
            self.probation = false;
            self.incumbent = true;
        }
    }

    fn hard_fault(&mut self, now: Instant) {
        self.healthy_streak = 0;
        self.consecutive_hard = self.consecutive_hard.saturating_add(1);
        Self::push(&mut self.hard, true, OUTCOME_WINDOW);
        if self.open_until.is_some_and(|until| now >= until)
            || self.consecutive_hard >= 2
            || self.hard.iter().filter(|fault| **fault).count() >= 3
        {
            self.open(now);
        }
    }

    fn normal(&mut self, outcome: Outcome, cost: Duration, now: Instant) {
        self.valid(cost, now);
        Self::push(&mut self.slow, false, OUTCOME_WINDOW);
        self.healthy_streak = self.healthy_streak.saturating_add(1);
        self.degraded = false;
        if outcome == Outcome::ZeroYield {
            self.zero_yield = self.zero_yield.saturating_add(1);
        }
        if self.healthy_streak >= 64 {
            self.open_count = 0;
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Policy {
    nodes: HashMap<PeerId, NodeHealth>,
    ring: Vec<PeerId>,
    ring_at: usize,
    rng: u64,
    operations: u64,
    explorations: u64,
    explored: HashMap<PeerId, u32>,
}

impl Policy {
    pub(crate) fn new(seed: u64) -> Self {
        Self {
            nodes: HashMap::new(),
            ring: Vec::new(),
            ring_at: 0,
            rng: seed,
            operations: 0,
            explorations: 0,
            explored: HashMap::new(),
        }
    }

    pub(crate) fn register(&mut self, peer: PeerId, group: String, address: Multiaddr) {
        if let Some(existing) = self.nodes.get_mut(&peer) {
            existing.addresses.entry(address).or_default();
            return;
        }
        if self.nodes.len() == MAX_NODES {
            let evict = self
                .nodes
                .iter()
                .find(|(_, node)| node.open_until.is_some())
                .or_else(|| self.nodes.iter().find(|(_, node)| node.degraded))
                .or_else(|| self.nodes.iter().find(|(_, node)| node.probation))
                .map(|(peer, _)| *peer);
            let Some(evict) = evict else {
                return;
            };
            self.nodes.remove(&evict);
            self.ring.retain(|peer| *peer != evict);
            self.explored.remove(&evict);
            self.ring_at %= self.ring.len().max(1);
        }
        let group_open = self
            .nodes
            .values()
            .any(|node| node.group == group && node.open_until.is_some());
        let mut node = NodeHealth::new(group, address);
        node.probation = group_open;
        self.nodes.insert(peer, node);
        self.ring.push(peer);
    }

    pub(crate) fn admitted(&self, peer: PeerId) -> bool {
        self.nodes
            .get(&peer)
            .is_some_and(|node| node.incumbent && node.open_until.is_none())
    }

    pub(crate) fn state(&mut self, peer: PeerId, now: Instant) -> State {
        let Some(node) = self.nodes.get_mut(&peer) else {
            return State::Open;
        };
        if let Some(until) = node.open_until {
            return if now >= until {
                State::HalfOpen
            } else {
                State::Open
            };
        }
        if node.degraded {
            State::Degraded
        } else {
            State::Sufficient
        }
    }

    pub(crate) fn open_count(&self, peer: PeerId) -> u8 {
        self.nodes.get(&peer).map_or(0, |node| node.open_count)
    }

    pub(crate) fn address_open(&self, peer: PeerId, address: &Multiaddr) -> bool {
        self.nodes
            .get(&peer)
            .and_then(|node| node.addresses.get(address))
            .and_then(|address| address.open_until)
            .is_some_and(|until| until > Instant::now())
    }

    pub(crate) fn raw_weight(&self, peer: PeerId) -> u8 {
        self.nodes.get(&peer).map_or(0, |node| {
            if node.confidence < 3 || node.degraded || node.zero_yield >= 3 {
                1
            } else {
                4
            }
        })
    }

    pub(crate) fn observe(
        &mut self,
        peer: PeerId,
        address: &Multiaddr,
        _operation: Operation,
        outcome: Outcome,
        cost: Duration,
        now: Instant,
    ) {
        let Some(node) = self.nodes.get_mut(&peer) else {
            return;
        };
        node.addresses.entry(address.clone()).or_default();
        if outcome == Outcome::AddressFault {
            node.addresses
                .entry(address.clone())
                .or_default()
                .open_until = Some(now + Duration::from_secs(60));
            return;
        }
        match outcome {
            Outcome::HardFault => node.hard_fault(now),
            Outcome::Invalid => node.open(now),
            Outcome::Slow => {
                node.valid(cost, now);
                NodeHealth::push(&mut node.slow, true, OUTCOME_WINDOW);
                node.healthy_streak = 0;
                node.degraded = true;
                let slow_faults = node.slow.iter().filter(|slow| **slow).count();
                let healthy_alternates = self.nodes.values().filter(|other| {
                    other.incumbent
                        && other.open_until.is_none()
                        && other.slow.iter().filter(|slow| **slow).count() * 10
                            < other.slow.len() * 3
                });
                if slow_faults >= 3 && healthy_alternates.count() >= 2 {
                    self.nodes.get_mut(&peer).expect("known peer").open(now);
                }
            }
            Outcome::Normal | Outcome::NotFound | Outcome::ZeroYield => {
                node.normal(outcome, cost, now);
            }
            Outcome::AddressFault => unreachable!(),
        }
    }

    fn pool(&mut self, operation: Operation) -> Vec<PeerId> {
        let (target, maximum) = operation.limits();
        let mut pool = self
            .nodes
            .iter_mut()
            .filter(|(_, node)| node.incumbent && node.open_until.is_none())
            .map(|(peer, _)| *peer)
            .collect::<Vec<_>>();
        let wanted = if pool.is_empty() { maximum } else { target };
        if pool.len() < wanted {
            for peer in self.ring.clone() {
                if pool.len() == wanted {
                    break;
                }
                if pool.contains(&peer) {
                    continue;
                }
                let Some(node) = self.nodes.get_mut(&peer) else {
                    continue;
                };
                if (!node.probation || pool.is_empty()) && node.open_until.is_none() {
                    node.incumbent = true;
                    pool.push(peer);
                }
            }
        }
        pool.sort_unstable_by_key(|peer| peer.to_bytes());
        pool.truncate(maximum);
        pool
    }

    fn weights(&mut self, pool: &[PeerId]) -> Vec<f64> {
        let mut costs = pool
            .iter()
            .map(|peer| self.nodes.get(peer).expect("pool peer").mean_cost())
            .collect::<Vec<_>>();
        costs.sort_unstable_by(f64::total_cmp);
        let median = if costs.is_empty() {
            1.0
        } else {
            costs[costs.len() / 2]
        };
        pool.iter()
            .map(|peer| {
                let node = self.nodes.get(peer).expect("pool peer");
                if node.confidence < 3 || node.degraded || node.zero_yield >= 3 {
                    1.0
                } else {
                    (4.0 * median / node.mean_cost()).clamp(1.0, 4.0)
                }
            })
            .collect()
    }

    pub(crate) fn shares(&mut self, operation: Operation, _now: Instant) -> Vec<(PeerId, f64)> {
        let pool = self.pool(operation);
        let weights = self.weights(&pool);
        self.allocate(pool, &weights)
    }

    fn available(
        pool: &[PeerId],
        groups: &[String],
        assigned: &[usize],
        weights: &[f64],
        cap: usize,
        group_cap: usize,
    ) -> (Vec<usize>, Vec<f64>) {
        let mut candidates = Vec::new();
        let mut candidate_weights = Vec::new();
        for (index, weight) in weights.iter().enumerate() {
            let group_used = pool
                .iter()
                .zip(groups)
                .zip(assigned)
                .filter(|((_, group), _)| **group == groups[index])
                .map(|((_, _), count)| count)
                .sum::<usize>();
            if assigned[index] < ALLOCATION_TICKETS * cap / 100 && group_used < group_cap {
                candidates.push(index);
                candidate_weights.push(*weight);
            }
        }
        (candidates, candidate_weights)
    }

    fn allocate(&mut self, pool: Vec<PeerId>, weights: &[f64]) -> Vec<(PeerId, f64)> {
        let Some((cap, floor)) = bounds(pool.len()) else {
            return Vec::new();
        };
        let mut assigned = vec![ALLOCATION_TICKETS * floor / 100; pool.len()];
        let mut remaining = ALLOCATION_TICKETS - assigned.iter().sum::<usize>();
        let groups = pool
            .iter()
            .map(|peer| self.nodes.get(peer).expect("pool peer").group.clone())
            .collect::<Vec<_>>();
        let distinct = groups
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len();
        let mut group_cap = if distinct == 1 || pool.len() < 3 {
            ALLOCATION_TICKETS
        } else {
            ALLOCATION_TICKETS / 2
        };
        let mut rng = 0xd17u64;
        while remaining > 0 {
            let (candidates, candidate_weights) =
                Self::available(&pool, &groups, &assigned, weights, cap, group_cap);
            if candidates.is_empty() {
                if group_cap < ALLOCATION_TICKETS {
                    group_cap = (group_cap + ALLOCATION_TICKETS / 10).min(ALLOCATION_TICKETS);
                    continue;
                }
                break;
            }
            let total = candidate_weights.iter().sum::<f64>();
            let mut draw = seeded_random(&mut rng) * total;
            let mut selected = candidates.len() - 1;
            for (position, weight) in candidate_weights.iter().enumerate() {
                draw -= weight;
                if draw <= 0.0 {
                    selected = position;
                    break;
                }
            }
            assigned[candidates[selected]] += 1;
            remaining -= 1;
        }
        pool.into_iter()
            .zip(assigned)
            .map(|(peer, count)| (peer, ratio(count, ALLOCATION_TICKETS)))
            .collect()
    }

    pub(crate) fn select(
        &mut self,
        operation: Operation,
        count: usize,
        now: Instant,
    ) -> Vec<PeerId> {
        let exploration_period = if self.pool(operation).len() < operation.limits().0 {
            10
        } else {
            20
        };
        let mut selected = Vec::new();
        let mut shares = self.shares(operation, now);
        for _ in 0..count {
            if shares.is_empty() {
                break;
            }
            let total = shares.iter().map(|(_, share)| share).sum::<f64>();
            let mut draw = self.random() * total;
            let mut index = shares.len() - 1;
            for (position, (_, share)) in shares.iter().enumerate() {
                draw -= share;
                if draw <= 0.0 {
                    index = position;
                    break;
                }
            }
            selected.push(shares.remove(index).0);
        }
        self.operations = self.operations.saturating_add(1);
        if self.operations.is_multiple_of(exploration_period) {
            self.add_exploration(&mut selected, count);
        }
        selected
    }

    fn add_exploration(&mut self, selected: &mut Vec<PeerId>, count: usize) {
        let Some(peer) = self.explore() else {
            return;
        };
        if selected.contains(&peer) {
            return;
        }
        if selected.len() == count && !selected.is_empty() {
            selected.pop();
        }
        selected.push(peer);
    }

    fn explore(&mut self) -> Option<PeerId> {
        let len = self.ring.len();
        for offset in 0..len {
            let index = (self.ring_at + offset) % len;
            let peer = self.ring[index];
            let Some(node) = self.nodes.get(&peer) else {
                continue;
            };
            if node.incumbent || node.open_until.is_some() {
                continue;
            }
            self.ring_at = (index + 1) % len;
            if self.ring_at == 0 {
                self.shuffle_ring();
            }
            self.explorations = self.explorations.saturating_add(1);
            *self.explored.entry(peer).or_default() += 1;
            return Some(peer);
        }
        None
    }

    pub(crate) fn exploration_count(&self) -> u64 {
        self.explorations
    }

    pub(crate) fn explored(&self, peer: PeerId) -> u32 {
        self.explored.get(&peer).copied().unwrap_or(0)
    }

    pub(crate) fn challenge(
        &mut self,
        challenger: PeerId,
        operation: Operation,
        _now: Instant,
    ) -> bool {
        let candidate_cost = {
            let Some(candidate) = self.nodes.get(&challenger) else {
                return false;
            };
            if candidate.incumbent || candidate.confidence < 3 {
                return false;
            }
            candidate.mean_cost()
        };
        let Some((incumbent, _)) = self
            .pool(operation)
            .into_iter()
            .filter(|peer| *peer != challenger)
            .map(|peer| (peer, self.nodes.get(&peer).expect("pool peer").mean_cost()))
            .max_by(|left, right| left.1.total_cmp(&right.1))
        else {
            return false;
        };
        if 2.0 * candidate_cost > self.nodes.get(&incumbent).expect("incumbent").mean_cost() {
            return false;
        }
        self.nodes.get_mut(&incumbent).expect("incumbent").incumbent = false;
        self.nodes
            .get_mut(&challenger)
            .expect("challenger")
            .incumbent = true;
        true
    }

    pub(crate) fn resume(&mut self, now: Instant) {
        for node in self.nodes.values_mut() {
            if node.open_until.is_some() {
                node.open_until = Some(now);
            }
        }
    }

    fn random(&mut self) -> f64 {
        seeded_random(&mut self.rng)
    }

    fn shuffle_ring(&mut self) {
        for index in (1..self.ring.len()).rev() {
            let other = usize::try_from(next_u32(&mut self.rng)).unwrap_or(0) % (index + 1);
            self.ring.swap(index, other);
        }
    }
}

fn next_u32(seed: &mut u64) -> u32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    u32::try_from(*seed >> 32).unwrap_or(0)
}

fn seeded_random(seed: &mut u64) -> f64 {
    f64::from(next_u32(seed)) / f64::from(u32::MAX)
}

fn ratio(value: usize, total: usize) -> f64 {
    let value = u32::try_from(value).unwrap_or(u32::MAX);
    let total = u32::try_from(total).unwrap_or(u32::MAX);
    f64::from(value) / f64::from(total)
}

fn bounds(size: usize) -> Option<(usize, usize)> {
    if size == 0 || size > 5 {
        return None;
    }
    Some(match size {
        1 => (1, 1),
        2 => (65, 35),
        _ => (40, 10),
    })
}

#[cfg(test)]
mod tests;
