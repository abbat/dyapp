use super::*;
use crate::{config::NodeConfig, media::MediaStore, service::Peer, storage::BootstrapStore};
use dyapp_identity::{Domain, Identity};
use prost::Message;

fn service() -> Service {
    let dir = format!("/tmp/ai/media-repair-{}", uuid::Uuid::new_v4());
    let mut config = NodeConfig::default();
    config.storage.dir = dir.clone().into();
    config.network.id_pow_bits = 0;
    config.limits.min_free_mb = 0;
    let mut service = Service::new(BootstrapStore::new(&dir).unwrap(), config);
    service.media = Some(MediaStore::open(std::path::Path::new(&dir)).unwrap());
    service
}

fn proof(hash: Vec<u8>) -> SignedRecord {
    Identity::generate().sign(
        Domain::MediaKeep,
        proto::MediaKeep {
            hashes: vec![hash],
            version: 1,
            time: chrono::Utc::now().timestamp().unsigned_abs(),
        }
        .encode_to_vec(),
    )
}

fn replica(
    record: &SignedRecord,
    data: Vec<u8>,
    manifest: Option<proto::MediaManifest>,
) -> proto::MediaReplicaPut {
    proto::MediaReplicaPut {
        data,
        manifest,
        authorization: Some(proto::media_replica_put::Authorization::Keep(
            record.clone(),
        )),
    }
}

fn keep(service: &Service, record: &SignedRecord) {
    let peer = Peer {
        id: "repair-test".into(),
        group: "loopback".into(),
        coarse: "loopback".into(),
    };
    assert_eq!(
        service
            .media(
                &peer,
                proto::MediaRequest {
                    request: Some(proto::media_request::Request::Keep(record.clone())),
                }
            )
            .unwrap()
            .status,
        i32::from(proto::Status::Ok)
    );
}

fn drive(
    repair: &mut Repair,
    origin: &Service,
    me: PeerId,
    nodes: &HashMap<PeerId, &Service>,
    fail_get: bool,
) -> Vec<proto::media_request::Request> {
    let mut requests = Vec::new();
    let mut failed = false;
    for _ in 0..32 {
        repair.advance(origin, me);
        if repair.done() {
            return requests;
        }
        let queued = repair.queued();
        assert!(!queued.is_empty(), "repair stuck");
        for (to, job) in queued {
            let (request, _) = repair.request(job).unwrap();
            let service = nodes[&to];
            let mut response = match request.request.as_ref().unwrap() {
                proto::media_request::Request::Have(have) => {
                    service.media_inventory(&have.items).unwrap()
                }
                proto::media_request::Request::ReplicaPut(put) => service.store_media(put).unwrap(),
                proto::media_request::Request::ShardPut(put) => service.store_shard(put).unwrap(),
                _ => service
                    .media(
                        &Peer {
                            id: "repair-transfer".into(),
                            group: "loopback".into(),
                            coarse: "loopback".into(),
                        },
                        request.clone(),
                    )
                    .unwrap(),
            };
            let fail = fail_get
                && !failed
                && matches!(request.request, Some(proto::media_request::Request::Get(_)));
            if fail {
                failed = true;
                response.data[0] ^= 1;
            }
            requests.push(request.request.unwrap());
            repair.response(job, Some(response));
            assert!(!repair.has_job(job));
        }
    }
    panic!("repair did not finish");
}

#[test]
fn locally_refused_repair_write_releases_the_job_and_preserves_recoverability() {
    let (origin, target) = (service(), service());
    let (me, remote) = (PeerId::random(), PeerId::random());
    let data = b"capped repair".to_vec();
    let hash = dyapp_identity::sha256(&data).to_vec();
    let record = proof(hash.clone());
    origin.store_media(&replica(&record, data, None)).unwrap();
    let mut repair = Repair::new(&origin, record, vec![hash.clone()], Instant::now()).unwrap();
    for index in repair.keys(0) {
        repair.holder(0, index, Some(remote));
    }
    repair.advance(&origin, me);
    for (to, job) in repair.queued() {
        assert_eq!(to, remote);
        let (request, _) = repair.request(job).unwrap();
        let Some(proto::media_request::Request::Have(have)) = request.request else {
            panic!("expected inventory");
        };
        repair.response(job, Some(target.media_inventory(&have.items).unwrap()));
    }
    repair.advance(&origin, me);
    let writes = repair.queued();
    assert_eq!(writes.len(), 1);
    let (_, job) = writes[0];
    assert!(matches!(
        repair.request(job).unwrap().0.request,
        Some(proto::media_request::Request::ReplicaPut(_))
    ));
    // The node uses None for a local cap refusal, without a network failure callback.
    repair.response(job, None);
    assert!(!repair.has_job(job));
    repair.advance(&origin, me);
    assert!(repair.done());
    assert!(repair.queued().is_empty());
    assert_eq!(
        repair.results().collect::<Vec<_>>(),
        vec![(hash.as_slice(), false)]
    );
    assert_eq!(target.media.as_ref().unwrap().get(&hash).unwrap(), None);
}

