use super::*;

#[test]
fn same_ip_compares_direct_hosts_not_groups_or_ports() {
    let addr = |s: &str| s.parse::<Multiaddr>().unwrap();
    for (a, b, expected) in [
        ("/ip4/1.2.3.4/tcp/1", "/ip4/1.2.3.4/udp/9/quic-v1", true),
        ("/ip4/1.2.3.4/tcp/1", "/ip4/1.2.3.5/tcp/1", false),
        ("/ip6/2001:db8::1/tcp/1", "/ip6/2001:db8::1/tcp/2", true),
        ("/ip6/2001:db8::1/tcp/1", "/ip6/2001:db8::2/tcp/1", false),
        ("/ip4/1.2.3.4/tcp/1", "/ip6/::ffff:1.2.3.4/tcp/1", false),
    ] {
        assert_eq!(same_ip(&addr(a), &addr(b)), expected, "{a} vs {b}");
        assert_eq!(same_ip(&addr(b), &addr(a)), expected, "{b} vs {a}");
    }
    let direct = addr("/ip4/1.2.3.4/tcp/1");
    let pinned = direct.clone().with(Protocol::P2p(PeerId::random()));
    assert!(same_ip(&direct, &pinned));
    for unproven in [
        "",
        "/tcp/1",
        "/dns/example.org/tcp/1",
        "/dns4/example.org/tcp/1",
        "/dns6/example.org/tcp/1",
        "/dnsaddr/example.org",
        "/tcp/1/ip4/1.2.3.4",
        "/ip4/1.2.3.4/dns/example.org/tcp/1",
        "/ip4/1.2.3.4/ip4/1.2.3.4/tcp/1",
        "/ip4/1.2.3.4/tcp/1/p2p-circuit",
    ] {
        let unproven = addr(unproven);
        assert!(!same_ip(&direct, &unproven), "{unproven}");
        assert!(!same_ip(&unproven, &direct), "{unproven}");
        assert!(!same_ip(&unproven, &unproven), "{unproven}");
    }
}

#[test]
fn filtered_kad_keeps_local_provider_announcements() {
    use libp2p::kad::store::RecordStore as _;
    let mut swarm = build_swarm(Keypair::generate_ed25519(), Mode::Auto).unwrap();
    let key = kad::RecordKey::new(&b"local-provider");
    swarm
        .behaviour_mut()
        .kad
        .start_providing(key.clone())
        .unwrap();
    let providers = swarm.behaviour_mut().kad.store_mut().providers(&key);
    assert_eq!(providers.len(), 1);
    assert_eq!(providers[0].provider, *swarm.local_peer_id());
}

#[derive(Clone, PartialEq, prost::Message)]
struct Frame {
    #[prost(bytes = "vec", tag = "1")]
    data: Vec<u8>,
}

#[test]
fn old_heartbeat_decodes_with_version_zero() {
    use prost::Message;
    // Field 2 is absent on the wire when it is zero, as from a client older than it.
    let old = proto::Heartbeat {
        time: 7,
        version: 0,
        ..Default::default()
    }
    .encode_to_vec();
    assert_eq!(proto::Heartbeat::decode(old.as_slice()).unwrap().version, 0);
    // Field 3 is absent from an older inventory request: no upper bound.
    let old = proto::InventoryRequest {
        topic: 3,
        after: vec![1],
        before: vec![],
    }
    .encode_to_vec();
    assert!(proto::InventoryRequest::decode(old.as_slice())
        .unwrap()
        .before
        .is_empty());
}

fn frame_bytes(size: usize) -> Vec<u8> {
    use prost::Message;
    let mut frame = Frame {
        data: vec![0; size],
    };
    let overhead = frame.encoded_len() - size;
    frame.data.truncate(size - overhead);
    let bytes = frame.encode_to_vec();
    assert_eq!(bytes.len(), size);
    bytes
}

/// A reader that fails the test if admission touches any request-body bytes.
struct UnreadBody;

