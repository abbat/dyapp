//! Local operation health of the nodes the client uses. The shared policy ranks candidates,
//! bounds their shares and opens recovery circuits; the legacy penalty retains the public scoring
//! contract used by existing callers. What a node says about itself (`info`, status) is never
//! scored: it is a hint only.

use crate::selection::{Operation, Outcome, Policy};
use crate::{Error, Node};
use dyapp_p2p_net::proto::Status;
use libp2p::{Multiaddr, PeerId};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Seconds after which a node's penalty halves.
pub const HALF_LIFE: u64 = 24 * 3_600;

/// Penalty and the time it was last raised, per node, plus the selection policy.
// shortcut: held in memory only; persist health only with a separate privacy and poisoning design.
#[derive(Debug, Clone)]
pub struct Scores {
    penalties: HashMap<PeerId, (u32, u64)>,
    policy: Policy,
}

impl Default for Scores {
    fn default() -> Self {
        Self {
            penalties: HashMap::new(),
            policy: Policy::new(getrandom::u64().unwrap_or(0xd17)),
        }
    }
}

impl Scores {
    /// Counts one fault of `node` at `now`, seconds since the epoch.
    pub(crate) fn fault(&mut self, node: PeerId, now: u64, operation: Operation) {
        let penalty = self.penalty(node, now).saturating_add(1);
        self.penalties.insert(node, (penalty, now));
        self.policy.observe(
            node,
            &Multiaddr::empty(),
            operation,
            Outcome::HardFault,
            Duration::ZERO,
            Instant::now(),
        );
    }

    /// Records one valid response and its observed cost.
    pub(crate) fn success(&mut self, node: PeerId, operation: Operation, cost: Duration) {
        self.policy.observe(
            node,
            &Multiaddr::empty(),
            operation,
            if cost > crate::TIMEOUT.checked_div(2).unwrap_or(crate::TIMEOUT) {
                Outcome::Slow
            } else {
                Outcome::Normal
            },
            cost,
            Instant::now(),
        );
    }

    pub(crate) fn invalid(&mut self, node: PeerId, operation: Operation) {
        self.policy.observe(
            node,
            &Multiaddr::empty(),
            operation,
            Outcome::Invalid,
            Duration::ZERO,
            Instant::now(),
        );
    }

    pub(crate) fn zero_yield(&mut self, node: PeerId, operation: Operation, cost: Duration) {
        self.policy.observe(
            node,
            &Multiaddr::empty(),
            operation,
            Outcome::ZeroYield,
            cost,
            Instant::now(),
        );
    }

    /// Records an honest absence without penalizing the node.
    pub(crate) fn not_found(&mut self, node: PeerId, operation: Operation, cost: Duration) {
        self.policy.observe(
            node,
            &Multiaddr::empty(),
            operation,
            Outcome::NotFound,
            cost,
            Instant::now(),
        );
    }

    /// The penalty of `node` at `now`: 0 for a node without faults.
    #[must_use]
    pub fn penalty(&self, node: PeerId, now: u64) -> u32 {
        self.penalties.get(&node).map_or(0, |&(penalty, at)| {
            let halvings = now.saturating_sub(at) / HALF_LIFE;
            u32::try_from(halvings)
                .ok()
                .and_then(|halvings| penalty.checked_shr(halvings))
                .unwrap_or(0)
        })
    }

    /// Move open circuits to half-open after an observed platform resume.
    pub fn resume(&mut self) {
        self.policy.resume(Instant::now());
    }

    #[cfg(test)]
    pub(crate) fn state(&mut self, peer: PeerId, now: Instant) -> crate::selection::State {
        self.policy.state(peer, now)
    }

    /// Registers real holder addresses and IP groups before selecting them.
    pub(crate) fn register_nodes(&mut self, nodes: &[Node]) {
        for node in nodes {
            let (peer, address, group) = node.describe();
            self.policy.register(peer, group, address);
        }
    }