#[test]
fn whole_copy_repair_batches_by_holder_and_only_writes_the_gap() {
    let (origin, target) = (service(), service());
    let (me, remote) = (PeerId::random(), PeerId::random());
    let data = b"whole repair".to_vec();
    let hash = dyapp_identity::sha256(&data).to_vec();
    let record = proof(hash.clone());
    assert_eq!(
        origin
            .store_media(&replica(&record, data.clone(), None))
            .unwrap()
            .status,
        i32::from(proto::Status::Ok)
    );
    let mut repair = Repair::new(&origin, record, vec![hash.clone()], Instant::now()).unwrap();
    for index in repair.keys(0) {
        repair.holder(0, index, Some(if index == 1 { remote } else { me }));
    }
    assert_eq!(repair.groups.len(), 2);
    assert!(repair.groups.values().all(|items| items.len() == 1));
    let requests = drive(
        &mut repair,
        &origin,
        me,
        &HashMap::from([(me, &origin), (remote, &target)]),
        false,
    );
    assert_eq!(
        requests
            .iter()
            .filter(|r| matches!(r, proto::media_request::Request::Have(_)))
            .count(),
        2
    );
    assert_eq!(
        requests
            .iter()
            .filter(|r| matches!(r, proto::media_request::Request::ReplicaPut(_)))
            .count(),
        1
    );
    assert!(!requests.iter().any(|r| matches!(
        r,
        proto::media_request::Request::Get(_) | proto::media_request::Request::ShardGet(_)
    )));
    assert_eq!(
        target.media.as_ref().unwrap().get(&hash).unwrap(),
        Some(data)
    );
    assert_eq!(
        repair.results().collect::<Vec<_>>(),
        vec![(hash.as_slice(), false)]
    );
    repair.response(999, None);
    assert!(repair.request(999).is_none());
}

