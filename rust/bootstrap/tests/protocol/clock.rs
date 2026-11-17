use super::*;

#[tokio::test]
async fn signed_time_failures_keep_clock_hints_over_real_tcp() {
    let address = clock_node().await;
    let mut client = Client::connect(&address).await;
    let server_time = clock_server_time(&mut client).await;
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
        assert_future_heartbeat(&mut client, heartbeat).await;
        for authorization in signed_media_authorizations(&owner, &hash, time) {
            assert_media_clock_invalid(&mut client, authorization, &data, &manifest, &shards).await;
        }
    }
    assert_invalid_signed_records(&mut client, &owner, hash).await;
    assert_eq!(status(client.info().await.status), Status::Ok);
}

async fn clock_server_time(client: &mut Client) -> u64 {
    let before = dyapp_bootstrap::clock::Clock::system()
        .timestamp()
        .unsigned_abs();
    let response = client.info().await;
    assert_eq!(status(response.status), Status::Ok);
    let server_time = response.info.unwrap().server_time;
    assert!((before
        ..=dyapp_bootstrap::clock::Clock::system()
            .timestamp()
            .unsigned_abs())
        .contains(&server_time));
    server_time
}

async fn assert_future_heartbeat(client: &mut Client, heartbeat: SignedRecord) {
    let before = dyapp_bootstrap::clock::Clock::system()
        .timestamp()
        .unsigned_abs();
    let response = client
        .profile(profile_request::Request::Heartbeat(heartbeat.clone()))
        .await;
    assert_clock_invalid(response.status, response.server_time, before);
    let response = client
        .search(proto::search_request::Request::Heartbeat(heartbeat))
        .await;
    assert_clock_invalid(response.status, response.server_time, before);
}

fn signed_media_authorizations(
    owner: &Identity,
    hash: &[u8],
    time: u64,
) -> [proto::media_replica_put::Authorization; 2] {
    use proto::media_replica_put::Authorization;
    let keep = owner.sign(
        Domain::MediaKeep,
        proto::MediaKeep {
            version: 1,
            hashes: vec![hash.to_vec()],
            time,
        }
        .encode_to_vec(),
    );
    let attach = owner.sign(
        Domain::MediaAttach,
        proto::MediaAttach {
            release: dyapp_identity::sha256(b"wire-release").to_vec(),
            hashes: vec![hash.to_vec()],
            created: time,
        }
        .encode_to_vec(),
    );
    [Authorization::Keep(keep), Authorization::Attach(attach)]
}

async fn assert_media_clock_invalid(
    client: &mut Client,
    authorization: proto::media_replica_put::Authorization,
    data: &[u8],
    manifest: &proto::MediaManifest,
    shards: &[Vec<u8>],
) {
    use proto::media_replica_put::Authorization;
    let direct = match &authorization {
        Authorization::Keep(record) => proto::media_request::Request::Keep(record.clone()),
        Authorization::Attach(record) => proto::media_request::Request::Attach(record.clone()),
    };
    let replica = proto::MediaReplicaPut {
        data: data.to_vec(),
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
        let before = dyapp_bootstrap::clock::Clock::system()
            .timestamp()
            .unsigned_abs();
        let response = client.media(request).await;
        assert_clock_invalid(response.status, response.server_time, before);
    }
}

async fn assert_invalid_signed_records(client: &mut Client, owner: &Identity, hash: Vec<u8>) {
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
}

#[tokio::test]
async fn future_clock_boundary_preserves_valid_request_statuses_over_tcp() {
    let address = clock_node().await;
    let mut client = Client::connect(&address).await;
    let owner = Identity::generate();
    publish_clock_profile(&mut client, &owner).await;
    let time = dyapp_bootstrap::clock::Clock::system()
        .timestamp()
        .unsigned_abs()
        + 600;
    assert_boundary_heartbeat(&mut client, &owner, time).await;
    assert_boundary_keep(&mut client, &owner).await;
    let data = b"clock-boundary".to_vec();
    let hash = dyapp_identity::sha256(&data).to_vec();
    let attach = assert_boundary_attach(&mut client, &owner, &hash).await;
    let keep = owner.sign(
        Domain::MediaKeep,
        proto::MediaKeep {
            version: 2,
            hashes: vec![hash],
            time: dyapp_bootstrap::clock::Clock::system()
                .timestamp()
                .unsigned_abs()
                + 600,
        }
        .encode_to_vec(),
    );
    assert_boundary_replicas(&mut client, &data, &keep, attach).await;
    assert_boundary_shard(&mut client, &data, keep).await;
}

async fn publish_clock_profile(client: &mut Client, owner: &Identity) {
    let response = client
        .profile(profile_request::Request::Publish(
            Profile {
                version: 1,
                ..Default::default()
            }
            .sign(owner),
        ))
        .await;
    assert_eq!(status(response.status), Status::Ok);
}

async fn assert_boundary_heartbeat(client: &mut Client, owner: &Identity, time: u64) {
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
}

async fn assert_boundary_keep(client: &mut Client, owner: &Identity) {
    let keep = owner.sign(
        Domain::MediaKeep,
        proto::MediaKeep {
            version: 1,
            hashes: vec![],
            time: dyapp_bootstrap::clock::Clock::system()
                .timestamp()
                .unsigned_abs()
                + 600,
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
}

async fn assert_boundary_attach(
    client: &mut Client,
    owner: &Identity,
    hash: &[u8],
) -> SignedRecord {
    let attach = owner.sign(
        Domain::MediaAttach,
        proto::MediaAttach {
            release: dyapp_identity::sha256(b"boundary-release").to_vec(),
            hashes: vec![hash.to_vec()],
            created: dyapp_bootstrap::clock::Clock::system()
                .timestamp()
                .unsigned_abs()
                + 600,
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
    attach
}

async fn assert_boundary_replicas(
    client: &mut Client,
    data: &[u8],
    keep: &SignedRecord,
    attach: SignedRecord,
) {
    use proto::media_replica_put::Authorization;
    for authorization in [
        Authorization::Keep(keep.clone()),
        Authorization::Attach(attach),
    ] {
        let response = client
            .media(proto::media_request::Request::ReplicaPut(
                proto::MediaReplicaPut {
                    data: data.to_vec(),
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
}

async fn assert_boundary_shard(client: &mut Client, data: &[u8], keep: SignedRecord) {
    use proto::media_replica_put::Authorization;
    let (manifest, shards) = dyapp_bootstrap::replication::media_encode(data).unwrap();
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
