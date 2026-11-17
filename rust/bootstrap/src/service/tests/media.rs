//! Media tests.
use super::*;

mod boundaries;

pub(in crate::service) fn media_service(
    change: impl FnOnce(&mut crate::config::Limits),
) -> Service {
    let mut s = service_with(change);
    s.media = Some(MediaStore::open(&s.config.storage.dir).unwrap());
    s
}

fn media_replica(owner: &Identity, data: Vec<u8>, version: u64) -> proto::MediaReplicaPut {
    let keep = proto::MediaKeep {
        version,
        hashes: vec![dyapp_identity::sha256(&data).to_vec()],
        time: crate::clock::Clock::system().timestamp().unsigned_abs(),
    };
    proto::MediaReplicaPut {
        data,
        manifest: None,
        authorization: Some(proto::media_replica_put::Authorization::Keep(
            owner.sign(Domain::MediaKeep, keep.encode_to_vec()),
        )),
    }
}

#[test]
fn media_size_hints_preserve_object_boundaries() {
    let s = media_service(|l| {
        l.media_requests_per_second = 100;
        l.bytes_per_second = 0;
    });
    let owner = Identity::try_generate().unwrap();
    let max = crate::replication::MEDIA_OBJECT_BYTES;
    let put = |size| {
        media_request::Request::Put(proto::MediaPut {
            owner: dyapp_identity::key_hash(&owner.public_key()).to_vec(),
            data: vec![0; size],
        })
    };
    let exact = media_size_call(&s, put(MAX_MEDIA_BYTES));
    assert_eq!(
        (exact.status, exact.max_bytes),
        (i32::from(Status::NotFound), 0)
    );
    let over = media_size_call(&s, put(MAX_MEDIA_BYTES + 1));
    assert_eq!(
        (over.status, over.max_bytes),
        (i32::from(Status::TooLarge), MAX_MEDIA_BYTES as u64)
    );
    check_replica_size_hints(&s, &owner, max);
    check_authorization_size_hints(&s, &owner, max);
}

fn media_size_call(s: &Service, request: media_request::Request) -> MediaResponse {
    s.media(
        &peer("media-size"),
        MediaRequest {
            request: Some(request),
        },
    )
    .unwrap()
}

fn check_replica_size_hints(s: &Service, owner: &Identity, max: usize) {
    let exact = media_replica(owner, vec![7; max], 1);
    assert_eq!(s.store_media(&exact).unwrap().status, i32::from(Status::Ok));
    let over = media_replica(owner, vec![7; max + 1], 2);
    let response = s.store_media(&over).unwrap();
    assert_eq!(
        (response.status, response.max_bytes),
        (i32::from(Status::TooLarge), max as u64)
    );
}

fn check_authorization_size_hints(s: &Service, owner: &Identity, max: usize) {
    let data = b"authorization-boundary".to_vec();
    let hash = dyapp_identity::sha256(&data).to_vec();
    let now = crate::clock::Clock::system().timestamp().unsigned_abs();
    let keep = proto::MediaKeep {
        version: 2,
        hashes: vec![hash.clone()],
        time: now,
    };
    let attach = proto::MediaAttach {
        release: vec![9; 32],
        hashes: vec![hash],
        created: now,
    };
    for (domain, payload) in [
        (Domain::MediaKeep, keep.encode_to_vec()),
        (Domain::MediaAttach, attach.encode_to_vec()),
    ] {
        for size in [max, max + 1] {
            check_authorization_size_hint(s, owner, max, &data, domain, &payload, size);
        }
    }
}

