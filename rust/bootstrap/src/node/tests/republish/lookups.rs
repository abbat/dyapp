//! Lookup completion is driven directly, independently of network timing.
use super::*;

fn photo(source: &Service) -> (proto::MediaReplicaPut, Vec<u8>) {
    let data = b"lookup photo".to_vec();
    let hash = dyapp_identity::sha256(&data).to_vec();
    let record = dyapp_identity::Identity::from_secret(&[61; 32]).sign(
        dyapp_identity::Domain::MediaKeep,
        proto::MediaKeep {
            version: 1,
            hashes: vec![hash.clone()],
            time: chrono::Utc::now().timestamp().unsigned_abs(),
        }
        .encode_to_vec(),
    );
    let replica = proto::MediaReplicaPut {
        data,
        authorization: Some(proto::media_replica_put::Authorization::Keep(record)),
        ..Default::default()
    };
    assert_eq!(
        source.store_media(&replica).unwrap().status,
        i32::from(Status::Ok)
    );
    (replica, hash)
}

#[tokio::test]
async fn successful_empty_lookup_confirms_the_local_holder() {
    let (source, mut swarm) = node();
    let (replica, hash) = photo(&source);
    let query = swarm.behaviour_mut().kad.get_closest_peers(hash.clone());
    let mut replicas = Replicas::default();
    replicas
        .puts
        .insert(1, PendingPut::new(PutRecord::Media(replica), None, true));
    replicas.lookups.insert(query, Lookup::Put(1));
    replicas.closest(
        &mut swarm,
        &source,
        &Watchers::default(),
        query,
        Ok(kad::GetClosestPeersOk {
            key: hash,
            peers: vec![],
        }),
    );
    assert!(!replicas.lookups.contains_key(&query));
    assert_eq!(replicas.media_holders, (1, 1));
}

#[tokio::test]
async fn failed_lookup_releases_one_repair_slot_without_confirming_a_holder() {
    let (source, mut swarm) = node();
    let (replica, hash) = photo(&source);
    let Some(proto::media_replica_put::Authorization::Keep(record)) = replica.authorization else {
        panic!("expected signed keep");
    };
    let repair = MediaRepair::new(&source, record, vec![hash.clone()], Instant::now()).unwrap();
    let remaining = repair.lookups - 1;
    let query = swarm.behaviour_mut().kad.get_closest_peers(hash.clone());
    let mut replicas = Replicas::default();
    replicas.media_repairs.insert(1, (repair, None));
    replicas.lookups.insert(query, Lookup::MediaRepair(1, 0, 0));
    replicas.closest(
        &mut swarm,
        &source,
        &Watchers::default(),
        query,
        Err(kad::GetClosestPeersError::Timeout {
            key: hash,
            peers: vec![],
        }),
    );
    assert!(!replicas.lookups.contains_key(&query));
    let repair = &replicas.media_repairs.get(&1).unwrap().0;
    assert_eq!(repair.lookups, remaining);
    assert!(!repair.transferred());
}
