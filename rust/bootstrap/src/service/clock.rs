use super::tests::*;
use super::*;
use dyapp_identity::Identity;

fn assert_server_clock(server_time: u64, before: u64) {
    let after = chrono::Utc::now().timestamp().unsigned_abs();
    assert!(
        (before..=after).contains(&server_time),
        "{before} <= {server_time} <= {after}"
    );
}

#[test]
fn info_and_heartbeat_time_failures_report_clock_without_changing_gossip() {
    let s = service();
    let search = search_service();
    let owner = Identity::generate();
    let now = chrono::Utc::now().timestamp().unsigned_abs();
    let info = s.node(
        &peer("clock"),
        NodeRequest {
            request: Some(node_request::Request::Info(proto::InfoRequest {})),
        },
    );
    assert_eq!(info.status, i32::from(Status::Ok));
    assert_server_clock(info.info.unwrap().server_time, now);
    for time in [0, now + 3600, u64::MAX] {
        let record = owner.sign(
            Domain::Heartbeat,
            proto::Heartbeat {
                time,
                version: 1,
                ..Default::default()
            }
            .encode_to_vec(),
        );
        let before = chrono::Utc::now().timestamp().unsigned_abs();
        let response = s
            .profile(
                &peer("clock"),
                ProfileRequest {
                    request: Some(profile_request::Request::Heartbeat(record.clone())),
                    age_seconds: 0,
                },
            )
            .unwrap();
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
    let record = owner.sign(
        Domain::Heartbeat,
        proto::Heartbeat {
            time: chrono::Utc::now().timestamp().unsigned_abs(),
            version: 1,
            ..Default::default()
        }
        .encode_to_vec(),
    );
    let response = s
        .profile(
            &peer("clock"),
            ProfileRequest {
                request: Some(profile_request::Request::Heartbeat(record.clone())),
                age_seconds: 0,
            },
        )
        .unwrap();
    assert_eq!(
        (response.status, response.server_time),
        (i32::from(Status::NotFound), 0)
    );
    let response = search_as(
        &search,
        &peer("clock"),
        search_request::Request::Heartbeat(record.clone()),
    );
    assert_eq!(
        (response.status, response.server_time),
        (i32::from(Status::NotFound), 0)
    );
    let mut forged = record;
    forged.payload.push(0);
    let response = s
        .profile(
            &peer("clock"),
            ProfileRequest {
                request: Some(profile_request::Request::Heartbeat(forged.clone())),
                age_seconds: 0,
            },
        )
        .unwrap();
    assert_eq!(
        (response.status, response.server_time),
        (i32::from(Status::Denied), 0)
    );
    let response = search_as(
        &search,
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

#[test]
fn media_time_hints_cover_direct_replica_and_shard_authorizations() {
    use proto::media_replica_put::Authorization;
    let s = media_service(|l| l.media_requests_per_second = 100);
    let from = replica_peer("clock");
    let owner = Identity::generate();
    let data = b"clock".to_vec();
    let hash = dyapp_identity::sha256(&data).to_vec();
    let (manifest, shards) = crate::replication::media_encode(&data).unwrap();
    let now = chrono::Utc::now().timestamp().unsigned_abs();
    for (keep, time) in [
        (true, 0),
        (true, now + 3600),
        (true, u64::MAX),
        (false, 0),
        (false, now + 3600),
        (false, u64::MAX),
    ] {
        let authorization = if keep {
            Authorization::Keep(
                owner.sign(
                    Domain::MediaKeep,
                    proto::MediaKeep {
                        version: 1,
                        hashes: vec![hash.clone()],
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
                        hashes: vec![hash.clone()],
                        created: time,
                    }
                    .encode_to_vec(),
                ),
            )
        };
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
            let before = chrono::Utc::now().timestamp().unsigned_abs();
            let response = s
                .media(
                    &from,
                    MediaRequest {
                        request: Some(request),
                    },
                )
                .unwrap();
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
    }
}

#[test]
fn media_non_clock_errors_and_old_replica_keeps_preserve_existing_rules() {
    let s = media_service(|l| l.media_requests_per_second = 100);
    let from = replica_peer("clock-errors");
    let owner = Identity::generate();
    let data = b"old-keep".to_vec();
    let now = chrono::Utc::now().timestamp().unsigned_abs();
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
    let response = s
        .media(
            &from,
            MediaRequest {
                request: Some(media_request::Request::Keep(record)),
            },
        )
        .unwrap();
    assert_eq!(response.status, i32::from(Status::Invalid));
    assert_server_clock(response.server_time, now);
    keep.version = 0;
    keep.time = u64::MAX;
    let record = owner.sign(Domain::MediaKeep, keep.encode_to_vec());
    let response = s
        .media(
            &from,
            MediaRequest {
                request: Some(media_request::Request::Keep(record.clone())),
            },
        )
        .unwrap();
    assert_eq!(
        (response.status, response.server_time),
        (i32::from(Status::Invalid), 0)
    );
    let malformed = proto::MediaReplicaPut {
        authorization: Some(proto::media_replica_put::Authorization::Keep(
            record.clone(),
        )),
        ..replica
    };
    let response = s.store_media(&malformed).unwrap();
    assert_eq!(
        (response.status, response.server_time),
        (i32::from(Status::Invalid), 0)
    );
    let mut forged = record;
    forged.payload.push(0);
    let response = s
        .media(
            &from,
            MediaRequest {
                request: Some(media_request::Request::Keep(forged)),
            },
        )
        .unwrap();
    assert_eq!(
        (response.status, response.server_time),
        (i32::from(Status::Denied), 0)
    );
}