fn check_authorization_size_hint(
    s: &Service,
    owner: &Identity,
    max: usize,
    data: &[u8],
    domain: Domain,
    payload: &[u8],
    size: usize,
) {
    use proto::media_replica_put::Authorization;

    // For these sizes both payload varints have the same length.
    let probe = padded_record(owner, domain, payload.to_vec(), max);
    let overhead = probe.encoded_len() - probe.payload.len();
    let record = padded_record(owner, domain, payload.to_vec(), size - overhead);
    assert_eq!(record.encoded_len(), size);
    let (request, authorization) = if domain == Domain::MediaKeep {
        (
            media_request::Request::Keep(record.clone()),
            Authorization::Keep(record),
        )
    } else {
        (
            media_request::Request::Attach(record.clone()),
            Authorization::Attach(record),
        )
    };
    let direct = media_size_call(s, request);
    let replica = proto::MediaReplicaPut {
        data: data.to_vec(),
        authorization: Some(authorization),
        manifest: None,
    };
    let stored = s.store_media(&replica).unwrap();
    for response in [direct, stored] {
        assert_eq!(
            (response.status, response.max_bytes),
            if size == max {
                (i32::from(Status::Ok), 0)
            } else {
                (i32::from(Status::TooLarge), max as u64)
            }
        );
    }
}

#[test]
fn media_authorization_records_are_bounded_before_storage() {
    use proto::media_replica_put::Authorization;
    let s = media_service(|_| {});
    let peer = peer("media-proof-limit");
    let record = SignedRecord {
        payload: vec![0; crate::replication::MEDIA_OBJECT_BYTES],
        ..Default::default()
    };
    for request in [
        media_request::Request::Keep(record.clone()),
        media_request::Request::Attach(record.clone()),
    ] {
        assert_eq!(
            s.media(
                &peer,
                MediaRequest {
                    request: Some(request)
                }
            )
            .unwrap()
            .status,
            Status::TooLarge as i32,
        );
    }
    for authorization in [
        Authorization::Keep(record.clone()),
        Authorization::Attach(record),
    ] {
        let replica = proto::MediaReplicaPut {
            authorization: Some(authorization),
            ..Default::default()
        };
        assert_eq!(
            s.store_media(&replica).unwrap().status,
            Status::TooLarge as i32
        );
    }
}

#[test]
fn media_replicas_verify_proofs_and_never_restore_dropped_hashes() {
    let mut s = media_service(|_| {});
    let owner = Identity::try_generate().unwrap();
    let replica = media_replica(&owner, b"replica".to_vec(), 1);
    let hash = dyapp_identity::sha256(&replica.data);
    check_replica_proofs(&s, &replica);
    check_keep_replacement(&mut s, &owner, &replica, &hash);
    check_attachment_lifecycle(&mut s, &owner, &replica, &hash);
}

fn store_media_status(s: &Service, replica: &proto::MediaReplicaPut) -> Status {
    Status::try_from(s.store_media(replica).unwrap().status).unwrap()
}

fn check_replica_proofs(s: &Service, replica: &proto::MediaReplicaPut) {
    use proto::media_replica_put::Authorization;

    let mut bad = replica.clone();
    bad.authorization = None;
    assert_eq!(store_media_status(s, &bad), Status::Denied);
    bad.authorization = Some(Authorization::Keep(SignedRecord::default()));
    assert_eq!(store_media_status(s, &bad), Status::Denied);
    bad.data.push(1);
    bad.authorization = replica.authorization.clone();
    assert_eq!(store_media_status(s, &bad), Status::NotFound);
    assert_eq!(store_media_status(s, replica), Status::Ok);
    assert_eq!(
        store_media_status(s, replica),
        Status::Ok,
        "already held counts as stored"
    );
}

fn check_keep_replacement(
    s: &mut Service,
    owner: &Identity,
    replica: &proto::MediaReplicaPut,
    hash: &[u8],
) {
    use proto::media_replica_put::Authorization;

    // The signed proof survives a restart and supports acceptor forwarding.
    s.media = Some(MediaStore::open(&s.config.storage.dir).unwrap());
    assert_eq!(
        s.media
            .as_ref()
            .unwrap()
            .authorization(&owner.peer_id(), hash)
            .unwrap(),
        replica.authorization,
    );
    let newer = proto::MediaKeep {
        version: 2,
        hashes: Vec::new(),
        time: crate::clock::Clock::system().timestamp().unsigned_abs(),
    };
    let record = owner.sign(Domain::MediaKeep, newer.encode_to_vec());
    s.media
        .as_ref()
        .unwrap()
        .keep_signed(&owner.peer_id(), &newer, &record)
        .unwrap();
    assert_eq!(store_media_status(s, replica), Status::NotFound);
    assert_eq!(s.media.as_ref().unwrap().get(hash).unwrap(), None);
    assert_eq!(
        s.media
            .as_ref()
            .unwrap()
            .authorization(&owner.peer_id(), hash)
            .unwrap(),
        None
    );
    // Even if the latest keep permits a hash, a carried proof must name it too.
    let current = media_replica(owner, replica.data.clone(), 3);
    assert_eq!(store_media_status(s, &current), Status::Ok);
    let mut bad = current;
    bad.authorization = Some(Authorization::Keep(record));
    assert_eq!(store_media_status(s, &bad), Status::NotFound);
}

