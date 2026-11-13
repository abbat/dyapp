//! Native submission and background receive retain an offline operation across reopen.
use super::*;
use libp2p::futures::StreamExt;

async fn listening(swarm: &mut libp2p::Swarm<dyapp_p2p_net::Behaviour>) -> Multiaddr {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let SwarmEvent::NewListenAddr { address, .. } = swarm.select_next_some().await {
                return address;
            }
        }
    })
    .await
    .expect("listener readiness deadline")
}

fn refusing_seed(runtime: &tokio::runtime::Runtime) -> String {
    runtime.block_on(async {
        let mut swarm = dyapp_p2p_net::build_configured_swarm(
            Keypair::generate_ed25519(),
            dyapp_p2p_net::SwarmOptions::default(),
        )
        .unwrap();
        swarm
            .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
            .unwrap();
        let tcp = listening(&mut swarm).await;
        let port = tcp
            .iter()
            .find_map(|part| match part {
                Protocol::Tcp(port) => Some(port),
                _ => None,
            })
            .unwrap();
        swarm
            .listen_on(
                format!("/ip4/127.0.0.1/udp/{port}/quic-v1")
                    .parse()
                    .unwrap(),
            )
            .unwrap();
        let quic = listening(&mut swarm).await;
        assert!(quic
            .iter()
            .any(|part| matches!(part, Protocol::Udp(found) if found == port)));
        let pinned = libp2p::PeerId::random();
        assert_ne!(swarm.local_peer_id(), &pinned);
        tokio::spawn(async move {
            loop {
                swarm.select_next_some().await;
            }
        });
        let seed = format!("127.0.0.1:{port}/{pinned}");
        check_connect_refusals(&seed).await;
        seed
    })
}

// Assert the fixture fails on handshake, never because the network deadline expires.
async fn check_connect_refusals(seed: &str) {
    let protocols = Arc::new(dyapp_p2p_net::Protocols::new("").unwrap());
    for address in dyapp_p2p_net::seed::parse(seed).unwrap() {
        let result = dyapp_client::Node::connect(address, protocols.clone()).await;
        assert!(matches!(result, Err(dyapp_client::Error::Connect(_))));
    }
}

fn set_seed(peer: &NativePeer, seed: String) {
    network::save(
        std::path::Path::new(&peer.dir),
        Settings::Active,
        &Network {
            seeds: vec![seed],
            name: String::new(),
        },
    )
    .unwrap();
}

fn matched(alice: &ChatSession, bob: &ChatSession, alice_id: &str, bob_id: &str) {
    alice.like(bob_id.into(), true).unwrap();
    bob.receive().unwrap();
    bob.like(alice_id.into(), true).unwrap();
    alice.receive().unwrap();
    assert_eq!(alice.state(bob_id.into()).unwrap(), ChatState::Open);
    assert_eq!(bob.state(alice_id.into()).unwrap(), ChatState::Open);
}

fn one_text(chats: &ChatSession, peer: &str, id: &[u8]) {
    let rows = chats.chat_history(peer.into()).unwrap().messages;
    let matching: Vec<_> = rows.iter().filter(|row| row.id == id).collect();
    assert_eq!(matching.len(), 1);
    assert_eq!(matching[0].text.as_deref(), Some("native offline"));
}

#[test]
fn configured_connection_refusal_queues_and_native_receive_delivers_after_reopen() {
    let (runtime, address) = store_node();
    let owner = retained_person(&address);
    let alice = owner.open();
    let (bob_key, bob) = person(&address);
    let (alice_id, bob_id) = (owner.key.peer_id(), bob_key.peer_id());
    matched(&alice, &bob, &alice_id, &bob_id);
    set_seed(&owner, refusing_seed(&runtime));
    let stranger = IdentityKey::generate().peer_id();
    assert!(alice
        .submit_text(stranger.clone(), "first contact".into())
        .is_err());
    assert_eq!(alice.chat_history(stranger).unwrap().messages.len(), 0);
    let result = alice
        .submit_text(bob_id.clone(), "native offline".into())
        .unwrap();
    let dyapp_ffi::ChatSubmission::Queued { id, .. } = result else {
        panic!("connection refusal must retain a queued operation");
    };
    one_text(&alice, &bob_id, &id);
    drop(alice);
    let reopened = owner.open();
    one_text(&reopened, &bob_id, &id);
    set_seed(&owner, seed_row(&address));
    reopened.receive().unwrap();
    bob.receive().unwrap();
    one_text(&bob, &alice_id, &id);
    reopened.receive().unwrap();
    bob.receive().unwrap();
    one_text(&bob, &alice_id, &id);
}
