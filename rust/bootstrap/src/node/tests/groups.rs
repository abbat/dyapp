//! Per-IP-group connection limit tests.
use super::*;

async fn connection_event(
    swarm: &mut Swarm<Behaviour>,
    matches: impl Fn(&SwarmEvent<BehaviourEvent>) -> bool,
) -> SwarmEvent<BehaviourEvent> {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let event = swarm.next().await.unwrap();
            if matches(&event) {
                return event;
            }
        }
    })
    .await
    .unwrap()
}

async fn group_test_node(limits: &Limits) -> (Swarm<Behaviour>, Multiaddr) {
    let mut node = swarm(Keypair::generate_ed25519(), limits, &[Role::Store]).unwrap();
    node.listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .unwrap();
    let SwarmEvent::NewListenAddr { address, .. } =
        connection_event(&mut node, |e| matches!(e, SwarmEvent::NewListenAddr { .. })).await
    else {
        unreachable!()
    };
    (node, address)
}

fn group_test_client(address: Multiaddr, key: Keypair) -> (PeerId, tokio::task::JoinHandle<()>) {
    let mut client = dyapp_p2p_net::build_swarm(key, Mode::Client).unwrap();
    let id = *client.local_peer_id();
    client.dial(address).unwrap();
    let task = tokio::spawn(async move { while client.next().await.is_some() {} });
    (id, task)
}

async fn stop_group_client(task: tokio::task::JoinHandle<()>) {
    task.abort();
    let _ = task.await;
}

#[tokio::test]
async fn incoming_group_caps_pending_tcp_and_releases_failed_handshakes() {
    let limits = Limits {
        max_memory_mb: 0,
        ..Limits::default()
    };
    let (mut node, address) = group_test_node(&limits).await;
    let port = address
        .iter()
        .find_map(|p| match p {
            Protocol::Tcp(port) => Some(port),
            _ => None,
        })
        .unwrap();
    let mut sockets = Vec::new();
    let mut ids = Vec::new();
    for _ in 0..32 {
        sockets.push(
            tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .unwrap(),
        );
        let SwarmEvent::IncomingConnection { connection_id, .. } =
            connection_event(&mut node, |e| {
                matches!(e, SwarmEvent::IncomingConnection { .. })
            })
            .await
        else {
            unreachable!()
        };
        ids.push(connection_id);
    }
    let refused = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap();
    connection_event(&mut node, |e| {
        matches!(
            e,
            SwarmEvent::IncomingConnectionError {
                error: ListenError::Denied { .. },
                ..
            }
        )
    })
    .await;
    assert_eq!(
        node.behaviour_mut()
            .incoming_groups
            .as_mut()
            .unwrap()
            .take_denied(),
        1
    );
    drop(refused);

    drop(sockets.pop());
    let released = ids.pop().unwrap();
    connection_event(&mut node, |e| {
        matches!(e,
        SwarmEvent::IncomingConnectionError { connection_id, .. } if *connection_id == released)
    })
    .await;
    let replacement = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap();
    connection_event(&mut node, |e| {
        matches!(e, SwarmEvent::IncomingConnection { .. })
    })
    .await;
    drop(replacement);
    drop(sockets);
    for _ in 0..32 {
        connection_event(&mut node, |e| {
            matches!(e, SwarmEvent::IncomingConnectionError { .. })
        })
        .await;
    }
    let guard = node.behaviour().incoming_groups.as_ref().unwrap();
    assert_eq!((guard.active_connections(), guard.active_groups()), (0, 0));
    let (_, client) = group_test_client(address, Keypair::generate_ed25519());
    connection_event(&mut node, |e| {
        matches!(e, SwarmEvent::ConnectionEstablished { .. })
    })
    .await;
    stop_group_client(client).await;
}