fn check_attachment_lifecycle(
    s: &mut Service,
    owner: &Identity,
    replica: &proto::MediaReplicaPut,
    hash: &[u8],
) {
    use proto::media_replica_put::Authorization;

    let attach = proto::MediaAttach {
        release: dyapp_identity::sha256(b"release").to_vec(),
        hashes: vec![hash.to_vec()],
        created: crate::clock::Clock::system().timestamp().unsigned_abs(),
    };
    let mut attachment = replica.clone();
    attachment.authorization = Some(Authorization::Attach(
        owner.sign(Domain::MediaAttach, attach.encode_to_vec()),
    ));
    assert_eq!(store_media_status(s, &attachment), Status::Ok);
    s.media = Some(MediaStore::open(&s.config.storage.dir).unwrap());
    assert_eq!(
        s.media
            .as_ref()
            .unwrap()
            .authorization(&owner.peer_id(), hash)
            .unwrap(),
        attachment.authorization,
        "unexpired attachments take precedence over potentially aged keeps"
    );
    // Releasing the attachment leaves the independently kept copy alive.
    assert!(s.media.as_ref().unwrap().release(b"release").unwrap());
    assert_eq!(store_media_status(s, replica), Status::Ok);
    assert_eq!(
        store_media_status(s, &attachment),
        Status::Refused,
        "a released attachment is not restored"
    );
    let expired = proto::MediaAttach {
        created: 1,
        ..attach
    };
    attachment.authorization = Some(Authorization::Attach(
        owner.sign(Domain::MediaAttach, expired.encode_to_vec()),
    ));
    assert_eq!(store_media_status(s, &attachment), Status::Invalid);
}

#[test]
fn deny_list_refuses_replica_and_shard_restoration() {
    let s = media_service(|_| {});
    let owner = Identity::try_generate().unwrap();
    let whole = media_replica(&owner, b"blocked whole".to_vec(), 1);
    let hash = dyapp_identity::sha256(&whole.data);
    assert_eq!(s.store_media(&whole).unwrap().status, i32::from(Status::Ok));
    let data = vec![7; (1 << 20) + 1];
    let (manifest, shards) = crate::replication::media_encode(&data).unwrap();
    let mut sharded = media_replica(&owner, data, 2);
    sharded.data.clear();
    sharded.manifest = Some(manifest.clone());
    let shard = proto::MediaShardPut {
        replica: Some(sharded.clone()),
        index: 0,
        data: shards[0].clone(),
    };
    assert_eq!(s.store_shard(&shard).unwrap().status, i32::from(Status::Ok));
    for hash in [hash.as_slice(), manifest.hash.as_slice()] {
        s.deny.write().unwrap().insert(hex(hash));
    }
    assert_eq!(
        s.store_media(&whole).unwrap().status,
        i32::from(Status::Refused)
    );
    assert_eq!(
        s.store_media(&sharded).unwrap().status,
        i32::from(Status::Refused)
    );
    assert_eq!(
        s.store_shard(&shard).unwrap().status,
        i32::from(Status::Refused)
    );
    assert_eq!(
        s.assemble_media(&manifest, &[], &proto::GetMedia::default())
            .unwrap()
            .status,
        i32::from(Status::Refused)
    );
}