impl AsyncRead for UnreadBody {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
        _: &mut [u8],
    ) -> std::task::Poll<io::Result<usize>> {
        panic!("rejected media stream read its body");
    }
}

#[tokio::test]
async fn media_permits_are_shared_and_reject_before_reading() {
    use libp2p::futures::io::Cursor;
    use request_response::Codec;
    let codec = ProtoCodec::<Frame, Frame>::media();
    let mut streams = Vec::new();
    for _ in 0..16 {
        let mut stream = codec.clone();
        stream
            .read_request(&MEDIA_PROTOCOL, &mut Cursor::new(Vec::new()))
            .await
            .unwrap();
        streams.push(stream);
    }
    let mut extra = codec.clone();
    assert_eq!(
        extra
            .read_request(&MEDIA_PROTOCOL, &mut UnreadBody)
            .await
            .unwrap_err()
            .kind(),
        io::ErrorKind::WouldBlock
    );
    // A completed response frees a slot, independently of the connection's codec clone.
    streams[0]
        .write_response(
            &MEDIA_PROTOCOL,
            &mut Cursor::new(Vec::new()),
            Frame::default(),
        )
        .await
        .unwrap();
    extra
        .read_request(&MEDIA_PROTOCOL, &mut Cursor::new(Vec::new()))
        .await
        .unwrap();
    assert_eq!(codec.permits.as_ref().unwrap().available_permits(), 0);
    drop(extra);
    // Malformed requests must return their permit as well.
    let mut malformed = codec.clone();
    assert!(malformed
        .read_request(&MEDIA_PROTOCOL, &mut Cursor::new(vec![0xff]))
        .await
        .is_err());
    assert_eq!(codec.permits.as_ref().unwrap().available_permits(), 1);
    drop(streams);
    assert_eq!(codec.permits.as_ref().unwrap().available_permits(), 16);
    // A different swarm has its own pool.
    let independent = ProtoCodec::<Frame, Frame>::media();
    assert_eq!(
        independent.permits.as_ref().unwrap().available_permits(),
        16
    );
}

