use super::*;

async fn exchange_relayed_request(
    relay: &mut Swarm<Behaviour>,
    a: &mut Swarm<Behaviour>,
    b: &mut Swarm<Behaviour>,
    circuit: Multiaddr,
    a_id: PeerId,
) -> bool {
    let (mut dialled, mut relayed, mut answered) = (false, false, false);
    tokio::time::timeout(Duration::from_secs(20), async {
        while !answered {
            tokio::select! {
                event = relay.select_next_some() => { let _ = event; }
                event = a.select_next_some() => match event {
                    SwarmEvent::NewListenAddr { address, .. } if !dialled => {
                        assert!(address.iter().any(|p| p == Protocol::P2pCircuit), "{address}");
                        b.dial(circuit.clone().with(Protocol::P2p(a_id))).unwrap();
                        dialled = true;
                    }
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
                    _ => {}
                },
            }
        }
    })
    .await
    .unwrap();
    relayed
}

#[tokio::test]
async fn peers_without_direct_listeners_exchange_a_request_through_a_relay() {
    let make = |mode, relay| {
        build_configured_swarm(
            Keypair::generate_ed25519(),
            SwarmOptions {
                mode,
                relay,
                ..Default::default()
            },
        )
        .unwrap()
    };
    let mut relay = make(Mode::Auto, Some(libp2p::relay::Config::default()));
    let mut a = make(Mode::Auto, None);
    let mut b = make(Mode::Client, None);
    relay
        .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .unwrap();
    let address = loop {
        if let SwarmEvent::NewListenAddr { address, .. } = relay.select_next_some().await {
            break address.with(Protocol::P2p(*relay.local_peer_id()));
        }
    };
    // The relay announces this address in reservations.
    relay.add_external_address(address.clone());
    let circuit = address.with(Protocol::P2pCircuit);
    a.listen_on(circuit.clone()).unwrap();
    let a_id = *a.local_peer_id();
    assert!(
        exchange_relayed_request(&mut relay, &mut a, &mut b, circuit, a_id).await,
        "the first connection to a peer without listeners runs through the circuit"
    );
}
