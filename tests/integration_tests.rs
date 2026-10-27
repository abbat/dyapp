//! Integration tests for dyapp
//! Tests multi-peer scenarios: logical time, video, replication, rate limits and relaying

#[tokio::test]
async fn test_video_session_lifecycle() {
    use dyapp_video::{SessionState, VideoSession};

    let session = VideoSession::new().await.expect("Failed to create session");
    assert_eq!(session.state().await, SessionState::Idle);

    let session_id = session.session_id();
    assert!(!session_id.is_empty());

    // Offer creation
    let _offer = session.create_offer().await.unwrap();
    assert_eq!(session.state().await, SessionState::OfferCreated);

    // Mark offer sent
    session.mark_offer_sent().await.unwrap();
    assert_eq!(session.state().await, SessionState::OfferSent);

    // Close session
    session.close().await.unwrap();
    assert_eq!(session.state().await, SessionState::Closed);
}

#[tokio::test]
async fn test_hybrid_clock_ordering() {
    use dyapp_messaging::hlc::{HybridClock, MessageOrdering};

    let alice_clock = HybridClock::new();
    let bob_clock = HybridClock::new();

    // Alice sends msg1, Bob receives it and replies, Alice receives the reply
    let t1 = alice_clock.now();
    let t2 = {
        bob_clock.observe(t1).unwrap();
        bob_clock.now()
    };
    alice_clock.observe(t2).unwrap();
    let t3 = alice_clock.now();

    // Causality preserved: t1 < t2 < t3
    assert!(t1 < t2 && t2 < t3);

    // Every peer orders the same messages the same way, ties broken by peer ID
    let mut ordering = MessageOrdering::new();
    ordering.add(t3, "alice".into(), "msg3".into());
    ordering.add(t2, "bob".into(), "msg2".into());
    ordering.add(t1, "alice".into(), "msg1".into());
    ordering.add(t1, "aaron".into(), "concurrent".into());
    assert_eq!(
        ordering.get_ordered(),
        vec!["concurrent", "msg1", "msg2", "msg3"]
    );
}

#[tokio::test]
async fn test_ice_candidate_priority() {
    use dyapp_video::ice::ICECandidate;

    let host = ICECandidate::new("candidate:1 typ host".to_string(), None, None);
    let srflx = ICECandidate::new("candidate:2 typ srflx".to_string(), None, None);
    let relay = ICECandidate::new("candidate:3 typ relay".to_string(), None, None);

    assert!(host.priority() > srflx.priority());
    assert!(srflx.priority() > relay.priority());
}

#[tokio::test]
async fn test_codec_negotiation() {
    use dyapp_video::codec::{CodecFormat, CodecInfo, CodecNegotiation};

    let local = vec![CodecInfo::vp8(), CodecInfo::h264()];
    let mut negotiation = CodecNegotiation::new(local);

    let remote = vec![CodecInfo::h264(), CodecInfo::av1()];
    negotiation.set_remote_codecs(remote);

    assert!(negotiation.has_agreed());
    assert_eq!(
        negotiation.agreed_codec().unwrap().format,
        CodecFormat::H264
    );
}

#[tokio::test]
async fn test_replication_fault_tolerance() {
    use dyapp_bootstrap::replication::Replication;

    let rep = Replication::new(2, 1).expect("Failed to create replication");

    let data = b"Important message for Bob";
    let fragments = rep.encode(data).expect("Failed to encode");
    assert_eq!(fragments.len(), 3); // 2 data + 1 parity

    // Can decode with 2 of 3 shards
    let decoded = rep
        .decode(&fragments[..2], data.len())
        .expect("Failed to decode");
    assert_eq!(&decoded, data);

    // Can also decode with different 2 of 3
    let decoded2 = rep
        .decode(&[fragments[0].clone(), fragments[2].clone()], data.len())
        .expect("Failed to decode");
    assert_eq!(&decoded2, data);
}

#[tokio::test]
async fn test_rate_limiting() {
    use dyapp_bootstrap::rate_limit::PeerRateLimiter;

    let limiter = PeerRateLimiter::new(100);

    // Peer within limit
    assert!(limiter.check_limit("alice"));

    // Different peer has fresh limit
    assert!(limiter.check_limit("bob"));

    assert_eq!(limiter.get_active_peers(), 2);
}

