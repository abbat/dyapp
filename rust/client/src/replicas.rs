//! Discovery of the nodes holding the independent replicas of one mailbox.

use crate::{Node, TIMEOUT};
use dyapp_p2p_net::{id_has_pow, id_pow_bits, replica_key, BehaviourEvent, Protocols, REPLICAS};
use libp2p::futures::StreamExt;
use libp2p::kad;
use libp2p::swarm::SwarmEvent;
use libp2p::{Multiaddr, PeerId};
use std::collections::HashMap;
use std::sync::Arc;

/// Connected mailbox holders and the replica key each can serve.
///
/// A node honestly close to several replica keys is connected only once. Missing entries are
/// lookup or connection failures; callers can still use the holders that were reached.
pub struct ReplicaSet {
    nodes: Vec<Node>,
    keys: Vec<Vec<u8>>,
    holders: Vec<Option<usize>>,
}

impl ReplicaSet {
    /// Connected distinct holders, suitable for score-based request fallback.
    pub fn nodes_mut(&mut self) -> &mut [Node] {
        &mut self.nodes
    }

    /// The connected holder of exactly `key`, not merely another mailbox replica.
    pub fn for_key(&mut self, key: &[u8]) -> Option<&mut Node> {
        let index = self.keys.iter().position(|candidate| candidate == key)?;
        let holder = self.holders.get(index).copied().flatten()?;
        self.nodes.get_mut(holder)
    }

    /// Two nodes associated with different replica keys, for an independent mailbox comparison.
    /// One physical node serving both keys is not independent evidence.
    pub fn pair_mut(&mut self) -> Option<(&mut Node, &mut Node)> {
        let first = self.holders.iter().flatten().next().copied()?;
        let second = self
            .holders
            .iter()
            .flatten()
            .find(|&&holder| holder != first)
            .copied()?;
        let [first, second] = self.nodes.get_disjoint_mut([first, second]).ok()?;
        Some((first, second))
    }

    /// Replica keys without a connected holder.
    pub fn missing(&self) -> impl Iterator<Item = &[u8]> {
        self.keys
            .iter()
            .zip(&self.holders)
            .filter(|(_, holder)| holder.is_none())
            .map(|(key, _)| key.as_slice())
    }
}

impl Node {
    /// Finds and connects one holder for every replica of `mailbox` through this entry node.
    /// Each lookup and dial has its own [`TIMEOUT`]; failed keys remain absent from the result.
    pub async fn replicas(&mut self, mailbox: &[u8], protocols: Arc<Protocols>) -> ReplicaSet {
        let keys = (0..REPLICAS)
            .map(|index| replica_key(mailbox, index))
            .collect::<Vec<_>>();
        let mut result = ReplicaSet {
            nodes: Vec::new(),
            holders: vec![None; keys.len()],
            keys: keys.clone(),
        };
        let mut connected = HashMap::<PeerId, usize>::new();
        for (index, key) in keys.iter().enumerate() {
            let candidates = self.closest(key).await;
            if !add_new_holder(&mut result, &mut connected, index, &candidates, &protocols).await {
                reuse_holder(&mut result, &connected, index, &candidates);
            }
        }
        result
    }

    async fn closest(&mut self, key: &[u8]) -> Vec<kad::PeerInfo> {
        let query = self
            .swarm
            .behaviour_mut()
            .kad
            .get_closest_peers(key.to_vec());
        let mut found = Vec::new();
        let collect = async {
            loop {
                let event = self.swarm.select_next_some().await;
                let Some((result, last)) = closest_event(event, query) else {
                    continue;
                };
                if let Ok(result) = result {
                    found.extend(result.peers);
                }
                if last {
                    break;
                }
            }
        };
        let _ = tokio::time::timeout(TIMEOUT, collect).await;
        for candidate in &mut found {
            if candidate.peer_id == self.peer && candidate.addrs.is_empty() {
                candidate.addrs.push(self.address.clone());
            }
        }
        found
    }
}

fn closest_event(
    event: SwarmEvent<BehaviourEvent>,
    query: kad::QueryId,
) -> Option<(kad::GetClosestPeersResult, bool)> {
    let SwarmEvent::Behaviour(BehaviourEvent::Kad(kad::Event::OutboundQueryProgressed {
        id,
        result: kad::QueryResult::GetClosestPeers(result),
        step,
        ..
    })) = event
    else {
        return None;
    };
    (id == query).then_some((result, step.last))
}

async fn add_new_holder(
    result: &mut ReplicaSet,
    connected: &mut HashMap<PeerId, usize>,
    index: usize,
    candidates: &[kad::PeerInfo],
    protocols: &Arc<Protocols>,
) -> bool {
    for candidate in candidates {
        if connected.contains_key(&candidate.peer_id) {
            continue;
        }
        if add_holder(result, connected, index, candidate.clone(), protocols).await {
            return true;
        }
    }
    false
}

fn reuse_holder(
    result: &mut ReplicaSet,
    connected: &HashMap<PeerId, usize>,
    index: usize,
    candidates: &[kad::PeerInfo],
) {
    let holder = candidates
        .iter()
        .find_map(|candidate| connected.get(&candidate.peer_id));
    if let (Some(holder), Some(slot)) = (holder, result.holders.get_mut(index)) {
        *slot = Some(*holder);
    }
}

async fn add_holder(
    result: &mut ReplicaSet,
    connected: &mut HashMap<PeerId, usize>,
    index: usize,
    candidate: kad::PeerInfo,
    protocols: &Arc<Protocols>,
) -> bool {
    if !id_has_pow(&candidate.peer_id, id_pow_bits()) {
        return false;
    }
    if let Some(&holder) = connected.get(&candidate.peer_id) {
        let Some(slot) = result.holders.get_mut(index) else {
            return false;
        };
        *slot = Some(holder);
        return true;
    }
    let Some(node) = connect(candidate.peer_id, candidate.addrs, protocols).await else {
        return false;
    };
    let holder = result.nodes.len();
    result.nodes.push(node);
    connected.insert(candidate.peer_id, holder);
    let Some(slot) = result.holders.get_mut(index) else {
        return false;
    };
    *slot = Some(holder);
    true
}

async fn connect(
    peer: PeerId,
    addresses: Vec<Multiaddr>,
    protocols: &Arc<Protocols>,
) -> Option<Node> {
    for address in addresses {
        let Ok(node) = Node::connect(address, Arc::clone(protocols)).await else {
            continue;
        };
        if node.peer() == peer {
            return Some(node);
        }
    }
    None
}

#[cfg(test)]
mod tests;