#[tokio::test]
async fn named_builder_announces_only_its_network_and_exchanges_large_media_over_tcp() {
    use prost::Message;
    let protocols = Arc::new(Protocols::new("builder-test").unwrap());
    let a_wire = Arc::new(WireBytes::default());
    let b_wire = Arc::new(WireBytes::default());
    let make = |mode, wire| {
        build_configured_swarm(
            Keypair::generate_ed25519(),
            SwarmOptions {
                mode,
                search: true,
                protocols: protocols.clone(),
                wire,
                ..Default::default()
            },
        )
        .unwrap()
    };
    let mut a = make(Mode::Auto, a_wire.clone());
    let mut b = make(Mode::Client, b_wire.clone());
    a.behaviour_mut().kad.set_mode(Some(kad::Mode::Server));
    a.listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .unwrap();
    let address = loop {
        if let SwarmEvent::NewListenAddr { address, .. } = a.select_next_some().await {
            break address;
        }
    };
    let request = proto::MediaRequest {
        request: Some(proto::media_request::Request::Put(proto::MediaPut {
            owner: vec![1; 32],
            data: vec![7; 2 << 20],
        })),
    };
    let request_len = request.encoded_len() as u64;
    let expected_response = proto::MediaResponse {
        status: proto::Status::Ok.into(),
        data: vec![7; 2 << 20],
        ..Default::default()
    };
    let response_len = expected_response.encoded_len() as u64;
    let check_identify = |info: &identify::Info, server: bool| {
        assert_eq!(info.protocol_version, protocols.version);
        let ids = info
            .protocols
            .iter()
            .map(|id| id.as_ref())
            .collect::<Vec<_>>();
        for id in &ids {
            if id.starts_with("/dyapp") || id.starts_with("/meshsub") {
                assert!(
                    id.starts_with(&format!("{}/", protocols.version)),
                    "public protocol leaked: {id}"
                );
            }
        }
        if server {
            for id in protocols
                .all()
                .into_iter()
                .filter(|id| *id != &protocols.mailbox_push)
            {
                assert!(ids.contains(&id.as_ref()), "not advertised: {id}");
            }
        } else {
            assert!(ids.contains(&protocols.mailbox_push.as_ref()));
        }
        assert!(ids
            .iter()
            .any(|id| id.starts_with(protocols.gossip_prefix.as_ref().unwrap())));
    };
    b.dial(address).unwrap();
    let (mut a_identified, mut b_identified, mut received) = (false, false, false);
    tokio::time::timeout(Duration::from_secs(10), async {
        while !(a_identified && b_identified && received) {
            tokio::select! {
                event = a.select_next_some() => match event {
                    SwarmEvent::Behaviour(BehaviourEvent::Identify(identify::Event::Received { info, .. })) => {
                        check_identify(&info, false);
                        a_identified = true;
                    }
                    SwarmEvent::Behaviour(BehaviourEvent::Media(request_response::Event::Message {
                        message: request_response::Message::Request { request, channel, .. }, ..
                    })) => {
                        let Some(proto::media_request::Request::Put(put)) = request.request else { panic!("not media put") };
                        a.behaviour_mut().media.send_response(channel, proto::MediaResponse {
                            status: proto::Status::Ok.into(), data: put.data, ..Default::default()
                        }).unwrap();
                    }
                    _ => {}
                },
                event = b.select_next_some() => match event {
                    SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                        b.behaviour_mut().media.send_request(&peer_id, request.clone());
                    }
                    SwarmEvent::Behaviour(BehaviourEvent::Identify(identify::Event::Received { info, .. })) => {
                        check_identify(&info, true);
                        b_identified = true;
                    }
                    SwarmEvent::Behaviour(BehaviourEvent::Media(request_response::Event::Message {
                        message: request_response::Message::Response { response, .. }, ..
                    })) => {
                        assert_eq!(response, expected_response);
                        received = true;
                    }
                    _ => {}
                }
            }
        }
    }).await.unwrap();
    let mut expected = [[0; 2]; 11];
    expected[4] = [request_len, response_len];
    assert_eq!(a_wire.take().0, expected);
    expected[4] = [response_len, request_len];
    assert_eq!(b_wire.take().0, expected);
}