#[test]
fn media_bills_whole_copy_puts_five_times_and_gets_by_reply_bytes() {
    let mut s = media_service(|l| l.bytes_per_second = 10_000);
    let owner = Identity::try_generate().unwrap();
    let put = check_whole_copy_billing(&mut s, &owner);
    check_get_reply_billing(&mut s, &put);
    check_replica_billing(&mut s, &owner, &put);
}

fn check_whole_copy_billing(s: &mut Service, owner: &Identity) -> proto::MediaPut {
    let one = media_replica(owner, vec![1; 1000], 1);
    assert_eq!(s.store_media(&one).unwrap().status, i32::from(Status::Ok));
    let put = proto::MediaPut {
        owner: dyapp_identity::key_hash(&owner.public_key()).to_vec(),
        data: one.data.clone(),
    };
    let put_framing = MediaRequest {
        request: Some(media_request::Request::Put(put.clone())),
    }
    .encoded_len() as u64
        - put.data.len() as u64;
    assert!(s.prepare_media(&peer("client"), &put).unwrap().is_ok());
    assert_eq!(
        s.traffic.charged_bytes(),
        5_000 + put_framing,
        "intake reserves five copies plus framing"
    );
    put
}

fn check_get_reply_billing(s: &mut Service, put: &proto::MediaPut) {
    // Test reply billing in its own window, independent of the host's second boundary.
    s.traffic = Traffic::new(10_000);
    let get = MediaRequest {
        request: Some(media_request::Request::Get(proto::GetMedia {
            hash: dyapp_identity::sha256(&put.data).to_vec(),
            ..Default::default()
        })),
    };
    let get_framing = get.encoded_len() as u64;
    assert_eq!(s.media(&peer("client"), get).unwrap().data, put.data);
    assert_eq!(
        s.traffic.charged_bytes(),
        1_000 + get_framing,
        "get reserves reply bytes plus framing"
    );
}

fn check_replica_billing(s: &mut Service, owner: &Identity, put: &proto::MediaPut) {
    // Isolate peer billing from the shared traffic guard.
    s.traffic = Traffic::new(0);
    s.media_bytes = crate::rate_limit::PeerRateLimiter::frozen(10_000);
    let big = media_replica(owner, vec![2; 1600], 2);
    assert_eq!(s.store_media(&big).unwrap().status, i32::from(Status::Ok));
    let put = proto::MediaPut {
        data: big.data.clone(),
        ..put.clone()
    };
    assert!(s
        .prepare_media(&replica_peer("gallery"), &put)
        .unwrap()
        .is_ok());
    assert_eq!(
        s.prepare_media(&replica_peer("gallery"), &put).unwrap(),
        Err(Status::RateLimited)
    );
    let node_put = || MediaRequest {
        request: Some(media_request::Request::ReplicaPut(big.clone())),
    };
    assert_eq!(
        s.media(&replica_peer("gallery"), node_put())
            .unwrap()
            .status,
        i32::from(Status::Ok)
    );
    assert_eq!(
        s.media(&replica_peer("gallery"), node_put())
            .unwrap()
            .status,
        i32::from(Status::RateLimited)
    );
    assert_eq!(
        s.media(&replica_peer("other-node"), node_put())
            .unwrap()
            .status,
        i32::from(Status::Ok)
    );
}

#[test]
fn media_whole_copy_owner_quota_includes_five_copies() {
    let s = media_service(|l| l.media_per_owner_mb = 1);
    let owner = Identity::try_generate().unwrap();
    let replica = media_replica(&owner, vec![1; 210_000], 1);
    assert_eq!(
        s.store_media(&replica).unwrap().status,
        i32::from(Status::Full)
    );
    let replica = media_replica(&owner, vec![2; 200_000], 2);
    assert_eq!(
        s.store_media(&replica).unwrap().status,
        i32::from(Status::Ok)
    );
    // A second owner cannot bypass its quota by listing a copy already on this node.
    let other = Identity::try_generate().unwrap();
    let first = media_replica(&other, vec![3; 20_000], 1);
    assert_eq!(s.store_media(&first).unwrap().status, i32::from(Status::Ok));
    let shared = media_replica(&other, replica.data.clone(), 2);
    let keep = proto::MediaKeep {
        version: 2,
        hashes: vec![
            dyapp_identity::sha256(&first.data).to_vec(),
            dyapp_identity::sha256(&shared.data).to_vec(),
        ],
        time: crate::clock::Clock::system().timestamp().unsigned_abs(),
    };
    let shared = proto::MediaReplicaPut {
        authorization: Some(proto::media_replica_put::Authorization::Keep(
            other.sign(Domain::MediaKeep, keep.encode_to_vec()),
        )),
        ..shared
    };
    assert_eq!(
        s.store_media(&shared).unwrap().status,
        i32::from(Status::Full)
    );
}

