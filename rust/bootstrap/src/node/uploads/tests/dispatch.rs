//! Drive actual node queue/reply callbacks with separately stored holder data.
use super::*;
use crate::node::{Lookup, Watchers};
use crate::replication::{media_encode, MEDIA_OBJECT_BYTES};
#[path = "dispatch/lifecycle.rs"]
mod lifecycle;

fn record(chain: &Chain, data: Vec<u8>, sharded: bool) -> PutRecord {
    let mut replica = proto::MediaReplicaPut {
        data,
        authorization: Some(proto::media_replica_put::Authorization::Keep(
            chain.target().clone(),
        )),
        ..Default::default()
    };
    if sharded {
        let (manifest, shards) = media_encode(&replica.data).unwrap();
        replica.data.clear();
        replica.manifest = Some(manifest);
        PutRecord::Sharded(replica, std::sync::Arc::new(shards))
    } else {
        PutRecord::Media(replica)
    }
}

struct Upload {
    chain: Chain,
    second: Service,
    swarm: Swarm<Behaviour>,
    replicas: Replicas,
    peers: [PeerId; 2],
    lookup: libp2p::kad::QueryId,
    now: Instant,
}

impl Upload {
    fn new(sharded: bool) -> Self {
        let data = if sharded {
            vec![8; MEDIA_OBJECT_BYTES + 1]
        } else {
            b"uploaded photo".to_vec()
        };
        let hash = dyapp_identity::sha256(&data).to_vec();
        let chain = Chain::new(&hash);
        let mut swarm = crate::node::swarm(
            Keypair::generate_ed25519(),
            &chain.source.config.limits,
            &chain.source.config.roles,
        )
        .unwrap();
        let lookup = swarm.behaviour_mut().kad.get_closest_peers(hash);
        let mut replicas = Replicas::default();
        replicas.puts.insert(
            1,
            PendingPut::new(record(&chain, data, sharded), None, false),
        );
        // The second holder lookup remains active while first-holder callbacks complete.
        replicas.lookups.insert(lookup, Lookup::Put(1));
        let now = Instant::now();
        replicas.puts.get_mut(&1).unwrap().deadline = now + PUT_TIMEOUT;
        let peers = [PeerId::random(), PeerId::random()];
        Self {
            chain,
            second: service(),
            swarm,
            replicas,
            peers,
            lookup,
            now,
        }
    }

    fn destination(&self, peer: PeerId) -> &Service {
        if peer == self.peers[0] {
            &self.chain.destination
        } else {
            &self.second
        }
    }

    fn queue(&mut self, peer: PeerId) {
        self.replicas.put_holder(
            &mut self.swarm,
            &self.chain.source,
            &Watchers::default(),
            1,
            peer,
        );
        let count = match &self.replicas.puts[&1].record {
            PutRecord::Sharded(_, shards) => shards.len(),
            _ => 0,
        };
        for index in 0..count {
            self.replicas
                .shard_holder(&mut self.swarm, &self.chain.source, 1, index, peer);
        }
        self.replicas
            .drain_media_at(&mut self.swarm, &self.chain.source, self.now);
    }

    fn ancestry(&mut self, probe: bool) {
        assert_eq!(self.replicas.uploads.requests.len(), 1);
        let (&id, flight) = self.replicas.uploads.requests.iter().next().unwrap();
        let to = flight.peer;
        let ancestors = if probe {
            vec![]
        } else {
            self.chain.records[..self.chain.records.len() - 1]
                .iter()
                .rev()
                .cloned()
                .collect()
        };
        let reply = self
            .destination(to)
            .media(&peer(), segment(self.chain.target(), ancestors))
            .unwrap();
        self.replicas.media_response_at(
            &mut self.swarm,
            &self.chain.source,
            to,
            id,
            reply,
            self.now,
        );
    }

    fn payload(&mut self) -> Option<OutboundRequestId> {
        let put = &self.replicas.puts[&1];
        let (replica, shards) = match &put.record {
            PutRecord::Media(replica) => (replica, None),
            PutRecord::Sharded(replica, shards) => (replica, Some(shards)),
            _ => panic!("expected photo upload"),
        };
        let (id, reply) = if let Some((&id, _)) = self.replicas.media_puts.iter().next() {
            let to = self.replicas.media_outgoing[&id];
            (id, self.destination(to).store_media(replica).unwrap())
        } else if let Some((&id, &(_, index))) = self.replicas.shard_puts.iter().next() {
            let to = self.replicas.media_outgoing[&id];
            let reply = self
                .destination(to)
                .store_shard(&proto::MediaShardPut {
                    replica: Some(replica.clone()),
                    index: u32::try_from(index).unwrap(),
                    data: shards.unwrap()[index].clone(),
                })
                .unwrap();
            (id, reply)
        } else {
            return None;
        };
        assert_eq!(reply.status, i32::from(proto::Status::Ok));
        let to = self.replicas.media_outgoing[&id];
        self.replicas.media_response_at(
            &mut self.swarm,
            &self.chain.source,
            to,
            id,
            reply,
            self.now,
        );
        Some(id)
    }

    fn drain_payloads(&mut self) {
        let mut completed = std::collections::HashSet::new();
        while !self.replicas.media_outgoing.is_empty() {
            let id = self.payload().expect("no ready write callback");
            assert!(completed.insert(id), "duplicate write completion");
            assert!(self.replicas.media_outgoing.len() <= 2);
        }
        assert!(self.replicas.media_queue.is_empty());
    }
}

#[tokio::test]
async fn metadata_is_not_a_write_ack_and_uploads_keep_distinct_holder_and_shard_quorums() {
    for sharded in [false, true] {
        let mut upload = Upload::new(sharded);
        upload.queue(upload.peers[0]);
        assert!(upload.replicas.media_puts.is_empty());
        assert!(upload.replicas.shard_puts.is_empty());
        upload.ancestry(true);
        assert!(upload.replicas.puts[&1].stored.is_empty());
        upload.ancestry(false);
        assert!(upload.replicas.puts[&1].stored.is_empty());
        assert!(upload.replicas.puts[&1].shard_stored.is_empty());
        upload.drain_payloads();
        assert_eq!(upload.replicas.puts[&1].stored.len(), 1);
        assert_eq!(upload.replicas.puts[&1].result(upload.now), None);
        upload.queue(upload.peers[1]);
        upload.ancestry(true);
        upload.ancestry(false);
        upload.drain_payloads();
        assert_eq!(upload.replicas.puts[&1].stored.len(), 2);
        assert_eq!(
            upload.replicas.puts[&1].result(upload.now),
            Some(proto::Status::Ok)
        );
        upload.replicas.lookups.remove(&upload.lookup);
    }
}