#[tokio::test]
async fn search_nodes_gossip() {
    let search_swarm = || {
        let limits = connection_limits::ConnectionLimits::default();
        let keypair = Keypair::generate_ed25519();
        let mut s = build_limited_swarm(keypair, Mode::Auto, limits, 64, usize::MAX, true);
        let gossip = s.as_mut().unwrap().behaviour_mut().gossip.as_mut().unwrap();
        gossip.subscribe(&search_topic(3)).unwrap();
        s.unwrap()
    };
    let (mut a, mut b) = (search_swarm(), search_swarm());
    a.listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .unwrap();
    let addr = loop {
        if let SwarmEvent::NewListenAddr { address, .. } = a.select_next_some().await {
            break address;
        }
    };
    b.dial(addr).unwrap();
    let got = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            tokio::select! {
                e = a.select_next_some() => {
                    if let SwarmEvent::Behaviour(BehaviourEvent::Gossip(
                        gossipsub::Event::Subscribed { .. },
                    )) = e
                    {
                        let gossip = a.behaviour_mut().gossip.as_mut().unwrap();
                        gossip.publish(search_topic(3), b"hello".to_vec()).unwrap();
                    }
                }
                e = b.select_next_some() => {
                    if let SwarmEvent::Behaviour(BehaviourEvent::Gossip(
                        gossipsub::Event::Message { message, .. },
                    )) = e
                    {
                        break message.data;
                    }
                }
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(got, b"hello");
    let plain = build_swarm(Keypair::generate_ed25519(), Mode::Auto).unwrap();
    assert!(plain.behaviour().gossip.as_ref().is_none());
}

#[tokio::test]
async fn configured_media_ids_keep_large_frames_and_stable_counter_slots() {
    use libp2p::futures::io::Cursor;
    use prost::Message;
    use request_response::Codec;
    for protocols in [Protocols::public(), Protocols::new("codec-test").unwrap()] {
        let protocols = Arc::new(protocols);
        let wire = Arc::new(WireBytes::default());
        let mut codec = ProtoCodec::<Frame, Frame>::media()
            .with_wire(wire.clone())
            .with_protocols(protocols.clone());
        let frame = Frame {
            data: vec![0; 2 << 20],
        };
        let bytes = frame.encode_to_vec();
        codec
            .read_request(&protocols.media, &mut Cursor::new(&bytes))
            .await
            .unwrap();
        let mut cloned = codec.clone();
        assert!(Arc::ptr_eq(&cloned.protocols, &protocols));
        assert!(cloned.permit.is_none());
        cloned
            .read_response(&protocols.media, &mut Cursor::new(&bytes))
            .await
            .unwrap();
        cloned
            .write_request(
                &protocols.media,
                &mut Cursor::new(Vec::new()),
                frame.clone(),
            )
            .await
            .unwrap();
        codec
            .write_response(&protocols.media, &mut Cursor::new(Vec::new()), frame)
            .await
            .unwrap();
        let mut expected = [[0; 2]; 11];
        expected[4] = [2 * bytes.len() as u64; 2];
        assert_eq!(wire.take().0, expected);
        assert_eq!(codec.permits.as_ref().unwrap().available_permits(), 16);
        for foreign in [&protocols.node, &StreamProtocol::new("/unknown/media")] {
            assert!(codec
                .read_request(foreign, &mut Cursor::new(&bytes))
                .await
                .is_err());
            assert!(codec
                .read_response(foreign, &mut Cursor::new(&bytes))
                .await
                .is_err());
        }
        if protocols.media != MEDIA_PROTOCOL {
            assert!(codec
                .read_request(&MEDIA_PROTOCOL, &mut Cursor::new(&bytes))
                .await
                .is_err());
        }
        assert_eq!(wire.take().0, [[0; 2]; 11]);
        let unknown = frame_bytes(16);
        codec
            .read_request(
                &StreamProtocol::new("/unknown/media"),
                &mut Cursor::new(&unknown),
            )
            .await
            .unwrap();
        let counts = wire.take();
        assert_eq!(counts.0[WireBytes::OTHER], [16, 0]);
        assert_eq!(counts.0[4], [0, 0]);
    }
}

#[tokio::test]
async fn codec_counts_exact_frames_in_all_directions_and_shares_clones() {
    use libp2p::futures::io::Cursor;
    use prost::Message;
    use request_response::Codec;
    let wire = Arc::new(WireBytes::default());
    let mut codec = ProtoCodec::<Frame, Frame>::default().with_wire(wire.clone());
    let mut cloned = codec.clone();
    for protocol in [
        NODE_PROTOCOL,
        PROFILE_PROTOCOL,
        MAILBOX_PROTOCOL,
        MAILBOX_PUSH_PROTOCOL,
        MEDIA_PROTOCOL,
        SEARCH_PROTOCOL,
        INVENTORY_PROTOCOL,
        DIGEST_PROTOCOL,
        StreamProtocol::new("/unknown"),
    ] {
        let mut incoming = frame_bytes(13);
        incoming.extend([0x10, 1]); // Unknown field: count wire bytes, not a decoded re-encoding.
        codec
            .read_request(&protocol, &mut Cursor::new(&incoming))
            .await
            .unwrap();
        cloned
            .read_response(&protocol, &mut Cursor::new(&incoming))
            .await
            .unwrap();
        let frame = Frame { data: vec![3; 7] };
        let n = frame.encoded_len();
        codec
            .write_request(&protocol, &mut Cursor::new(Vec::new()), frame.clone())
            .await
            .unwrap();
        cloned
            .write_response(&protocol, &mut Cursor::new(Vec::new()), frame)
            .await
            .unwrap();
        let mut expected = [[0; 2]; 11];
        expected[WireBytes::protocol_slot(&protocol)] = [2 * incoming.len() as u64, 2 * n as u64];
        assert_eq!(wire.take().0, expected);
    }
}

#[tokio::test]
async fn failed_codec_io_does_not_count_and_releases_media_permits() {
    use libp2p::futures::io::Cursor;
    use request_response::Codec;
    struct BrokenRead(bool);
    impl AsyncRead for BrokenRead {
        fn poll_read(
            mut self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
            buf: &mut [u8],
        ) -> std::task::Poll<io::Result<usize>> {
            if self.0 {
                std::task::Poll::Ready(Err(io::Error::other("read failed")))
            } else {
                self.0 = true;
                buf[0] = 0;
                std::task::Poll::Ready(Ok(1))
            }
        }
    }
    let wire = Arc::new(WireBytes::default());
    let mut codec = ProtoCodec::<Frame, Frame>::media().with_wire(wire.clone());
    for bytes in [
        vec![0xff],
        frame_bytes(message_limit(&MEDIA_PROTOCOL, &Protocols::public()) as usize + 1),
    ] {
        assert!(codec
            .read_request(&MEDIA_PROTOCOL, &mut Cursor::new(&bytes))
            .await
            .is_err());
        assert!(codec
            .read_response(&MEDIA_PROTOCOL, &mut Cursor::new(&bytes))
            .await
            .is_err());
    }
    assert!(codec
        .read_response(&MEDIA_PROTOCOL, &mut Cursor::new(Vec::new()))
        .await
        .is_err());
    assert!(codec
        .read_request(&MEDIA_PROTOCOL, &mut BrokenRead(false))
        .await
        .is_err());
    assert!(codec
        .read_response(&MEDIA_PROTOCOL, &mut BrokenRead(false))
        .await
        .is_err());
    codec
        .read_request(&MEDIA_PROTOCOL, &mut Cursor::new(Vec::new()))
        .await
        .unwrap();
    let cloned = codec.clone();
    assert!(cloned.permit.is_none());
    assert!(Arc::ptr_eq(&cloned.wire, &wire));
    let frame = Frame { data: vec![1; 5] };
    let mut prefix = [0; 1];
    assert!(codec
        .write_request(
            &MEDIA_PROTOCOL,
            &mut Cursor::new(&mut prefix[..]),
            frame.clone()
        )
        .await
        .is_err());
    assert_ne!(prefix[0], 0); // A partial write happened but contributes no completed frame.
    assert!(codec
        .write_response(&MEDIA_PROTOCOL, &mut Cursor::new(&mut prefix[..]), frame)
        .await
        .is_err());
    assert_eq!(codec.permits.as_ref().unwrap().available_permits(), 16);
    assert_eq!(wire.take().0, [[0; 2]; 11]);
}

#[tokio::test]
async fn codec_enforces_protocol_limits_in_both_directions() {
    use libp2p::futures::io::Cursor;
    use request_response::Codec;
    let mut codec = ProtoCodec::<Frame, Frame>::default();
    for protocol in [
        NODE_PROTOCOL,
        PROFILE_PROTOCOL,
        MAILBOX_PROTOCOL,
        MAILBOX_PUSH_PROTOCOL,
        MEDIA_PROTOCOL,
    ] {
        let limit = usize::try_from(message_limit(&protocol, &Protocols::public())).unwrap();
        for size in [limit, limit + 1] {
            let bytes = frame_bytes(size);
            let request = codec
                .read_request(&protocol, &mut Cursor::new(&bytes))
                .await;
            let response = codec
                .read_response(&protocol, &mut Cursor::new(&bytes))
                .await;
            if size == limit {
                assert!(request.is_ok(), "{protocol}: request at limit");
                assert!(response.is_ok(), "{protocol}: response at limit");
            } else {
                assert_eq!(request.unwrap_err().kind(), io::ErrorKind::InvalidData);
                assert_eq!(response.unwrap_err().kind(), io::ErrorKind::InvalidData);
            }
        }
    }
    let bytes = frame_bytes(6 * 1024 * 1024);
    assert!(codec
        .read_request(&MEDIA_PROTOCOL, &mut Cursor::new(&bytes))
        .await
        .is_ok());
    assert!(codec
        .read_response(&MEDIA_PROTOCOL, &mut Cursor::new(&bytes))
        .await
        .is_ok());
    assert!(codec
        .read_request(&MAILBOX_PROTOCOL, &mut Cursor::new(&bytes))
        .await
        .is_err());
}
use libp2p::futures::StreamExt;
use libp2p::swarm::SwarmEvent;
use libp2p::Multiaddr;

#[test]
fn pow_keypair_has_the_leading_zero_bits() {
    let peer = generate_pow_keypair(12).public().to_peer_id();
    assert!(id_has_pow(&peer, 12));
    assert!(id_has_pow(&peer, 0));
    let weak = (0..64)
        .map(|_| PeerId::random())
        .filter(|p| !id_has_pow(p, 12))
        .count();
    assert!(weak > 0);
}

#[test]
fn unknown_request_variant_decodes_as_none() {
    use prost::Message;
    // Field 15 of a request is a variant from a newer client: the node answers unsupported.
    let newer = [15 << 3 | 2, 0];
    let request = proto::NodeRequest::decode(newer.as_slice()).unwrap();
    assert_eq!(request.request, None);
}

#[tokio::test]
async fn client_learns_server_through_identify() {
    let mut server = build_swarm(Keypair::generate_ed25519(), Mode::Auto).unwrap();
    server
        .listen_on("/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap())
        .unwrap();
    let addr: Multiaddr = loop {
        if let SwarmEvent::NewListenAddr { address, .. } = server.select_next_some().await {
            break address;
        }
    };
    // A confirmed external address switches Kademlia to server mode.
    server.add_external_address(addr.clone());

    let mut client = build_swarm(Keypair::generate_ed25519(), Mode::Client).unwrap();
    client.dial(addr).unwrap();
    let server_id = *server.local_peer_id();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            tokio::select! {
                _ = server.select_next_some() => {}
                event = client.select_next_some() => {
                    route(&mut client, &event);
                    if let SwarmEvent::Behaviour(BehaviourEvent::Kad(
                        kad::Event::RoutingUpdated { peer, .. },
                    )) = event
                    {
                        if peer == server_id {
                            return;
                        }
                    }
                }
            }
        }
    })
    .await
    .expect("client did not add the server to its routing table");
}

