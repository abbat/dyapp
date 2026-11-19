use super::*;
use dyapp_p2p_net::{build_configured_swarm, SwarmOptions};
async fn server() -> (libp2p::Swarm<dyapp_p2p_net::Behaviour>, Multiaddr) {
    let mut swarm = build_configured_swarm(
        libp2p::identity::Keypair::generate_ed25519(),
        SwarmOptions::default(),
    )
    .expect("server swarm");
    swarm
        .listen_on("/ip4/127.0.0.1/tcp/0".parse().expect("listen address"))
        .expect("listen");
    let address = loop {
        if let SwarmEvent::NewListenAddr { address, .. } = swarm.select_next_some().await {
            break address;
        }
    };
    swarm.add_external_address(address.clone());
    (swarm, address)
}

#[test]
fn entry_is_restored_once_whether_present_or_omitted() {
    let peer = PeerId::random();
    let address: Multiaddr = "/ip4/127.0.0.1/tcp/4001".parse().expect("address");
    let other = kad::PeerInfo {
        peer_id: PeerId::random(),
        addrs: Vec::new(),
    };
    let entry = kad::PeerInfo {
        peer_id: peer,
        addrs: Vec::new(),
    };
    let mut candidates = vec![other, entry];
    restore_entry(&mut candidates, peer, &address);
    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0].addrs, Vec::<Multiaddr>::new());
    assert_eq!(candidates[1].addrs, vec![address.clone()]);

    let existing: Multiaddr = "/ip4/127.0.0.1/tcp/4002".parse().expect("address");
    candidates[1].addrs = vec![existing.clone()];
    restore_entry(&mut candidates, peer, &address);
    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[1].addrs, vec![address.clone(), existing.clone()]);
    restore_entry(&mut candidates, peer, &address);
    assert_eq!(candidates[1].addrs, vec![address.clone(), existing]);

    candidates.clear();
    restore_entry(&mut candidates, peer, &address);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].peer_id, peer);
    assert_eq!(candidates[0].addrs, vec![address]);
}

#[tokio::test]
async fn entry_connection_is_established_before_the_test_deadline() {
    let (mut swarm, address) = server().await;
    let peer = *swarm.local_peer_id();
    let task = tokio::spawn(async move {
        loop {
            swarm.select_next_some().await;
        }
    });
    let node = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        Node::connect(address, Arc::new(Protocols::public())),
    )
    .await
    .expect("connection deadline")
    .expect("entry connection");
    assert_eq!(node.peer(), peer);
    task.abort();
}

#[tokio::test]
async fn entry_is_the_fallback_when_routing_returns_no_peers() {
    dyapp_p2p_net::set_id_pow_bits(0);
    let (mut swarm, address) = server().await;
    let peer = *swarm.local_peer_id();
    let task = tokio::spawn(async move {
        loop {
            swarm.select_next_some().await;
        }
    });
    let mut entry = Node::connect(address, Arc::new(Protocols::public()))
        .await
        .expect("entry connection");
    let mut replicas = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        entry.replicas(&[7; 32], Arc::new(Protocols::public())),
    )
    .await
    .expect("replica discovery deadline");
    assert_eq!(replicas.nodes_mut().len(), 1);
    assert_eq!(replicas.nodes_mut()[0].peer(), peer);
    assert_eq!(replicas.missing().count(), 0);
    task.abort();
}

#[test]
fn empty_routing_has_explicit_missing_coverage() {
    let mut replicas = ReplicaSet {
        nodes: Vec::new(),
        keys: vec![vec![1], vec![2]],
        holders: vec![None, None],
    };
    assert_eq!(replicas.missing().collect::<Vec<_>>(), [&[1][..], &[2][..]]);
    assert!(replicas.for_key(&[1]).is_none());
    assert!(replicas.nodes_mut().is_empty());
}

#[tokio::test]
async fn an_unavailable_holder_is_skipped() {
    let protocols = Arc::new(Protocols::public());
    let unavailable = "/ip4/127.0.0.1/tcp/1".parse().expect("address");
    assert!(connect(PeerId::random(), vec![unavailable], &protocols)
        .await
        .is_none());
}

#[tokio::test]
async fn distinct_holders_cover_every_key_and_are_reused() {
    dyapp_p2p_net::set_id_pow_bits(0);
    let (mut first, address) = server().await;
    let (mut second, second_address) = server().await;
    first
        .behaviour_mut()
        .kad
        .add_address(second.local_peer_id(), second_address.clone());
    second
        .behaviour_mut()
        .kad
        .add_address(first.local_peer_id(), address.clone());
    let tasks = vec![first, second]
        .into_iter()
        .map(|mut swarm| {
            tokio::spawn(async move {
                loop {
                    swarm.select_next_some().await;
                }
            })
        })
        .collect::<Vec<_>>();
    let mut entry = Node::connect(address, Arc::new(Protocols::public()))
        .await
        .expect("entry connection");
    let mut replicas = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        entry.replicas(&[7; 32], Arc::new(Protocols::public())),
    )
    .await
    .expect("replica discovery deadline");
    assert_eq!(replicas.nodes_mut().len(), 2);
    assert_eq!(replicas.missing().count(), 0);
    let peers = replicas
        .nodes_mut()
        .iter()
        .map(Node::peer)
        .collect::<Vec<_>>();
    for index in 0..REPLICAS {
        let key = replica_key(&[7; 32], index);
        let peer = replicas.for_key(&key).map(|node| node.peer());
        assert!(peer.is_some_and(|peer| peers.contains(&peer)));
    }
    assert!(replicas.for_key(&[9; 32]).is_none());
    for task in tasks {
        task.abort();
    }
}

#[tokio::test]
async fn slow_holders_hit_one_discovery_budget_and_keep_partial_coverage() {
    dyapp_p2p_net::set_id_pow_bits(0);
    let (mut server, address) = server().await;
    let server_peer = *server.local_peer_id();
    let unavailable = PeerId::random();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("silent holder listener");
    let unavailable_address = format!(
        "/ip4/127.0.0.1/tcp/{}",
        listener.local_addr().unwrap().port()
    );
    let stub = tokio::spawn(async move {
        let mut sockets = Vec::new();
        loop {
            sockets.push(listener.accept().await.unwrap());
        }
    });
    server.behaviour_mut().kad.add_address(
        &unavailable,
        unavailable_address.parse().expect("silent holder"),
    );
    let task = tokio::spawn(async move {
        loop {
            server.select_next_some().await;
        }
    });
    let mut entry = Node::connect(address, Arc::new(Protocols::public()))
        .await
        .expect("entry connection");
    let mut replicas = tokio::time::timeout(
        OPERATION_TIMEOUT + std::time::Duration::from_secs(5),
        entry.replicas(&[9; 32], Arc::new(Protocols::public())),
    )
    .await
    .expect("one replica discovery budget");
    assert_eq!(replicas.nodes_mut().len(), 1);
    assert_eq!(replicas.nodes_mut()[0].peer(), server_peer);
    let missing = replicas.missing().count();
    assert!(missing > 0 && missing < usize::from(REPLICAS));
    task.abort();
    stub.abort();
}