#[tokio::test]
async fn test_nated_peers_exchange_a_message_through_a_dyappd_relay() {
    use dyapp_bootstrap::config::Role;
    use dyapp_bootstrap::{node, NodeConfig};
    use dyapp_p2p_net::{proto, BehaviourEvent, Mode};
    use libp2p::futures::StreamExt;
    use libp2p::multiaddr::Protocol;
    use libp2p::swarm::dial_opts::{DialOpts, PeerCondition};
    use libp2p::swarm::SwarmEvent;
    use libp2p::{identity::Keypair, kad, request_response, Multiaddr};
    use std::time::Duration;

    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let mut config = NodeConfig::default();
    config.storage.dir = format!("/tmp/ai/test-relay-{}", uuid::Uuid::new_v4()).into();
    std::fs::create_dir_all(&config.storage.dir).unwrap();
    config.network.id_pow_bits = 0;
    config.roles = vec![Role::Store, Role::Relay];
    config.listen = vec![format!("127.0.0.1:{port}")];
    // The relay announces this address in reservations.
    config.external = config.listen.clone();
    config.resolve_secrets().unwrap();
    config.validate().unwrap();
    let relay_id = config.keygen(None).unwrap();
    let running = tokio::spawn(node::open(config).await.unwrap().run());
    let relay: Multiaddr = format!("/ip4/127.0.0.1/tcp/{port}/p2p/{relay_id}")
        .parse()
        .unwrap();

    // Neither peer listens directly: each reserves a slot on the relay. A client serves the node
    // protocol outbound only, so the answering peer runs in auto mode.
    let peer = |mode| {
        let mut swarm = dyapp_p2p_net::build_swarm(Keypair::generate_ed25519(), mode).unwrap();
        swarm
            .listen_on(relay.clone().with(Protocol::P2pCircuit))
            .unwrap();
        swarm
    };
    let (mut a, mut b) = (peer(Mode::Auto), peer(Mode::Client));
    let a_id = *a.local_peer_id();
    b.behaviour_mut().kad.add_address(&relay_id, relay.clone());
    let (mut reserved, mut relayed, mut answered) = ([false; 2], false, false);
    let mut retry = tokio::time::interval(Duration::from_millis(200));
    tokio::time::timeout(Duration::from_secs(30), async {
        while !answered {
            tokio::select! {
                _ = retry.tick(), if reserved == [true; 2] => {
                    // B knows only A's peer ID: the dial adds a circuit through each connected
                    // relay once identify has shown the relay's hop protocol.
                    let opts = DialOpts::peer_id(a_id)
                        .condition(PeerCondition::DisconnectedAndNotDialing)
                        .build();
                    let _ = b.dial(opts);
                }
                event = a.select_next_some() => match event {
                    SwarmEvent::NewListenAddr { .. } => reserved[0] = true,
                    SwarmEvent::Behaviour(BehaviourEvent::Node(request_response::Event::Message {
                        message: request_response::Message::Request { channel, .. }, ..
                    })) => {
                        a.behaviour_mut().node.send_response(channel, proto::NodeResponse {
                            status: proto::Status::Unsupported.into(), ..Default::default()
                        }).unwrap();
                    }
                    _ => {}
                },
                event = b.select_next_some() => match event {
                    SwarmEvent::NewListenAddr { .. } => reserved[1] = true,
                    SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } if peer_id == a_id => {
                        // A hole punch may add a direct connection later.
                        relayed |= endpoint.is_relayed();
                        b.behaviour_mut().node.send_request(&a_id, proto::NodeRequest::default());
                    }
                    SwarmEvent::Behaviour(BehaviourEvent::Node(request_response::Event::Message {
                        message: request_response::Message::Response { response, .. }, ..
                    })) => {
                        assert_eq!(response.status(), proto::Status::Unsupported);
                        answered = true;
                    }
                    SwarmEvent::Behaviour(BehaviourEvent::Node(
                        request_response::Event::OutboundFailure { error, .. },
                    )) => panic!("{error}"),
                    _ => {}
                },
            }
        }
    })
    .await
    .unwrap();
    assert!(relayed, "the first connection runs through the circuit");

    // The relay's Kademlia saw both circuits announced by identify and stored none of them.
    b.behaviour_mut().kad.get_closest_peers(a_id);
    let peers = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            tokio::select! {
                _ = a.select_next_some() => {}
                event = b.select_next_some() => {
                    if let SwarmEvent::Behaviour(BehaviourEvent::Kad(
                        kad::Event::OutboundQueryProgressed {
                            result: kad::QueryResult::GetClosestPeers(result), ..
                        },
                    )) = event
                    {
                        break result.unwrap().peers;
                    }
                }
            }
        }
    })
    .await
    .unwrap();
    for info in peers {
        assert!(
            info.addrs.iter().all(|a| !dyapp_p2p_net::is_circuit(a)),
            "{info:?}"
        );
    }
    running.abort();
}