#[tokio::test]
async fn seeds_fill_the_peer_cache_and_the_cache_joins() {
    let mut server = build_swarm(Keypair::generate_ed25519(), Mode::Auto).unwrap();
    server
        .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .unwrap();
    let addr: Multiaddr = loop {
        if let SwarmEvent::NewListenAddr { address, .. } = server.select_next_some().await {
            break address;
        }
    };
    server.add_external_address(addr.clone());
    let cached: Multiaddr = format!("{addr}/p2p/{}", server.local_peer_id())
        .parse()
        .unwrap();

    let mut first = build_swarm(Keypair::generate_ed25519(), Mode::Client).unwrap();
    join(&mut first, &[addr], &[]);
    tokio::time::timeout(Duration::from_secs(10), async {
        while known_peers(&mut first) != [cached.clone()] {
            tokio::select! {
                _ = server.select_next_some() => {}
                event = first.select_next_some() => route(&mut first, &event),
            }
        }
    })
    .await
    .expect("the seed did not reach the routing table");

    // The next start needs no seed: the cache alone fills the table.
    let mut next = build_swarm(Keypair::generate_ed25519(), Mode::Client).unwrap();
    join(&mut next, &[], &known_peers(&mut first));
    assert_eq!(known_peers(&mut next), [cached]);
}

