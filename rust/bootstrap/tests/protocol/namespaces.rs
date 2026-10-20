use super::*;

#[tokio::test]
async fn different_namespaces_refuse_rpc_and_gossip_negotiation_over_tcp() {
    fn unsupported<Req, Resp>(event: &Event<Req, Resp>) -> bool {
        match event {
            Event::OutboundFailure { error, .. } => {
                assert!(
                    matches!(
                        error,
                        request_response::OutboundFailure::UnsupportedProtocols
                    ),
                    "{error:?}"
                );
                true
            }
            Event::Message { .. } => panic!("RPC negotiated across namespaces"),
            _ => false,
        }
    }
    for (server_name, client_name) in [
        ("isolated-a", ""),
        ("", "isolated-a"),
        ("isolated-a", "isolated-b"),
    ] {
        let server_protocols =
            std::sync::Arc::new(dyapp_p2p_net::Protocols::new(server_name).unwrap());
        // The server's identify version keeps the node from dropping the client, so only the
        // stream IDs differ.
        let client_protocols = std::sync::Arc::new(dyapp_p2p_net::Protocols {
            version: server_protocols.version.clone(),
            ..dyapp_p2p_net::Protocols::new(client_name).unwrap()
        });
        let (address, wire) = start_config_in_protocols(
            |c| {
                c.roles = vec![Role::Store, Role::Media, Role::Search];
                c.limits.requests_per_second = 1000;
                c.limits.media_requests_per_second = 100;
                c.limits.bytes_per_second = 0;
                c.limits.min_free_mb = 0;
            },
            server_protocols,
        )
        .await;
        let mut client = Client::connect_in_network(
            &address,
            Keypair::generate_ed25519(),
            true,
            client_protocols,
        )
        .await;
        let id = client.server;
        let behaviour = client.swarm.behaviour_mut();
        behaviour.node.send_request(
            &id,
            proto::NodeRequest {
                request: Some(node_request::Request::Info(proto::InfoRequest {})),
            },
        );
        behaviour.profile.send_request(
            &id,
            proto::ProfileRequest {
                request: Some(profile_request::Request::Get(proto::GetProfile {
                    peer_id: vec![0; 32],
                })),
            },
        );
        behaviour.mailbox.send_request(
            &id,
            proto::MailboxRequest {
                request: Some(mailbox_request::Request::Challenge(
                    proto::ChallengeRequest {},
                )),
            },
        );
        behaviour.media.send_request(
            &id,
            proto::MediaRequest {
                request: Some(proto::media_request::Request::Get(proto::GetMedia {
                    hash: vec![0; 32],
                })),
            },
        );
        behaviour.search.send_request(
            &id,
            proto::SearchRequest {
                request: Some(proto::search_request::Request::Get(proto::SearchGet {
                    keys: vec![],
                    puzzle: None,
                })),
            },
        );
        behaviour.inventory.send_request(
            &id,
            proto::InventoryRequest {
                topic: 0,
                after: vec![],
            },
        );
        let mut failed = std::collections::HashSet::new();
        tokio::time::timeout(Duration::from_secs(10), async {
            while failed.len() < 7 {
                if let SwarmEvent::Behaviour(event) = client.swarm.select_next_some().await {
                    match event {
                        BehaviourEvent::Node(event) if unsupported(&event) => {
                            failed.insert("node");
                        }
                        BehaviourEvent::Profile(event) if unsupported(&event) => {
                            failed.insert("profile");
                        }
                        BehaviourEvent::Mailbox(event) if unsupported(&event) => {
                            failed.insert("mailbox");
                        }
                        BehaviourEvent::Media(event) if unsupported(&event) => {
                            failed.insert("media");
                        }
                        BehaviourEvent::Search(event) if unsupported(&event) => {
                            failed.insert("search");
                        }
                        BehaviourEvent::Inventory(event) if unsupported(&event) => {
                            failed.insert("inventory");
                        }
                        BehaviourEvent::Gossip(
                            libp2p::gossipsub::Event::GossipsubNotSupported { peer_id },
                        ) => {
                            assert_eq!(peer_id, id);
                            failed.insert("gossip");
                        }
                        BehaviourEvent::Gossip(
                            libp2p::gossipsub::Event::Subscribed { .. }
                            | libp2p::gossipsub::Event::Message { .. },
                        ) => panic!("gossip negotiated across namespaces"),
                        _ => {}
                    }
                }
            }
        })
        .await
        .unwrap();
        assert!(client.swarm.is_connected(&id)); // Common transport is separate from application isolation.
        assert_eq!(wire.take().0, [[0; 2]; 11]);
        assert_eq!(client.wire.take().0, [[0; 2]; 11]);
    }
}

