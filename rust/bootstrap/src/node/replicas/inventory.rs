//! Filter inventory visibility by node trust and proximity before accessing media storage.
use super::{proto, replica_key, Behaviour, PeerId, Swarm, REPLICAS};
use libp2p::kad;
use std::collections::HashMap;

fn near(swarm: &mut Swarm<Behaviour>, peer: PeerId, hash: &[u8]) -> bool {
    (0..10).any(|index| {
        let key = kad::KBucketKey::new(replica_key(hash, index));
        // Finish the iterator borrow before the bucket key is dropped.
        let found = swarm
            .behaviour_mut()
            .kad
            .get_closest_local_peers(&key)
            .take(REPLICAS.into())
            .any(|candidate| *candidate.preimage() == peer);
        found
    })
}

pub(super) fn allowed(
    swarm: &mut Swarm<Behaviour>,
    peer: PeerId,
    items: &[proto::MediaPart],
) -> Vec<proto::MediaPart> {
    let mut cache = HashMap::new();
    items
        .iter()
        .filter(|part| {
            *cache
                .entry(part.hash.clone())
                .or_insert_with(|| near(swarm, peer, &part.hash))
        })
        .cloned()
        .collect()
}