#[tokio::test]
async fn one_ip_group_fills_few_routing_slots() {
    let mut swarm = build_swarm(Keypair::generate_ed25519(), Mode::Auto).unwrap();
    let add = |swarm: &mut Swarm<Behaviour>, ip: String| {
        let address = format!("/ip4/{ip}/tcp/1").parse().unwrap();
        add_peer(swarm, PeerId::random(), address)
    };
    assert!((1..=5).all(|i| add(&mut swarm, format!("10.{i}.0.1"))));
    let peer = PeerId::random();
    assert!(!routed(&mut swarm, &peer));
    let address = "/ip4/10.6.0.1/tcp/1".parse().unwrap();
    assert!(add_peer(&mut swarm, peer, address));
    assert!(routed(&mut swarm, &peer));
    let admitted = (1..=30)
        .filter(|i| add(&mut swarm, format!("10.9.9.{i}")))
        .count();
    assert!((1..=MAX_GROUP_PER_TABLE).contains(&admitted), "{admitted}");
    for bucket in swarm.behaviour_mut().kad.kbuckets() {
        let same = bucket.iter().filter(|e| {
            e.node
                .value
                .iter()
                .any(|a| a.to_string().starts_with("/ip4/10.9.9."))
        });
        assert!(same.count() <= MAX_GROUP_PER_BUCKET);
    }
    // One /16 in several /24s: still one peer per bucket.
    let admitted = (1..=30)
        .filter(|i| add(&mut swarm, format!("10.7.{i}.1")))
        .count();
    assert!(admitted >= 1);
    for bucket in swarm.behaviour_mut().kad.kbuckets() {
        let wide = bucket.iter().filter(|e| {
            e.node
                .value
                .iter()
                .any(|a| a.to_string().starts_with("/ip4/10.7."))
        });
        assert!(wide.count() <= 1);
    }
}