#[test]
fn unknown_layout_is_discovered_and_only_the_missing_shard_is_rebuilt() {
    let (origin, source, target) = (service(), service(), service());
    let (me, from, to) = (PeerId::random(), PeerId::random(), PeerId::random());
    let data = vec![7; MEDIA_OBJECT_BYTES + 1];
    let (manifest, shards) = media_encode(&data).unwrap();
    let hash = manifest.hash.clone();
    let record = proof(hash.clone());
    keep(&origin, &record);
    let replica = replica(&record, Vec::new(), Some(manifest.clone()));
    for service in [&source, &target] {
        for (index, shard) in shards.iter().enumerate() {
            if std::ptr::eq(service, &target) && index == 1 {
                continue;
            }
            assert_eq!(
                service
                    .store_shard(&proto::MediaShardPut {
                        replica: Some(replica.clone()),
                        index: index as u32,
                        data: shard.clone(),
                    })
                    .unwrap()
                    .status,
                i32::from(proto::Status::Ok)
            );
        }
    }
    let mut repair = Repair::new(&origin, record, vec![hash.clone()], Instant::now()).unwrap();
    assert_eq!(repair.keys(0).len(), 10);
    for index in repair.keys(0) {
        repair.holder(0, index, Some(if index == 1 { to } else { from }));
    }
    let requests = drive(
        &mut repair,
        &origin,
        me,
        &HashMap::from([(from, &source), (to, &target)]),
        false,
    );
    let puts = requests
        .iter()
        .filter_map(|r| match r {
            proto::media_request::Request::ShardPut(p) => Some(p.index),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(puts, vec![1]);
    assert!(requests
        .iter()
        .any(|r| matches!(r, proto::media_request::Request::ShardGet(_))));
    assert!(!requests
        .iter()
        .any(|r| matches!(r, proto::media_request::Request::ReplicaPut(_))));
    assert_eq!(
        dyapp_identity::sha256(
            &target
                .media
                .as_ref()
                .unwrap()
                .shard(&hash, 1)
                .unwrap()
                .unwrap()
        ),
        dyapp_identity::sha256(&shards[1])
    );
    assert_eq!(
        repair.results().collect::<Vec<_>>(),
        vec![(hash.as_slice(), false)]
    );
}

#[test]
fn missing_or_invalid_inventory_reports_a_blob_for_reupload() {
    let origin = service();
    let (me, remote) = (PeerId::random(), PeerId::random());
    let hash = vec![8; 32];
    let record = proof(hash.clone());
    keep(&origin, &record);
    let mut repair = Repair::new(&origin, record, vec![hash.clone()], Instant::now()).unwrap();
    for index in repair.keys(0) {
        repair.holder(0, index, Some(remote));
    }
    repair.advance(&origin, me);
    let (_, job) = repair.queued().pop().unwrap();
    repair.response(
        job,
        Some(proto::MediaResponse {
            status: proto::Status::Ok.into(),
            inventory: Some(proto::MediaInventory {
                present: vec![proto::MediaPart {
                    hash: vec![9; 32],
                    index: None,
                }],
                ..Default::default()
            }),
            ..Default::default()
        }),
    );
    repair.advance(&origin, me);
    assert!(repair.done());
    assert_eq!(
        repair.results().collect::<Vec<_>>(),
        vec![(hash.as_slice(), true)]
    );
    let mut repair = Repair::new(
        &origin,
        proof(hash.clone()),
        vec![hash.clone()],
        Instant::now(),
    )
    .unwrap();
    for index in repair.keys(0) {
        repair.holder(0, index, None);
    }
    repair.advance(&origin, me);
    assert!(repair.done());
    assert!(repair.results().next().unwrap().1);
}

#[test]
fn a_bad_source_is_retried_and_fetches_are_hash_checked() {
    let (origin, source, second, target) = (service(), service(), service(), service());
    let (me, a, b, to) = (
        PeerId::random(),
        PeerId::random(),
        PeerId::random(),
        PeerId::random(),
    );
    let data = b"retry repair".to_vec();
    let hash = dyapp_identity::sha256(&data).to_vec();
    let record = proof(hash.clone());
    keep(&origin, &record);
    for s in [&source, &second] {
        s.store_media(&replica(&record, data.clone(), None))
            .unwrap();
    }
    let mut repair = Repair::new(&origin, record, vec![hash.clone()], Instant::now()).unwrap();
    for index in repair.keys(0) {
        repair.holder(
            0,
            index,
            Some(match index {
                0 => to,
                1 => a,
                _ => b,
            }),
        );
    }
    let requests = drive(
        &mut repair,
        &origin,
        me,
        &HashMap::from([(a, &source), (b, &second), (to, &target)]),
        true,
    );
    assert_eq!(
        requests
            .iter()
            .filter(|r| matches!(r, proto::media_request::Request::Get(_)))
            .count(),
        2
    );
    assert_eq!(
        target.media.as_ref().unwrap().get(&hash).unwrap(),
        Some(data)
    );
    assert!(!repair.results().next().unwrap().1);
}

#[test]
fn a_manifest_with_fewer_than_k_shards_and_a_bad_decode_are_missing() {
    let (origin, source, target) = (service(), service(), service());
    let (me, from, to) = (PeerId::random(), PeerId::random(), PeerId::random());
    let (mut manifest, shards) = media_encode(&vec![3; MEDIA_OBJECT_BYTES + 1]).unwrap();
    manifest.hash[0] ^= 1;
    let hash = manifest.hash.clone();
    let record = proof(hash.clone());
    let replica = replica(&record, Vec::new(), Some(manifest));
    origin.store_media(&replica).unwrap();
    source
        .store_shard(&proto::MediaShardPut {
            replica: Some(replica.clone()),
            index: 0,
            data: shards[0].clone(),
        })
        .unwrap();
    let run = |origin: &Service| {
        let mut repair =
            Repair::new(origin, record.clone(), vec![hash.clone()], Instant::now()).unwrap();
        for index in repair.keys(0) {
            repair.holder(0, index, Some(if index == 1 { to } else { from }));
        }
        let requests = drive(
            &mut repair,
            origin,
            me,
            &HashMap::from([(from, &source), (to, &target)]),
            false,
        );
        assert!(repair.results().next().unwrap().1);
        assert!(!requests.iter().any(|r| matches!(
            r,
            proto::media_request::Request::ReplicaPut(_)
                | proto::media_request::Request::ShardPut(_)
        )));
    };
    run(&origin);
    source
        .store_shard(&proto::MediaShardPut {
            replica: Some(replica.clone()),
            index: 2,
            data: shards[2].clone(),
        })
        .unwrap();
    run(&origin);
    assert!(origin
        .media
        .as_ref()
        .unwrap()
        .manifest(&hash)
        .unwrap()
        .is_none());
}