#[tokio::test]
async fn incoming_group_caps_established_tcp_but_exempts_outgoing_and_releases_close() {
    let limits = Limits {
        max_memory_mb: 0,
        ..Limits::default()
    };
    let (mut node, address) = group_test_node(&limits).await;
    let mut clients = Vec::new();
    for _ in 0..32 {
        let (id, client) = group_test_client(address.clone(), Keypair::generate_ed25519());
        let event = connection_event(&mut node, |e| {
            matches!(e,
            SwarmEvent::ConnectionEstablished { peer_id, .. } if *peer_id == id)
        })
        .await;
        assert!(
            matches!(event, SwarmEvent::ConnectionEstablished { endpoint, .. } if endpoint.is_listener())
        );
        clients.push(client);
    }
    let (_, refused) = group_test_client(address.clone(), Keypair::generate_ed25519());
    connection_event(&mut node, |e| {
        matches!(
            e,
            SwarmEvent::IncomingConnectionError {
                error: ListenError::Denied { .. },
                ..
            }
        )
    })
    .await;
    stop_group_client(refused).await;
    assert_eq!(
        node.behaviour_mut()
            .incoming_groups
            .as_mut()
            .unwrap()
            .take_denied(),
        1
    );

    // A node can still make an outgoing connection while its inbound group is full.
    let (mut other, other_address) = group_test_node(&limits).await;
    let other_id = *other.local_peer_id();
    let other_task = tokio::spawn(async move { while other.next().await.is_some() {} });
    node.dial(other_address).unwrap();
    let event = connection_event(&mut node, |e| {
        matches!(e,
        SwarmEvent::ConnectionEstablished { peer_id, .. } if *peer_id == other_id)
    })
    .await;
    assert!(
        matches!(event, SwarmEvent::ConnectionEstablished { endpoint, .. } if endpoint.is_dialer())
    );
    assert_eq!(node.network_info().num_peers(), 33);

    stop_group_client(clients.pop().unwrap()).await;
    connection_event(&mut node, |e| {
        matches!(e, SwarmEvent::ConnectionClosed { .. })
    })
    .await;
    let (id, replacement) = group_test_client(address, Keypair::generate_ed25519());
    connection_event(&mut node, |e| {
        matches!(e,
        SwarmEvent::ConnectionEstablished { peer_id, .. } if *peer_id == id)
    })
    .await;
    assert_eq!(
        node.behaviour_mut()
            .incoming_groups
            .as_mut()
            .unwrap()
            .take_denied(),
        0
    );
    clients.push(replacement);
    for client in clients {
        stop_group_client(client).await;
    }
    stop_group_client(other_task).await;
}

#[tokio::test]
async fn quic_churn_releases_incoming_groups() {
    let limits = Limits {
        max_connections_per_ip_group: 1,
        max_memory_mb: 0,
        ..Limits::default()
    };
    let mut node = swarm(Keypair::generate_ed25519(), &limits, &[Role::Store]).unwrap();
    node.listen_on("/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap())
        .unwrap();
    let SwarmEvent::NewListenAddr { address, .. } =
        connection_event(&mut node, |e| matches!(e, SwarmEvent::NewListenAddr { .. })).await
    else {
        unreachable!()
    };
    for _ in 0..100 {
        let (id, client) = group_test_client(address.clone(), Keypair::generate_ed25519());
        connection_event(&mut node, |e| {
            matches!(e,
            SwarmEvent::ConnectionEstablished { peer_id, .. } if *peer_id == id)
        })
        .await;
        let guard = node.behaviour().incoming_groups.as_ref().unwrap();
        assert_eq!((guard.active_connections(), guard.active_groups()), (1, 1));
        stop_group_client(client).await;
        connection_event(&mut node, |e| {
            matches!(e,
            SwarmEvent::ConnectionClosed { peer_id, .. } if *peer_id == id)
        })
        .await;
        let guard = node.behaviour().incoming_groups.as_ref().unwrap();
        assert_eq!((guard.active_connections(), guard.active_groups()), (0, 0));
    }
    assert_eq!(
        node.behaviour_mut()
            .incoming_groups
            .as_mut()
            .unwrap()
            .take_denied(),
        0
    );
}

#[tokio::test]
async fn composed_peer_limit_refusal_does_not_leak_incoming_group_reservation() {
    let limits = Limits {
        max_connections_per_ip_group: 2,
        max_connections_per_peer: 1,
        max_memory_mb: 0,
        ..Limits::default()
    };
    let (mut node, address) = group_test_node(&limits).await;
    let key = Keypair::generate_ed25519();
    let (id, first) = group_test_client(address.clone(), key.clone());
    connection_event(&mut node, |e| {
        matches!(e,
        SwarmEvent::ConnectionEstablished { peer_id, .. } if *peer_id == id)
    })
    .await;
    let (_, duplicate) = group_test_client(address.clone(), key);
    connection_event(&mut node, |e| {
        matches!(
            e,
            SwarmEvent::IncomingConnectionError {
                error: ListenError::Denied { .. },
                ..
            }
        )
    })
    .await;
    assert_eq!(
        node.behaviour_mut()
            .incoming_groups
            .as_mut()
            .unwrap()
            .take_denied(),
        0
    );
    stop_group_client(duplicate).await;
    let (id, second) = group_test_client(address, Keypair::generate_ed25519());
    connection_event(&mut node, |e| {
        matches!(e,
        SwarmEvent::ConnectionEstablished { peer_id, .. } if *peer_id == id)
    })
    .await;
    assert_eq!(node.network_info().num_peers(), 2);
    stop_group_client(first).await;
    stop_group_client(second).await;
}