#[tokio::test]
async fn search_routing_has_its_own_ip_group_limits() {
    let mut swarm = build_swarm(Keypair::generate_ed25519(), Mode::Auto).unwrap();
    let admitted = (1..=30)
        .filter(|i| {
            let address = format!("/ip4/10.9.9.{i}/tcp/1").parse().unwrap();
            add_search_peer(&mut swarm, PeerId::random(), address)
        })
        .count();
    assert!((1..=MAX_GROUP_PER_TABLE).contains(&admitted), "{admitted}");
    let search: usize = swarm
        .behaviour_mut()
        .search_kad
        .kbuckets()
        .map(|b| b.num_entries())
        .sum();
    assert_eq!(search, admitted);
    assert_eq!(swarm.behaviour_mut().kad.kbuckets().count(), 0);
}

#[tokio::test]
async fn client_gets_node_info() {
    let mut server = build_swarm(Keypair::generate_ed25519(), Mode::Auto).unwrap();
    server
        .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .unwrap();
    let addr: Multiaddr = loop {
        if let SwarmEvent::NewListenAddr { address, .. } = server.select_next_some().await {
            break address;
        }
    };
    let mut client = build_swarm(Keypair::generate_ed25519(), Mode::Client).unwrap();
    client.add_peer_address(*server.local_peer_id(), addr);
    let request = proto::NodeRequest {
        request: Some(proto::node_request::Request::Info(proto::InfoRequest {})),
    };
    client
        .behaviour_mut()
        .node
        .send_request(server.local_peer_id(), request);

    let reply = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            tokio::select! {
                event = server.select_next_some() => {
                    if let SwarmEvent::Behaviour(BehaviourEvent::Node(
                        request_response::Event::Message {
                            message: request_response::Message::Request { request: got, channel, .. },
                            ..
                        },
                    )) = event
                    {
                        assert_eq!(got, request);
                        let response = proto::NodeResponse {
                            status: proto::Status::Ok.into(),
                            info: Some(proto::NodeInfo { max_profile_bytes: 7, ..Default::default() }),
                            ..Default::default()
                        };
                        server.behaviour_mut().node.send_response(channel, response).unwrap();
                    }
                }
                event = client.select_next_some() => {
                    if let SwarmEvent::Behaviour(BehaviourEvent::Node(
                        request_response::Event::Message {
                            message: request_response::Message::Response { response, .. },
                            ..
                        },
                    )) = event
                    {
                        return response;
                    }
                }
            }
        }
    })
    .await
    .expect("no reply");
    assert_eq!(reply.info.unwrap().max_profile_bytes, 7);
}

