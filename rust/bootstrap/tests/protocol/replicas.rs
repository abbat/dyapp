use super::*;

enum ReplicaRequest {
    Profile(SignedRecord),
    Mailbox(SignedRecord),
    Media(proto::media_request::Request),
}

impl ReplicaRequest {
    async fn send(&self, client: &mut Client) -> Status {
        let code = match self {
            Self::Profile(record) => {
                client
                    .profile(profile_request::Request::ReplicaPut(record.clone()))
                    .await
                    .status
            }
            Self::Mailbox(record) => {
                client
                    .mailbox(mailbox_request::Request::ReplicaPut(proto::Envelopes {
                        envelopes: vec![record.clone()],
                        ..Default::default()
                    }))
                    .await
                    .status
            }
            Self::Media(request) => client.media(request.clone()).await.status,
        };
        status(code)
    }
}

fn replica_requests() -> [ReplicaRequest; 4] {
    let owner = Identity::generate();
    let (manifest, shards) =
        dyapp_bootstrap::replication::media_encode(&vec![1; (1 << 20) + 1]).unwrap();
    let data = vec![3];
    let keep = proto::MediaKeep {
        version: 1,
        hashes: vec![
            dyapp_identity::sha256(&data).to_vec(),
            manifest.hash.clone(),
        ],
        time: chrono::Utc::now().timestamp().unsigned_abs(),
    };
    let authorization = Some(proto::media_replica_put::Authorization::Keep(
        owner.sign(Domain::MediaKeep, keep.encode_to_vec()),
    ));
    [
        ReplicaRequest::Profile(
            Profile {
                version: 1,
                ..Default::default()
            }
            .sign(&owner),
        ),
        ReplicaRequest::Mailbox(
            owner.sign(
                Domain::Envelope,
                proto::Envelope {
                    id: vec![7; 16],
                    mailbox: vec![8; 32],
                    ciphertext: vec![9],
                    ..Default::default()
                }
                .encode_to_vec(),
            ),
        ),
        ReplicaRequest::Media(proto::media_request::Request::ReplicaPut(
            proto::MediaReplicaPut {
                data,
                authorization: authorization.clone(),
                ..Default::default()
            },
        )),
        ReplicaRequest::Media(proto::media_request::Request::ShardPut(
            proto::MediaShardPut {
                replica: Some(proto::MediaReplicaPut {
                    manifest: Some(manifest),
                    authorization,
                    ..Default::default()
                }),
                data: shards[0].clone(),
                index: 0,
            },
        )),
    ]
}

#[tokio::test]
async fn replica_writes_check_transport_pow_before_accepting_the_record() {
    let addr = start_config(|config| {
        config.network.id_pow_bits = 8;
        config.roles = vec![Role::Store, Role::Media];
    })
    .await;
    let weak = loop {
        let key = Keypair::generate_ed25519();
        if !dyapp_p2p_net::id_has_pow(&key.public().to_peer_id(), 8) {
            break key;
        }
    };
    let requests = replica_requests();
    let mut client = Client::connect_with_key(&addr, weak).await;
    for (i, request) in requests.iter().enumerate() {
        assert_eq!(
            request.send(&mut client).await,
            Status::Denied,
            "replica {i}"
        );
    }
    let key = dyapp_p2p_net::generate_pow_keypair(8);
    let mut node = Client::connect_with_key(&addr, key).await;
    for (i, request) in requests.iter().enumerate() {
        assert_eq!(request.send(&mut node).await, Status::Ok, "replica {i}");
    }
}

#[tokio::test]
async fn replica_peer_and_group_refusals_keep_the_connection_open() {
    let key = dyapp_p2p_net::generate_pow_keypair(8);
    let requests = replica_requests();
    for group_bucket in [false, true] {
        for (i, request) in requests.iter().enumerate() {
            let addr = start_config(|config| {
                config.network.id_pow_bits = 8;
                config.roles = vec![Role::Store, Role::Media];
                config.limits.strikes_to_ban = 1;
                if group_bucket {
                    config.limits.ip_group_requests_per_second = 1;
                } else {
                    config.limits.requests_per_second = 1;
                }
            })
            .await;
            let mut client = Client::connect_with_key(&addr, key.clone()).await;
            assert_eq!(request.send(&mut client).await, Status::Ok, "replica {i}");
            assert_eq!(
                request.send(&mut client).await,
                Status::RateLimited,
                "replica {i}, group {group_bucket}"
            );
            // One strike would ban this connection; info must still answer on the same one,
            // from the IP-group bucket it shares.
            let info = if group_bucket {
                Status::RateLimited
            } else {
                Status::Ok
            };
            assert_eq!(status(client.info().await.status), info);
        }
    }
}
