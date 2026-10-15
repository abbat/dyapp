use super::*;

#[test]
fn seeds_can_pin_ipv4_ipv6_and_dns_without_accepting_multiaddrs() {
    let peer = PeerId::random();
    for host in ["127.0.0.1:7070", "[2001:db8::7]:7070", "seed.example:7070"] {
        let plain = seed(host).unwrap();
        let pinned = seed(&format!("{host}/{peer}")).unwrap();
        assert_eq!(
            pinned,
            plain.map(|address| address.with(Protocol::P2p(peer)))
        );
    }
    for bad in [
        "seed.example:7070/",
        "seed.example:7070/not-a-peer-id",
        "/dns/seed.example/tcp/7070",
        "/ip4/127.0.0.1/tcp/7070",
    ] {
        assert!(seed(bad).is_err(), "{bad}");
    }
    assert!(seed(&format!("seed.example:7070/{peer}/extra")).is_err());
}

#[tokio::test]
async fn seed_pins_refuse_the_wrong_transport_key_and_plain_seeds_still_connect() {
    use libp2p::futures::StreamExt;
    use libp2p::identity::Keypair;
    use libp2p::swarm::{DialError, SwarmEvent};
    use std::time::Duration;
    let mut server =
        dyapp_p2p_net::build_swarm(Keypair::generate_ed25519(), dyapp_p2p_net::Mode::Auto).unwrap();
    let real = *server.local_peer_id();
    server
        .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .unwrap();
    let address = loop {
        if let SwarmEvent::NewListenAddr { address, .. } = server.select_next_some().await {
            break address;
        }
    };
    let port = address
        .iter()
        .find_map(|part| match part {
            Protocol::Tcp(port) => Some(port),
            _ => None,
        })
        .unwrap();
    for expected in [None, Some(real), Some(PeerId::random())] {
        let mut client =
            dyapp_p2p_net::build_swarm(Keypair::generate_ed25519(), dyapp_p2p_net::Mode::Client)
                .unwrap();
        let host = format!("127.0.0.1:{port}");
        let text = expected.map_or(host.clone(), |peer| format!("{host}/{peer}"));
        let seeds = seed(&text).unwrap();
        dyapp_p2p_net::join(&mut client, &seeds, &[]);
        let wrong = expected.is_some_and(|id| id != real);
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                tokio::select! {
                    _ = server.select_next_some() => {}
                    event = client.select_next_some() => match event {
                        SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                            assert!(!wrong, "connected to a different seed key");
                            assert_eq!(peer_id, real);
                            break;
                        }
                        SwarmEvent::OutgoingConnectionError {
                            error: DialError::WrongPeerId { .. }, ..
                        } if wrong => {
                            assert_eq!(client.connected_peers().count(), 0);
                            assert!(client.behaviour_mut().kad.kbuckets().all(|bucket| bucket.num_entries() == 0));
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }).await.expect("seed authentication never resolved");
    }
}