// The guard sits below DNS: a name that resolves to loopback is refused like the IP itself,
// while a disabled guard and an unguarded client reach the same listener.
#[tokio::test]
async fn dial_guard_refuses_names_resolving_to_private_ips() {
    let mut node = build_swarm(Keypair::generate_ed25519(), Mode::Auto).unwrap();
    node.listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .unwrap();
    let port = loop {
        if let SwarmEvent::NewListenAddr { address, .. } = node.select_next_some().await {
            break address.iter().find_map(|p| match p {
                Protocol::Tcp(port) => Some(port),
                _ => None,
            });
        }
    }
    .unwrap();
    for (guard, reaches) in [
        (Some(DialGuard::new(true)), false),
        (Some(DialGuard::new(false)), true),
        (None, true),
    ] {
        let mut dialer = build_configured_swarm(
            Keypair::generate_ed25519(),
            SwarmOptions {
                dial_guard: guard.clone(),
                ..SwarmOptions::default()
            },
        )
        .unwrap();
        assert_eq!(dialer.behaviour().pow_gate.is_enabled(), guard.is_some());
        let address: Multiaddr = format!("/dns4/localhost/tcp/{port}").parse().unwrap();
        dialer.dial(address).unwrap();
        let connected = tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                tokio::select! {
                    _ = node.select_next_some() => {}
                    event = dialer.select_next_some() => match event {
                        SwarmEvent::ConnectionEstablished { .. } => break true,
                        SwarmEvent::OutgoingConnectionError { .. } => break false,
                        _ => {}
                    },
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(connected, reaches);
        let refused = guard.map_or(0, |guard| guard.take_denied());
        assert_eq!(refused > 0, !reaches);
    }
}

// A filter refuses a denied IP and a name resolving into a denied range, counting each once,
// while two filtered swarms still connect over loopback.
#[tokio::test]
async fn dial_filter_refuses_denied_and_resolved_addresses_below_dns() {
    let filtered = |nets: &[&str]| {
        let filter = DialFilter::new(nets.iter().map(|net| net.parse().unwrap()).collect());
        let options = SwarmOptions {
            dial_filter: filter.clone(),
            ..SwarmOptions::default()
        };
        (
            build_configured_swarm(Keypair::generate_ed25519(), options).unwrap(),
            filter,
        )
    };
    let (mut node, _) = filtered(&["169.254.0.0/16"]);
    node.listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .unwrap();
    let port = loop {
        if let SwarmEvent::NewListenAddr { address, .. } = node.select_next_some().await {
            break address.iter().find_map(|p| match p {
                Protocol::Tcp(port) => Some(port),
                _ => None,
            });
        }
    }
    .unwrap();
    for (nets, address, reaches) in [
        (
            &["169.254.0.0/16"][..],
            "/ip4/169.254.169.254/tcp/80".to_string(),
            false,
        ),
        (
            &["127.0.0.0/8"][..],
            format!("/dns4/localhost/tcp/{port}"),
            false,
        ),
        (
            &["169.254.0.0/16"][..],
            format!("/ip4/127.0.0.1/tcp/{port}"),
            true,
        ),
    ] {
        let (mut dialer, filter) = filtered(nets);
        dialer.dial(address.parse::<Multiaddr>().unwrap()).unwrap();
        let connected = tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                tokio::select! {
                    _ = node.select_next_some() => {}
                    event = dialer.select_next_some() => match event {
                        SwarmEvent::ConnectionEstablished { .. } => break true,
                        SwarmEvent::OutgoingConnectionError { .. } => break false,
                        _ => {}
                    },
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(connected, reaches, "{address}");
        assert_eq!(filter.take_denied(), u64::from(!reaches), "{address}");
    }
}
