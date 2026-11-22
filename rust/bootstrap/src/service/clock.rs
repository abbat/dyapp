use super::tests::*;
use super::*;
use dyapp_identity::Identity;

fn assert_server_clock(server_time: u64, before: u64) {
    let after = crate::clock::Clock::system().timestamp().unsigned_abs();
    assert!(
        (before..=after).contains(&server_time),
        "{before} <= {server_time} <= {after}"
    );
}

fn signed_heartbeat(owner: &Identity, time: u64) -> SignedRecord {
    owner.sign(
        Domain::Heartbeat,
        proto::Heartbeat {
            time,
            version: 1,
            ..Default::default()
        }
        .encode_to_vec(),
    )
}

fn profile_heartbeat(service: &Service, record: SignedRecord) -> ProfileResponse {
    service
        .profile(
            &peer("clock"),
            ProfileRequest {
                request: Some(profile_request::Request::Heartbeat(record)),
                age_seconds: 0,
            },
        )
        .unwrap()
}

#[test]
fn info_and_heartbeat_time_failures_report_clock_without_changing_gossip() {
    let (_s_test_dir, s) = service();
    let (_search_test_dir, search) = search_service();
    let owner = Identity::try_generate().unwrap();
    let now = crate::clock::Clock::system().timestamp().unsigned_abs();
    let info = s.node(
        &peer("clock"),
        NodeRequest {
            request: Some(node_request::Request::Info(proto::InfoRequest {})),
        },
    );
    assert_eq!(info.status, i32::from(Status::Ok));
    assert_server_clock(info.info.unwrap().server_time, now);
    for time in [0, now + 3600, u64::MAX] {
        let record = signed_heartbeat(&owner, time);
        let before = crate::clock::Clock::system().timestamp().unsigned_abs();
        let response = profile_heartbeat(&s, record.clone());
        assert_eq!(response.status, i32::from(Status::Invalid));
        assert_server_clock(response.server_time, before);
        let response = search_as(
            &search,
            &peer("clock"),
            search_request::Request::Heartbeat(record.clone()),
        );
        assert_eq!(response.status, i32::from(Status::Invalid));
        assert_server_clock(response.server_time, before);
        let gossip = SearchRequest {
            request: Some(search_request::Request::Heartbeat(record)),
        };
        assert_eq!(
            search.search_gossip(&gossip.encode_to_vec()),
            Verdict::Ignore
        );
    }
    check_current_and_forged_heartbeats(&s, &search, &owner);
}

fn check_current_and_forged_heartbeats(s: &Service, search: &Service, owner: &Identity) {
    let record = signed_heartbeat(
        owner,
        crate::clock::Clock::system().timestamp().unsigned_abs(),
    );
    let response = profile_heartbeat(s, record.clone());
    assert_eq!(
        (response.status, response.server_time),
        (i32::from(Status::NotFound), 0)
    );
    let response = search_as(
        search,
        &peer("clock"),
        search_request::Request::Heartbeat(record.clone()),
    );
    assert_eq!(
        (response.status, response.server_time),
        (i32::from(Status::NotFound), 0)
    );
    let mut forged = record;
    forged.payload.push(0);
    let response = profile_heartbeat(s, forged.clone());
    assert_eq!(
        (response.status, response.server_time),
        (i32::from(Status::Denied), 0)
    );
    let response = search_as(
        search,
        &peer("clock"),
        search_request::Request::Heartbeat(forged),
    );
    assert_eq!(
        (response.status, response.server_time),
        (i32::from(Status::Denied), 0)
    );
    let response = s.profile(&peer("clock"), get(vec![])).unwrap();
    assert_eq!(
        (response.status, response.server_time),
        (i32::from(Status::Invalid), 0)
    );
}

fn send_media(service: &Service, from: &Peer, request: media_request::Request) -> MediaResponse {
    service
        .media(
            from,
            MediaRequest {
                request: Some(request),
            },
        )
        .unwrap()
}

fn media_authorization(
    owner: &Identity,
    hash: &[u8],
    keep: bool,
    time: u64,
) -> proto::media_replica_put::Authorization {
    use proto::media_replica_put::Authorization;
    if keep {
        Authorization::Keep(
            owner.sign(
                Domain::MediaKeep,
                proto::MediaKeep {
                    version: 1,
                    hashes: vec![hash.to_vec()],
                    time,
                }
                .encode_to_vec(),
            ),
        )
    } else {
        Authorization::Attach(
            owner.sign(
                Domain::MediaAttach,
                proto::MediaAttach {
                    release: dyapp_identity::sha256(b"clock-release").to_vec(),
                    hashes: vec![hash.to_vec()],
                    created: time,
                }
                .encode_to_vec(),
            ),
        )
    }
}