#[test]
fn media_manifests_shards_quota_and_corruption() {
    let s = media_service(|_| {});
    let owner = Identity::try_generate().unwrap();
    let data = vec![7; (1 << 20) + 1];
    let mut replica = media_replica(&owner, data.clone(), 1);
    let (manifest, shards) = crate::replication::media_encode(&data).unwrap();
    replica.data.clear();
    replica.manifest = Some(manifest.clone());
    let mut put = check_manifest_and_shard_corruption(&s, &replica, &manifest, &shards);
    let fragments = check_assembled_media(&s, &manifest, &shards, data.len());
    check_wrong_manifest_cleanup(&s, &owner, replica, &manifest, &mut put, &fragments);
}

fn check_manifest_and_shard_corruption(
    s: &Service,
    replica: &proto::MediaReplicaPut,
    manifest: &proto::MediaManifest,
    shards: &[Vec<u8>],
) -> proto::MediaShardPut {
    assert_eq!(
        s.store_media(replica).unwrap().status,
        i32::from(Status::Ok)
    );
    assert_eq!(
        s.media.as_ref().unwrap().manifest(&manifest.hash).unwrap(),
        Some(manifest.clone())
    );
    let mut conflicting = replica.clone();
    conflicting.manifest.as_mut().unwrap().shard_hashes[0][0] ^= 1;
    assert_eq!(
        s.store_media(&conflicting).unwrap().status,
        i32::from(Status::Invalid)
    );
    let mut put = proto::MediaShardPut {
        replica: Some(replica.clone()),
        index: 5,
        data: shards[5].clone(),
    };
    put.data[0] ^= 1;
    assert_eq!(
        s.store_shard(&put).unwrap().status,
        i32::from(Status::Invalid)
    );
    put.data = shards[5].clone();
    assert_eq!(s.store_shard(&put).unwrap().status, i32::from(Status::Ok));
    put
}

fn check_assembled_media(
    s: &Service,
    manifest: &proto::MediaManifest,
    shards: &[Vec<u8>],
    data_len: usize,
) -> Vec<(usize, Vec<u8>)> {
    let fragments = vec![(4, shards[4].clone()), (5, shards[5].clone())];
    let last = proto::GetMedia {
        offset: 1 << 20,
        len: 1 << 20,
        ..Default::default()
    };
    let piece = s.assemble_media(manifest, &fragments, &last).unwrap();
    assert_eq!((piece.data, piece.total_size), (vec![7], data_len as u64));
    // The next piece reads the assembled copy: no shards are passed in.
    let first = s
        .media(
            &peer("ranged"),
            MediaRequest {
                request: Some(media_request::Request::Get(proto::GetMedia {
                    hash: manifest.hash.clone(),
                    offset: 0,
                    len: 10,
                })),
            },
        )
        .unwrap();
    assert_eq!(
        (Status::try_from(first.status).unwrap(), first.data),
        (Status::Ok, vec![7; 10])
    );
    let persisted = MediaStore::open(&s.config.storage.dir).unwrap();
    assert_eq!(
        persisted.shard(&manifest.hash, 5).unwrap(),
        Some(shards[5].clone())
    );
    // Shards plus the assembled copy.
    assert!(persisted.usage().unwrap().0 <= 2 * ((1 << 20) + 1024));
    fragments
}

