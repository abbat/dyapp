//! Local scores of the nodes the client uses. A node can refuse one user, time out, serve invalid
//! data or an older profile than another replica; each such fault adds one to its penalty, which
//! halves every [`HALF_LIFE`] seconds. [`first`] tries the caller's nodes, e.g. one per replica
//! key, lowest penalty first. What a node says about itself (`info`, status) is never scored: it
//! is a hint only.

use crate::Error;
use dyapp_p2p_net::proto::Status;
use libp2p::PeerId;
use std::collections::HashMap;

/// Seconds after which a node's penalty halves.
pub const HALF_LIFE: u64 = 24 * 3_600;

/// Penalty and the time it was last raised, per node.
// shortcut: held in memory only, a restart forgives every node; persist it when the app
// stores client state.
#[derive(Debug, Default, Clone)]
pub struct Scores(HashMap<PeerId, (u32, u64)>);

impl Scores {
    /// Counts one fault of `node` at `now`, seconds since the epoch.
    pub fn fault(&mut self, node: PeerId, now: u64) {
        let penalty = self.penalty(node, now).saturating_add(1);
        self.0.insert(node, (penalty, now));
    }

    /// The penalty of `node` at `now`: 0 for a node without faults.
    pub fn penalty(&self, node: PeerId, now: u64) -> u32 {
        self.0.get(&node).map_or(0, |&(penalty, at)| {
            let halvings = now.saturating_sub(at) / HALF_LIFE;
            u32::try_from(halvings)
                .ok()
                .and_then(|halvings| penalty.checked_shr(halvings))
                .unwrap_or(0)
        })
    }

    /// Indexes of `nodes` by penalty at `now`; equal penalties keep the given order.
    pub fn ranked(&self, nodes: &[PeerId], now: u64) -> Vec<usize> {
        let mut order: Vec<usize> = (0..nodes.len()).collect();
        order.sort_by_key(|&i| self.penalty(nodes[i], now));
        order
    }
}

/// Whether `error` from a node request counts against the node: anything but `NotFound`, which
/// an honest node also answers.
pub fn faulty(error: &Error) -> bool {
    !matches!(error, Error::Status(Status::NotFound))
}

/// The first success of `request` over `nodes`, tried in [`Scores::ranked`] order; `peer` names
/// each node. Every failure is scored and the next node tried; the last error is returned when
/// all fail.
pub async fn first<T, R>(
    scores: &mut Scores,
    nodes: &mut [T],
    peer: impl Fn(&T) -> PeerId,
    mut request: impl AsyncFnMut(&mut T) -> Result<R, Error>,
) -> Result<R, Error> {
    let now = crate::now();
    let peers: Vec<PeerId> = nodes.iter().map(&peer).collect();
    let mut last = Error::Status(Status::NotFound);
    for i in scores.ranked(&peers, now) {
        match request(&mut nodes[i]).await {
            Ok(reply) => return Ok(reply),
            Err(error) => {
                if faulty(&error) {
                    scores.fault(peers[i], now);
                }
                last = error;
            }
        }
    }
    Err(last)
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
        async fn put(&mut self, user: &str) -> Result<PeerId, Error> {
            if user == self.refused {
                return Err(Error::Status(Status::Refused));
            }
            Ok(self.peer)
        }
    }

    #[tokio::test]
    async fn a_node_that_refuses_one_user_is_skipped_and_scored() {
        let mut nodes = [
            Fake {
                peer: PeerId::random(),
                refused: "alice",
            },
            Fake {
                peer: PeerId::random(),
                refused: "",
            },
        ];
        let (hostile, honest) = (nodes[0].peer, nodes[1].peer);
        let mut scores = Scores::default();
        let served = first(
            &mut scores,
            &mut nodes,
            |node| node.peer,
            async |node| node.put("alice").await,
        )
        .await
        .unwrap();
        assert_eq!(served, honest);
        assert_eq!(scores.penalty(hostile, crate::now()), 1);
        assert_eq!(scores.penalty(honest, crate::now()), 0);
        // The hostile node is now tried last, even for a user it serves.
        let served = first(
            &mut scores,
            &mut nodes,
            |node| node.peer,
            async |node| node.put("bob").await,
        )
        .await
        .unwrap();
        assert_eq!(served, honest);
    }

    #[tokio::test]
    async fn every_node_failing_returns_the_last_error_and_not_found_is_no_fault() {
        let mut nodes = [PeerId::random(), PeerId::random()];
        let mut scores = Scores::default();
        let result: Result<(), _> = first(
            &mut scores,
            &mut nodes,
            |peer| *peer,
            async |_| Err(Error::Status(Status::NotFound)),
        )
        .await;
        assert!(matches!(result, Err(Error::Status(Status::NotFound))));
        assert_eq!(scores.penalty(nodes[0], crate::now()), 0);
        let result: Result<(), _> = first(
            &mut scores,
            &mut nodes,
            |peer| *peer,
            async |_| Err(Error::Timeout),
        )
        .await;
        assert!(matches!(result, Err(Error::Timeout)));
        assert_eq!(scores.penalty(nodes[1], crate::now()), 1);
    }

    #[test]
    fn a_bad_node_is_tried_last_and_recovers_over_time() {
        let (bad, good) = (PeerId::random(), PeerId::random());
        let mut scores = Scores::default();
        scores.fault(bad, NOW);
        scores.fault(bad, NOW + 60);
        assert_eq!(scores.penalty(bad, NOW + 60), 2);
        assert_eq!(scores.ranked(&[bad, good], NOW + 60), [1, 0]);
        assert_eq!(scores.penalty(bad, NOW + 60 + HALF_LIFE), 1);
        assert_eq!(scores.ranked(&[bad, good], NOW + HALF_LIFE), [1, 0]);
        assert_eq!(scores.penalty(bad, NOW + 60 + 2 * HALF_LIFE), 0);
        assert_eq!(
            scores.ranked(&[bad, good], NOW + 60 + 2 * HALF_LIFE),
            [0, 1]
        );
        assert_eq!(scores.penalty(bad, u64::MAX), 0);
    }
}
