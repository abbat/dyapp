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
            let (request, reply) = repair.request(job).unwrap();
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
            // The reply budget is the response cap plus 1 KiB of framing.
            assert!(response.encoded_len() as u64 <= reply);
            let expected = match request.request.as_ref().unwrap() {
                proto::media_request::Request::Have(have) => have.items.len() * 512 + 1024,
                proto::media_request::Request::Get(_) => MEDIA_OBJECT_BYTES + 1024,
                proto::media_request::Request::ShardGet(_) => response.data.len() + 1024,
                _ => reply as usize,
            };
            assert_eq!(reply, expected as u64);
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
fn an_inventory_is_ignored_unless_every_entry_was_requested_and_bounded() {
    let origin = service();
    let remote = PeerId::random();
    let (manifest, _) = media_encode(&vec![7; MEDIA_OBJECT_BYTES + 1]).unwrap();
    let hash = manifest.hash.clone();
    let record = proof(hash.clone());
    keep(&origin, &record);
    let fresh = || {
        let mut repair =
            Repair::new(&origin, record.clone(), vec![hash.clone()], Instant::now()).unwrap();
        for index in repair.keys(0) {
            repair.holder(0, index, Some(remote));
        }
        repair
    };
    let part = |hash: &[u8], index| proto::MediaPart {
        hash: hash.to_vec(),
        index,
    };
    let gap = || vec![part(&hash, None)];
    let other = vec![9; 32];
    let foreign = proto::MediaManifest {
        hash: other.clone(),
        ..manifest.clone()
    };
    assert!(foreign.data_shards().is_some());
    let accepted = |inventory| {
        let mut repair = fresh();
        repair.inventory(remote, inventory);
        !repair.blobs[0].gaps.is_empty()
    };
    let inventory = |missing, whole, manifests| proto::MediaInventory {
        missing,
        whole,
        manifests,
        ..Default::default()
    };
    // A full window of whole copies and manifests is accepted; one more is not.
    assert!(accepted(inventory(
        gap(),
        vec![hash.clone(); WINDOW],
        vec![manifest.clone(); WINDOW]
    )));
    assert!(!accepted(inventory(
        gap(),
        vec![hash.clone(); WINDOW + 1],
        Vec::new()
    )));
    assert!(!accepted(inventory(
        gap(),
        Vec::new(),
        vec![manifest.clone(); WINDOW + 1]
    )));
    // Any part, whole copy or manifest for an unrequested hash discards it all.
    let mut missing = gap();
    missing.push(part(&other, None));
    assert!(!accepted(inventory(missing, Vec::new(), Vec::new())));
    assert!(!accepted(inventory(gap(), vec![other], Vec::new())));
    assert!(!accepted(inventory(gap(), Vec::new(), vec![foreign])));

    // Shard holders count only under the manifest the repair adopted.
    let mut repair = fresh();
    repair.inventory(
        remote,
        inventory(Vec::new(), Vec::new(), vec![manifest.clone()]),
    );
    let mut changed = manifest.clone();
    changed.shard_hashes[0] = vec![1; 32];
    let present = |manifest, index| proto::MediaInventory {
        present: vec![part(&hash, Some(index))],
        manifests: vec![manifest],
        ..Default::default()
    };
    repair.inventory(remote, present(changed, 0));
    assert!(repair.blobs[0].shard_sources.is_empty());
    // Requested, since keys cover ten shards before the layout is known.
    let past = manifest.shard_hashes.len() as u32;
    repair.inventory(remote, present(manifest.clone(), past));
    assert!(repair.blobs[0].shard_sources.is_empty());
    repair.inventory(remote, present(manifest, 0));
    assert_eq!(repair.blobs[0].shard_sources[&0], [remote]);
}

#[test]
fn local_copies_count_only_when_they_verify() {
    let me = PeerId::random();
    let missing = |origin: &Service, record: &SignedRecord, hash: &[u8]| {
        let mut repair =
            Repair::new(origin, record.clone(), vec![hash.to_vec()], Instant::now()).unwrap();
        for index in repair.keys(0) {
            repair.holder(0, index, None);
        }
        repair.advance(origin, me);
        assert!(repair.done());
        let missing = repair.results().next().unwrap().1;
        missing
    };
    // A whole copy whose bytes no longer match its hash is not a source.
    let origin = service();
    let data = b"rotting".to_vec();
    let hash = dyapp_identity::sha256(&data).to_vec();
    let record = proof(hash.clone());
    origin
        .store_media(&replica(&record, data.clone(), None))
        .unwrap();
    assert!(!missing(&origin, &record, &hash));
    let path = origin.media.as_ref().unwrap().path(&hash);
    std::fs::write(path, b"rotten!").unwrap();
    assert!(missing(&origin, &record, &hash));

    // Enough verified local shards make a sharded blob recoverable without peers.
    let origin = service();
    let (manifest, shards) = media_encode(&vec![7; MEDIA_OBJECT_BYTES + 1]).unwrap();
    let hash = manifest.hash.clone();
    let record = proof(hash.clone());
    keep(&origin, &record);
    let replica = replica(&record, Vec::new(), Some(manifest));
    for (index, data) in shards.into_iter().enumerate() {
        let put = proto::MediaShardPut {
            replica: Some(replica.clone()),
            index: index as u32,
            data,
        };
        assert_eq!(
            origin.store_shard(&put).unwrap().status,
            i32::from(proto::Status::Ok)
        );
    }
    assert!(!missing(&origin, &record, &hash));
}

#[test]
fn a_whole_copy_listed_by_this_node_falls_through_to_the_next_source() {
    let origin = service();
    let (me, other, lacking) = (PeerId::random(), PeerId::random(), PeerId::random());
    let hash = vec![8; 32];
    let record = proof(hash.clone());
    keep(&origin, &record);
    let mut repair = Repair::new(&origin, record, vec![hash.clone()], Instant::now()).unwrap();
    for index in repair.keys(0) {
        for peer in [other, me, lacking] {
            repair.holder(0, index, Some(peer));
        }
    }
    repair.inventoried = true;
    repair.inventory(
        lacking,
        proto::MediaInventory {
            missing: vec![proto::MediaPart {
                hash: hash.clone(),
                index: None,
            }],
            ..Default::default()
        },
    );
    // Sources are tried last-in first, so this node's own listing comes up first.
    for peer in [other, me] {
        repair.inventory(
            peer,
            proto::MediaInventory {
                whole: vec![hash.clone()],
                ..Default::default()
            },
        );
    }
    repair.advance(&origin, me);
    let queued = repair.queued();
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].0, other);
    assert!(matches!(
        repair.request(queued[0].1).unwrap().0.request,
        Some(proto::media_request::Request::Get(_))
    ));
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