fn assert_media_time_response(
    service: &Service,
    from: &Peer,
    request: media_request::Request,
    keep: bool,
    time: u64,
    path: usize,
) {
    let before = crate::clock::Clock::system().timestamp().unsigned_abs();
    let response = send_media(service, from, request);
    if !keep && time == 0 && path == 0 {
        // Direct attach has no past-freshness gate; expiry is cleaned up later.
        assert_eq!(
            (response.status, response.server_time),
            (i32::from(Status::Ok), 0)
        );
    } else {
        assert_eq!(response.status, i32::from(Status::Invalid));
        assert_server_clock(response.server_time, before);
    }
}

#[test]
fn media_time_hints_cover_direct_replica_and_shard_authorizations() {
    use proto::media_replica_put::Authorization;
    let (_s_test_dir, s) = media_service(|l| l.media_requests_per_second = 100);
    let from = replica_peer("clock");
    let owner = Identity::try_generate().unwrap();
    let data = b"clock".to_vec();
    let hash = dyapp_identity::sha256(&data).to_vec();
    let (manifest, shards) = crate::replication::media_encode(&data).unwrap();
    let now = crate::clock::Clock::system().timestamp().unsigned_abs();
    for (keep, time) in [
        (true, 0),
        (true, now + 3600),
        (true, u64::MAX),
        (false, 0),
        (false, now + 3600),
        (false, u64::MAX),
    ] {
        let authorization = media_authorization(&owner, &hash, keep, time);
        let direct = match &authorization {
            Authorization::Keep(record) => media_request::Request::Keep(record.clone()),
            Authorization::Attach(record) => media_request::Request::Attach(record.clone()),
        };
        let replica = proto::MediaReplicaPut {
            data: data.clone(),
            authorization: Some(authorization.clone()),
            manifest: None,
        };
        let shard = proto::MediaShardPut {
            replica: Some(proto::MediaReplicaPut {
                manifest: Some(manifest.clone()),
                authorization: Some(authorization),
                data: vec![],
            }),
            index: 0,
            data: shards[0].clone(),
        };
        for (path, request) in [
            direct,
            media_request::Request::ReplicaPut(replica),
            media_request::Request::ShardPut(shard),
        ]
        .into_iter()
        .enumerate()
        {
            assert_media_time_response(&s, &from, request, keep, time, path);
        }
    }
}

#[test]
fn media_non_clock_errors_and_old_replica_keeps_preserve_existing_rules() {
    let (_s_test_dir, s) = media_service(|l| l.media_requests_per_second = 100);
    let from = replica_peer("clock-errors");
    let owner = Identity::try_generate().unwrap();
    let data = b"old-keep".to_vec();
    let now = crate::clock::Clock::system().timestamp().unsigned_abs();
    let mut keep = proto::MediaKeep {
        version: 1,
        hashes: vec![dyapp_identity::sha256(&data).to_vec()],
        time: now - 3600,
    };
    let record = owner.sign(Domain::MediaKeep, keep.encode_to_vec());
    let replica = proto::MediaReplicaPut {
        data,
        authorization: Some(proto::media_replica_put::Authorization::Keep(
            record.clone(),
        )),
        manifest: None,
    };
    let response = s.store_media(&replica).unwrap();
    assert_eq!(
        (response.status, response.server_time),
        (i32::from(Status::Ok), 0)
    );
    let response = send_media(&s, &from, media_request::Request::Keep(record));
    assert_eq!(response.status, i32::from(Status::Invalid));
    assert_server_clock(response.server_time, now);
    keep.version = 0;
    keep.time = u64::MAX;
    let record = owner.sign(Domain::MediaKeep, keep.encode_to_vec());
    let response = send_media(&s, &from, media_request::Request::Keep(record.clone()));
    assert_eq!(
        (response.status, response.server_time),
        (i32::from(Status::Invalid), 0)
    );
    check_malformed_and_forged_keep(&s, &from, replica, record);
}

fn check_malformed_and_forged_keep(
    service: &Service,
    from: &Peer,
    replica: proto::MediaReplicaPut,
    record: SignedRecord,
) {
    let malformed = proto::MediaReplicaPut {
        authorization: Some(proto::media_replica_put::Authorization::Keep(
            record.clone(),
        )),
        ..replica
    };
    let response = service.store_media(&malformed).unwrap();
    assert_eq!(
        (response.status, response.server_time),
        (i32::from(Status::Invalid), 0)
    );
    let mut forged = record;
    forged.payload.push(0);
    let response = send_media(service, from, media_request::Request::Keep(forged));
    assert_eq!(
        (response.status, response.server_time),
        (i32::from(Status::Denied), 0)
    );
}