    pub(crate) fn register(&mut self, peer: PeerId, address: Multiaddr, group: String) {
        self.policy.register(peer, group, address);
    }

    /// Indexes selected by the shared operation policy without replacement.
    #[must_use]
    pub(crate) fn ranked(&mut self, nodes: &[PeerId], operation: Operation) -> Vec<usize> {
        for peer in nodes {
            self.policy
                .register(*peer, String::new(), Multiaddr::empty());
        }
        self.policy
            .select_allowed(operation, nodes.len(), Instant::now(), Some(nodes))
            .iter()
            .filter_map(|selected| nodes.iter().position(|peer| peer == selected))
            .collect()
    }
}

/// Whether `error` from a node request counts against the node: anything but `NotFound`, which
/// an honest node also answers.
#[must_use]
pub fn faulty(error: &Error) -> bool {
    !matches!(error, Error::Status(Status::NotFound))
}

/// The first success of `request` over `nodes`, tried in [`Scores::ranked`] order; `peer` names
/// each node. Every failure is scored and the next node tried; the last error is returned when
/// all fail.
///
/// # Errors
///
/// Returns the last request error when every node fails, [`Error::Timeout`] when the total
/// failover budget expires, or the not-found status when `nodes` is empty. Fault penalties from
/// failed attempts remain applied.
pub(crate) async fn first<T, R>(
    scores: &mut Scores,
    nodes: &mut [T],
    describe: impl Fn(&T) -> (PeerId, Multiaddr, String),
    operation: Operation,
    mut request: impl AsyncFnMut(&mut T) -> Result<R, Error>,
) -> Result<R, Error> {
    let now = crate::now();
    let descriptions: Vec<_> = nodes.iter().map(&describe).collect();
    for (peer, address, group) in descriptions {
        scores.register(peer, address, group);
    }
    let peers: Vec<PeerId> = nodes.iter().map(|node| describe(node).0).collect();
    let mut last = Error::Status(Status::NotFound);
    tokio::time::timeout(crate::OPERATION_TIMEOUT, async {
        for i in scores.ranked(&peers, operation) {
            let (Some(node), Some(&peer)) = (nodes.get_mut(i), peers.get(i)) else {
                continue;
            };
            let started = Instant::now();
            match request(node).await {
                Ok(reply) => {
                    scores.success(peer, operation, started.elapsed());
                    return Ok(reply);
                }
                Err(error) => {
                    if faulty(&error) {
                        scores.fault(peer, now, operation);
                    } else {
                        scores.not_found(peer, operation, started.elapsed());
                    }
                    last = error;
                }
            }
        }
        Err(last)
    })
    .await
    .map_err(|_| Error::Timeout)?
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_800_000_000;

    // A node that refuses one user.
    struct Fake {
        peer: PeerId,
        refused: &'static str,
    }

    impl Fake {
        fn put(&mut self, user: &str) -> Result<PeerId, Error> {
            if user == self.refused {
                return Err(Error::Status(Status::Refused));
            }
            Ok(self.peer)
        }
    }

    #[tokio::test]
    async fn a_node_that_refuses_one_user_is_skipped_and_scored() {
        let hostile = PeerId::random();
        let honest = PeerId::random();
        let mut nodes = vec![Fake {
            peer: hostile,
            refused: "alice",
        }];
        let mut scores = Scores::default();
        for _ in 0..2 {
            let refused = first(
                &mut scores,
                &mut nodes,
                |node| (node.peer, Multiaddr::empty(), String::new()),
                Operation::Search,
                async |node| node.put("alice"),
            )
            .await;
            assert!(matches!(refused, Err(Error::Status(Status::Refused))));
        }
        assert_eq!(scores.penalty(hostile, crate::now()), 2);
        nodes.push(Fake {
            peer: honest,
            refused: "",
        });
        let served = first(
            &mut scores,
            &mut nodes,
            |node| (node.peer, Multiaddr::empty(), String::new()),
            Operation::Search,
            async |node| node.put("bob"),
        )
        .await
        .unwrap();
        assert_eq!(served, honest);
        assert_eq!(scores.penalty(honest, crate::now()), 0);
    }

    #[test]
    fn penalties_decay_while_selection_recovery_uses_the_policy_clock() {
        let (bad, good) = (PeerId::random(), PeerId::random());
        let mut scores = Scores::default();
        scores.fault(bad, NOW, Operation::Search);
        scores.fault(bad, NOW + 60, Operation::Search);
        assert_eq!(scores.penalty(bad, NOW + 60), 2);
        assert_eq!(scores.penalty(bad, NOW + 60 + HALF_LIFE), 1);
        assert_eq!(scores.penalty(bad, NOW + 60 + 2 * HALF_LIFE), 0);
        assert_eq!(scores.penalty(bad, u64::MAX), 0);
        assert_eq!(scores.ranked(&[bad, good], Operation::Search).len(), 2);
    }

    #[test]
    fn policy_state_is_isolated_per_session_scores() {
        let bad = PeerId::random();
        let mut first = Scores::default();
        let mut second = Scores::default();
        first.fault(bad, NOW, Operation::MailboxRead);
        first.fault(bad, NOW, Operation::MailboxRead);
        assert_eq!(second.penalty(bad, NOW), 0);
        assert_eq!(second.ranked(&[bad], Operation::MailboxRead), [0]);
    }

    #[tokio::test]
    async fn every_node_failing_returns_the_last_error_and_not_found_is_no_fault() {
        let mut nodes = [PeerId::random(), PeerId::random()];
        let mut scores = Scores::default();
        let result: Result<(), _> = first(
            &mut scores,
            &mut nodes,
            |peer| (*peer, Multiaddr::empty(), String::new()),
            Operation::Search,
            async |_| Err(Error::Status(Status::NotFound)),
        )
        .await;
        assert!(matches!(result, Err(Error::Status(Status::NotFound))));
        assert_eq!(scores.penalty(nodes[0], crate::now()), 0);
        let result: Result<(), _> = first(
            &mut scores,
            &mut nodes,
            |peer| (*peer, Multiaddr::empty(), String::new()),
            Operation::Search,
            async |_| Err(Error::Timeout),
        )
        .await;
        assert!(matches!(result, Err(Error::Timeout)));
        assert_eq!(scores.penalty(nodes[1], crate::now()), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn operation_budget_stops_a_pending_failover_chain() {
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let mut nodes = [PeerId::random(), PeerId::random()];
        let mut scores = Scores::default();
        let mut operation = Box::pin(first(
            &mut scores,
            &mut nodes,
            |peer| (*peer, Multiaddr::empty(), String::new()),
            Operation::Search,
            async |_| {
                calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                std::future::pending::<Result<(), Error>>().await
            },
        ));
        let result = tokio::select! {
            biased;
            result = &mut operation => result,
            () = tokio::time::advance(crate::OPERATION_TIMEOUT) => operation.await
        };
        assert!(matches!(result, Err(Error::Timeout)));
        assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn one_attempt_timeout_leaves_budget_for_a_healthy_fallback() {
        let (bad, good) = (PeerId::random(), PeerId::random());
        let mut nodes = [bad, good];
        let mut scores = Scores::default();
        let mut operation = Box::pin(first(
            &mut scores,
            &mut nodes,
            |peer| (*peer, Multiaddr::empty(), String::new()),
            Operation::Search,
            async |peer| {
                if *peer == bad {
                    let _ =
                        tokio::time::timeout(crate::TIMEOUT, std::future::pending::<()>()).await;
                    return Err(Error::Timeout);
                }
                Ok(*peer)
            },
        ));
        let result = tokio::select! {
            biased;
            result = &mut operation => result,
            () = tokio::time::advance(crate::OPERATION_TIMEOUT) => operation.await
        };
        assert_eq!(result.unwrap(), good);
    }
}