fn check_wrong_manifest_cleanup(
    s: &Service,
    owner: &Identity,
    mut replica: proto::MediaReplicaPut,
    manifest: &proto::MediaManifest,
    put: &mut proto::MediaShardPut,
    fragments: &[(usize, Vec<u8>)],
) {
    let persisted = MediaStore::open(&s.config.storage.dir).unwrap();
    let mut wrong = manifest.clone();
    wrong.hash[0] ^= 1;
    let keep = proto::MediaKeep {
        version: 2,
        hashes: vec![wrong.hash.clone()],
        time: crate::clock::Clock::system().timestamp().unsigned_abs(),
    };
    replica.manifest = Some(wrong.clone());
    replica.authorization = Some(proto::media_replica_put::Authorization::Keep(
        owner.sign(Domain::MediaKeep, keep.encode_to_vec()),
    ));
    assert_eq!(
        s.store_media(&replica).unwrap().status,
        i32::from(Status::Ok)
    );
    put.replica = Some(replica);
    assert_eq!(s.store_shard(put).unwrap().status, i32::from(Status::Ok));
    assert_eq!(
        s.assemble_media(&wrong, fragments, &proto::GetMedia::default())
            .unwrap()
            .status,
        i32::from(Status::NotFound)
    );
    assert_eq!(persisted.manifest(&wrong.hash).unwrap(), None);
    assert_eq!(persisted.shard(&wrong.hash, 5).unwrap(), None);
    let quota = media_service(|l| l.media_per_owner_mb = 1);
    assert_eq!(
        quota
            .store_media(put.replica.as_ref().unwrap())
            .unwrap()
            .status,
        i32::from(Status::Full)
    );
}

#[test]
fn a_verified_whole_copy_wins_and_sharded_uploads_pay_their_ratio() {
    let mut s = media_service(|l| l.bytes_per_second = 4 << 20);
    let owner = Identity::try_generate().unwrap();
    let small = media_replica(&owner, b"whole".to_vec(), 1);
    let (proof, manifest) = check_verified_whole_copy(&s, &small);
    check_whole_copy_replaces_manifest(&small, &proof, &manifest);
    check_sharded_upload_ratio(&mut s, &owner);
}

fn check_verified_whole_copy(
    s: &Service,
    small: &proto::MediaReplicaPut,
) -> (proto::MediaReplicaPut, proto::MediaManifest) {
    assert_eq!(s.store_media(small).unwrap().status, i32::from(Status::Ok));
    let (mut manifest, _) = crate::replication::media_encode(&small.data).unwrap();
    manifest.shard_hashes[0][0] ^= 1;
    let mut proof = small.clone();
    proof.data.clear();
    proof.manifest = Some(manifest.clone());
    assert_eq!(s.store_media(&proof).unwrap().status, i32::from(Status::Ok));
    assert_eq!(
        s.media.as_ref().unwrap().manifest(&manifest.hash).unwrap(),
        None
    );
    assert_eq!(
        s.assemble_media(&manifest, &[], &proto::GetMedia::default())
            .unwrap()
            .data,
        small.data
    );
    (proof, manifest)
}

fn check_whole_copy_replaces_manifest(
    small: &proto::MediaReplicaPut,
    proof: &proto::MediaReplicaPut,
    manifest: &proto::MediaManifest,
) {
    // A manifest already held must not suppress a later verified whole-copy put.
    let second = media_service(|_| {});
    assert_eq!(
        second.store_media(proof).unwrap().status,
        i32::from(Status::Ok)
    );
    assert!(second
        .media
        .as_ref()
        .unwrap()
        .manifest(&manifest.hash)
        .unwrap()
        .is_some());
    assert_eq!(
        second.store_media(small).unwrap().status,
        i32::from(Status::Ok)
    );
    assert_eq!(
        second.media.as_ref().unwrap().get(&manifest.hash).unwrap(),
        Some(small.data.clone())
    );
    assert_eq!(
        second
            .media
            .as_ref()
            .unwrap()
            .manifest(&manifest.hash)
            .unwrap(),
        None
    );
}

