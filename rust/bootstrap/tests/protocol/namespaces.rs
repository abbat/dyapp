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
        let client_protocols =
            std::sync::Arc::new(dyapp_p2p_net::Protocols::new(client_name).unwrap());
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
        assert_eq!(wire.take().0, [[0; 2]; 10]);
        assert_eq!(client.wire.take().0, [[0; 2]; 10]);
    }
}
