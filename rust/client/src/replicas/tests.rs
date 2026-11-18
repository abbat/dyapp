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
fn entry_address_is_restored_only_for_an_empty_self_entry() {
    let peer = PeerId::random();
    let address: Multiaddr = "/ip4/127.0.0.1/tcp/4001".parse().expect("address");
    let mut entry = kad::PeerInfo {
        peer_id: peer,
        addrs: Vec::new(),
    };
    restore_entry_address(&mut entry, peer, &address);
    assert_eq!(entry.addrs, vec![address.clone()]);

    let mut other = kad::PeerInfo {
        peer_id: PeerId::random(),
        addrs: Vec::new(),
    };
    restore_entry_address(&mut other, peer, &address);
    assert_eq!(other.addrs, Vec::<Multiaddr>::new());

    let existing: Multiaddr = "/ip4/127.0.0.1/tcp/4002".parse().expect("address");
    entry.addrs = vec![existing.clone()];
    restore_entry_address(&mut entry, peer, &address);
    assert_eq!(entry.addrs, vec![existing]);
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