fn check_sharded_upload_ratio(s: &mut Service, owner: &Identity) {
    let data = vec![4; (1 << 20) + 1];
    let mut replica = media_replica(owner, data.clone(), 2);
    let (manifest, _) = crate::replication::media_encode(&data).unwrap();
    assert_eq!(manifest.billed_bytes(), Some(3 * data.len() as u64));
    replica.data.clear();
    replica.manifest = Some(manifest);
    assert_eq!(
        s.store_media(&replica).unwrap().status,
        i32::from(Status::Ok)
    );
    s.traffic = Traffic::new(0);
    let put = proto::MediaPut {
        owner: dyapp_identity::key_hash(&owner.public_key()).to_vec(),
        data,
    };
    assert!(s.prepare_media(&peer("uploader"), &put).unwrap().is_ok());
    assert_eq!(
        s.prepare_media(&peer("uploader"), &put).unwrap(),
        Err(Status::RateLimited)
    );
}

#[test]
fn media_keep_put_get() {
    let mut service = service();
    assert_eq!(
        call_media(&service, media_get(&[0; 32])).0,
        Status::Unsupported
    );

    let dir = format!("/tmp/ai/test-service-media-{}", uuid::Uuid::new_v4());
    service.media = Some(MediaStore::open(std::path::Path::new(&dir)).unwrap());
    let identity = Identity::try_generate().unwrap();
    let owner = dyapp_identity::key_hash(&identity.public_key()).to_vec();
    let data = b"photo".to_vec();
    let hash = dyapp_identity::sha256(&data).to_vec();
    let keep = check_keep_clock(&service, &identity, &hash);
    check_keep_put_get(&service, &owner, &data, &hash, &keep);
    check_attachment_release(&service, &identity, &owner, &hash);
    check_denied_media_and_info(&mut service, &hash);
}

fn call_media_as(
    service: &Service,
    name: &str,
    request: media_request::Request,
) -> (Status, MediaResponse) {
    let response = service
        .media(
            &peer(name),
            MediaRequest {
                request: Some(request),
            },
        )
        .unwrap();
    (Status::try_from(response.status).unwrap(), response)
}

fn call_media(service: &Service, request: media_request::Request) -> (Status, MediaResponse) {
    call_media_as(service, "p", request)
}

fn media_get(hash: &[u8]) -> media_request::Request {
    media_request::Request::Get(proto::GetMedia {
        hash: hash.to_vec(),
        ..Default::default()
    })
}

fn media_range(hash: &[u8], offset: u64, len: u64) -> media_request::Request {
    media_request::Request::Get(proto::GetMedia {
        hash: hash.to_vec(),
        offset,
        len,
    })
}

fn media_put(owner: &[u8], data: &[u8]) -> media_request::Request {
    media_request::Request::Put(proto::MediaPut {
        owner: owner.to_vec(),
        data: data.to_vec(),
    })
}

fn media_release(secret: &[u8]) -> media_request::Request {
    media_request::Request::Release(proto::MediaRelease {
        secret: secret.to_vec(),
    })
}

fn check_keep_clock(service: &Service, identity: &Identity, hash: &[u8]) -> SignedRecord {
    let keep = proto::MediaKeep {
        version: 1,
        hashes: vec![hash.to_vec()],
        time: crate::clock::Clock::system().timestamp().unsigned_abs(),
    };
    for time in [keep.time - 3600, keep.time + 3600] {
        let off_clock = proto::MediaKeep {
            time,
            ..keep.clone()
        };
        let signed = identity.sign(Domain::MediaKeep, off_clock.encode_to_vec());
        assert_eq!(
            call_media_as(service, "clock", media_request::Request::Keep(signed)).0,
            Status::Invalid
        );
    }
    identity.sign(Domain::MediaKeep, keep.encode_to_vec())
}