#[tokio::test]
async fn peers_of_another_network_are_dropped_after_identify() {
    let city = std::sync::Arc::new(dyapp_p2p_net::Protocols::new("city").unwrap());
    let (address, _) = start_config_in_protocols(|c| c.network.name = "city".into(), city).await;
    let mut client = Client::connect(&address).await; // the public network
    let id = client.server;
    client.swarm.behaviour_mut().node.send_request(
        &id,
        proto::NodeRequest {
            request: Some(node_request::Request::Info(proto::InfoRequest {})),
        },
    );
    // The request fails on the IDs; only the identify check closes the connection: the idle
    // timeout is 60 s.
    let (mut failed, mut closed) = (false, false);
    tokio::time::timeout(Duration::from_secs(10), async {
        while !(failed && closed) {
            match client.swarm.select_next_some().await {
                SwarmEvent::Behaviour(BehaviourEvent::Node(Event::OutboundFailure { .. })) => {
                    failed = true;
                }
                SwarmEvent::Behaviour(BehaviourEvent::Node(Event::Message { .. })) => {
                    panic!("RPC negotiated across networks")
                }
                SwarmEvent::ConnectionClosed {
                    num_established: 0, ..
                } => closed = true,
                _ => {}
            }
        }
    })
    .await
    .expect("not disconnected");
}

#[tokio::test]
async fn named_node_serves_profile_search_and_configured_gossip_topics() {
    let protocols = std::sync::Arc::new(dyapp_p2p_net::Protocols::new("node-test").unwrap());
    let (address, wire) = start_config_in_protocols(
        |c| {
            c.roles = vec![Role::Store, Role::Search];
            c.search.min_profile_pow_bits = 0;
            c.limits.requests_per_second = 1000;
            c.limits.bytes_per_second = 0;
            c.limits.min_free_mb = 0;
        },
        protocols.clone(),
    )
    .await;
    let mut client = Client::connect_in_network(
        &address,
        Keypair::generate_ed25519(),
        true,
        protocols.clone(),
    )
    .await;
    let owner = Identity::generate();
    let key = dyapp_identity::key_hash(&owner.public_key());
    let topic = protocols
        .search_topic(dyapp_bootstrap::search::topic(&key))
        .hash();
    client
        .wait(|event| match event {
            BehaviourEvent::Gossip(libp2p::gossipsub::Event::Subscribed {
                topic: subscribed,
                ..
            }) if subscribed == topic => Some(()),
            _ => None,
        })
        .await;
    assert_eq!(status(client.info().await.status), Status::Ok);
    let record = Profile {
        version: 1,
        age: 28,
        ..Default::default()
    }
    .sign(&owner);
    assert_eq!(
        status(
            client
                .profile(profile_request::Request::Publish(record.clone()))
                .await
                .status
        ),
        Status::Ok
    );
    let request = proto::SearchRequest {
        request: Some(proto::search_request::Request::Publish(record)),
    };
    let bytes = request.encode_to_vec();
    client
        .swarm
        .behaviour_mut()
        .search
        .send_request(&client.server, request);
    let (mut replied, mut gossiped) = (false, false);
    tokio::time::timeout(Duration::from_secs(10), async {
        while !(replied && gossiped) {
            if let SwarmEvent::Behaviour(event) = client.swarm.select_next_some().await {
                match event {
                    BehaviourEvent::Search(event) => {
                        if let Some(response) = response(event) {
                            assert_eq!(status(response.status), Status::Ok);
                            replied = true;
                        }
                    }
                    BehaviourEvent::Gossip(libp2p::gossipsub::Event::Message {
                        message, ..
                    }) => {
                        assert_eq!(message.topic, topic);
                        assert_eq!(message.data, bytes);
                        gossiped = true;
                    }
                    _ => {}
                }
            }
        }
    })
    .await
    .unwrap();
    let counts = wire.take();
    assert_eq!(counts.0[dyapp_p2p_net::WireBytes::OTHER], [0, 0]);
    assert_eq!(
        counts.0[dyapp_p2p_net::WireBytes::GOSSIP],
        [0, bytes.len() as u64]
    );
    assert!(
        counts.0[dyapp_p2p_net::WireBytes::protocol_slot_for(&protocols.search, &protocols)][0] > 0
    );
}
