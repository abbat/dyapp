use super::*;

#[tokio::test]
async fn signed_time_failures_keep_clock_hints_over_real_tcp() {
    use proto::media_replica_put::Authorization;
    let address = clock_node().await;
    let mut client = Client::connect(&address).await;
    let before = chrono::Utc::now().timestamp().unsigned_abs();
    let response = client.info().await;
    assert_eq!(status(response.status), Status::Ok);
    let server_time = response.info.unwrap().server_time;
    assert!((before..=chrono::Utc::now().timestamp().unsigned_abs()).contains(&server_time));
    let owner = Identity::generate();
    let data = b"wire-clock".to_vec();
    let hash = dyapp_identity::sha256(&data).to_vec();
    let (manifest, shards) = dyapp_bootstrap::replication::media_encode(&data).unwrap();
    for time in [server_time + 3600, u64::MAX] {
        let heartbeat = owner.sign(
            Domain::Heartbeat,
            proto::Heartbeat {
                time,
                version: 1,
                ..Default::default()
            }
            .encode_to_vec(),
        );
        let before = chrono::Utc::now().timestamp().unsigned_abs();
        let response = client
            .profile(profile_request::Request::Heartbeat(heartbeat.clone()))
            .await;
        assert_clock_invalid(response.status, response.server_time, before);
        let response = client
            .search(proto::search_request::Request::Heartbeat(heartbeat))
            .await;
        assert_clock_invalid(response.status, response.server_time, before);
        let keep = owner.sign(
            Domain::MediaKeep,
            proto::MediaKeep {
                version: 1,
                hashes: vec![hash.clone()],
                time,
            }
            .encode_to_vec(),
        );
        let attach = owner.sign(
            Domain::MediaAttach,
            proto::MediaAttach {
                release: dyapp_identity::sha256(b"wire-release").to_vec(),
                hashes: vec![hash.clone()],
                created: time,
            }
            .encode_to_vec(),
        );
        for authorization in [Authorization::Keep(keep), Authorization::Attach(attach)] {
            let direct = match &authorization {
                Authorization::Keep(record) => proto::media_request::Request::Keep(record.clone()),
                Authorization::Attach(record) => {
                    proto::media_request::Request::Attach(record.clone())
                }
            };
            let replica = proto::MediaReplicaPut {
                data: data.clone(),
                authorization: Some(authorization.clone()),
                manifest: None,
            };
            let shard = proto::MediaShardPut {
                replica: Some(proto::MediaReplicaPut {
                    data: vec![],
                    authorization: Some(authorization),
                    manifest: Some(manifest.clone()),
                }),
                index: 0,
                data: shards[0].clone(),
            };
            for request in [
                direct,
                proto::media_request::Request::ReplicaPut(replica),
                proto::media_request::Request::ShardPut(shard),
            ] {
                let before = chrono::Utc::now().timestamp().unsigned_abs();
                let response = client.media(request).await;
                assert_clock_invalid(response.status, response.server_time, before);
            }
        }
    }
    let mut forged = owner.sign(
        Domain::Heartbeat,
        proto::Heartbeat {
            time: u64::MAX,
            version: 1,
            ..Default::default()
        }
        .encode_to_vec(),
    );
    forged.payload.push(0);
    let response = client
        .profile(profile_request::Request::Heartbeat(forged))
        .await;
    assert_eq!(
        (response.status, response.server_time),
        (i32::from(Status::Denied), 0)
    );
    let malformed = owner.sign(
        Domain::MediaKeep,
        proto::MediaKeep {
            version: 0,
            hashes: vec![hash],
            time: u64::MAX,
        }
        .encode_to_vec(),
    );
    let response = client
        .media(proto::media_request::Request::Keep(malformed))
        .await;
    assert_eq!(
        (response.status, response.server_time),
        (i32::from(Status::Invalid), 0)
    );
    assert_eq!(status(client.info().await.status), Status::Ok);
}

#[tokio::test]
async fn future_clock_boundary_preserves_valid_request_statuses_over_tcp() {
    use proto::media_replica_put::Authorization;
    let address = clock_node().await;
    let mut client = Client::connect(&address).await;
    let owner = Identity::generate();
    let response = client
        .profile(profile_request::Request::Publish(
            Profile {
                version: 1,
                age: 28,
                ..Default::default()
            }
            .sign(&owner),
        ))
        .await;
    assert_eq!(status(response.status), Status::Ok);
    let time = chrono::Utc::now().timestamp().unsigned_abs() + 600;
    let heartbeat = owner.sign(
        Domain::Heartbeat,
        proto::Heartbeat {
            time,
            version: 1,
            ..Default::default()
        }
        .encode_to_vec(),
    );
    let response = client
        .profile(profile_request::Request::Heartbeat(heartbeat.clone()))
        .await;
    assert_eq!(
        (response.status, response.server_time),
        (i32::from(Status::Ok), 0)
    );
    let response = client
        .search(proto::search_request::Request::Heartbeat(heartbeat))
        .await;
    assert!(matches!(
        status(response.status),
        Status::Ok | Status::NotFound
    ));
    assert_eq!(response.server_time, 0);
    let keep = owner.sign(
        Domain::MediaKeep,
        proto::MediaKeep {
            version: 1,
            hashes: vec![],
            time: chrono::Utc::now().timestamp().unsigned_abs() + 600,
        }
        .encode_to_vec(),
    );
    let response = client
        .media(proto::media_request::Request::Keep(keep))
        .await;
    assert_eq!(
        (response.status, response.server_time),
        (i32::from(Status::Ok), 0)
    );
    let data = b"clock-boundary".to_vec();
    let hash = dyapp_identity::sha256(&data).to_vec();
    let attach = owner.sign(
        Domain::MediaAttach,
        proto::MediaAttach {
            release: dyapp_identity::sha256(b"boundary-release").to_vec(),
            hashes: vec![hash.clone()],
            created: chrono::Utc::now().timestamp().unsigned_abs() + 600,
        }
        .encode_to_vec(),
    );
    let response = client
        .media(proto::media_request::Request::Attach(attach.clone()))
        .await;
    assert_eq!(
        (response.status, response.server_time),
        (i32::from(Status::Ok), 0)
    );
    let keep = owner.sign(
        Domain::MediaKeep,
        proto::MediaKeep {
            version: 2,
            hashes: vec![hash],
            time: chrono::Utc::now().timestamp().unsigned_abs() + 600,
        }
        .encode_to_vec(),
    );
    for authorization in [
        Authorization::Keep(keep.clone()),
        Authorization::Attach(attach),
    ] {
        let response = client
            .media(proto::media_request::Request::ReplicaPut(
                proto::MediaReplicaPut {
                    data: data.clone(),
                    authorization: Some(authorization),
                    manifest: None,
                },
            ))
            .await;
        assert_eq!(
            (response.status, response.server_time),
            (i32::from(Status::Ok), 0)
        );
    }
    let (manifest, shards) = dyapp_bootstrap::replication::media_encode(&data).unwrap();
    let response = client
        .media(proto::media_request::Request::ShardPut(
            proto::MediaShardPut {
                replica: Some(proto::MediaReplicaPut {
                    data: vec![],
                    authorization: Some(Authorization::Keep(keep)),
                    manifest: Some(manifest),
                }),
                index: 0,
                data: shards[0].clone(),
            },
        ))
        .await;
    assert_eq!(
        (response.status, response.server_time),
        (i32::from(Status::Ok), 0)
    );
}