fn check_keep_put_get(
    service: &Service,
    owner: &[u8],
    data: &[u8],
    hash: &[u8],
    keep: &SignedRecord,
) {
    assert_eq!(
        call_media(service, media_put(owner, data)).0,
        Status::NotFound
    );
    let (got, response) = call_media(service, media_request::Request::Keep(keep.clone()));
    assert_eq!((got, response.missing), (Status::Ok, vec![hash.to_vec()]));
    assert_eq!(
        call_media(service, media_request::Request::Keep(keep.clone())).0,
        Status::Stale
    );
    assert_eq!(call_media(service, media_put(owner, data)).0, Status::Ok);
    let (got, response) = call_media(service, media_get(hash));
    assert_eq!((got, response.data), (Status::Ok, data.to_vec()));
    assert_eq!(call_media(service, media_get(&[1; 5])).0, Status::Invalid);
    let (got, response) = call_media_as(service, "range", media_range(hash, 1, 3));
    assert_eq!(
        (got, response.data, response.total_size),
        (Status::Ok, b"hot".to_vec(), 5)
    );
    let (_, response) = call_media_as(service, "range", media_range(hash, 3, 1 << 20));
    assert_eq!(response.data, b"to");
    for (offset, len) in [(5, 1), (1, 0), (0, (1 << 20) + 1)] {
        assert_eq!(
            call_media_as(service, "range", media_range(hash, offset, len)).0,
            Status::Invalid
        );
    }

    let mut forged = keep.clone();
    forged.payload.push(0);
    assert_eq!(
        call_media(service, media_request::Request::Keep(forged)).0,
        Status::Denied
    );
}

fn check_attachment_release(
    service: &Service,
    identity: &Identity,
    owner: &[u8],
    kept_hash: &[u8],
) {
    // An attachment: put after the attach, gone after the release.
    let file = b"attachment".to_vec();
    let file_hash = dyapp_identity::sha256(&file).to_vec();
    let secret = vec![7u8; 32];
    let attach = |created| {
        let attach = proto::MediaAttach {
            release: dyapp_identity::sha256(&secret).to_vec(),
            hashes: vec![file_hash.clone()],
            created,
        };
        media_request::Request::Attach(identity.sign(Domain::MediaAttach, attach.encode_to_vec()))
    };
    let call = |request| call_media_as(service, "q", request).0;
    let now = crate::clock::Clock::system().timestamp().unsigned_abs();
    assert_eq!(call(attach(now + 3600)), Status::Invalid);
    let (got, response) = call_media_as(service, "q", attach(now));
    assert_eq!(
        (got, response.missing),
        (Status::Ok, vec![file_hash.clone()])
    );
    assert_eq!(call(media_put(owner, &file)), Status::Ok);
    assert_eq!(call(media_get(&file_hash)), Status::Ok);
    assert_eq!(call(media_release(&[8; 32])), Status::NotFound);
    assert_eq!(call(media_release(&secret)), Status::Ok);
    assert_eq!(call(media_get(&file_hash)), Status::NotFound);
    assert_eq!(call(attach(now)), Status::Refused);
    assert_eq!(call(media_get(&file_hash)), Status::NotFound);
    assert_eq!(call(media_get(kept_hash)), Status::Ok, "kept blob stays");
}

fn check_denied_media_and_info(service: &mut Service, hash: &[u8]) {
    *service.deny.get_mut().unwrap() = [hex(hash)].into();
    assert_eq!(
        call_media_as(service, "r", media_get(hash)).0,
        Status::Refused,
        "blocked blob"
    );

    let info = service.node(
        &peer("q"),
        NodeRequest {
            request: Some(node_request::Request::Info(proto::InfoRequest {})),
        },
    );
    let info = info.info.unwrap();
    assert_eq!(info.roles.len(), 2);
    assert_eq!(info.max_media_bytes, MAX_MEDIA_BYTES as u64);
}

// `count` hashes, the first naming `data`.
fn media_hashes(data: &[u8], count: usize) -> Vec<Vec<u8>> {
    let mut hashes = vec![dyapp_identity::sha256(data).to_vec()];
    hashes.extend(
        (1..u32::try_from(count).unwrap())
            .map(|i| dyapp_identity::sha256(&i.to_be_bytes()).to_vec()),
    );
    hashes
}

mod conditions;
