use super::*;

/// Asks `addr` for a relay slot. The holder swarm comes back once the circuit address is
/// announced, `None` once the reservation is refused; the slot lives as long as the swarm.
async fn reserve(addr: &Multiaddr) -> Option<Swarm<Behaviour>> {
    let mut swarm = dyapp_p2p_net::build_configured_swarm(
        Keypair::generate_ed25519(),
        dyapp_p2p_net::SwarmOptions {
            mode: Mode::Client,
            ..Default::default()
        },
    )
    .unwrap();
    swarm
        .listen_on(addr.clone().with(libp2p::multiaddr::Protocol::P2pCircuit))
        .unwrap();
    let reserved = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            match swarm.select_next_some().await {
                SwarmEvent::NewListenAddr { address, .. } => {
                    let circuit = libp2p::multiaddr::Protocol::P2pCircuit;
                    assert!(address.iter().any(|p| p == circuit), "{address}");
                    return true;
                }
                SwarmEvent::ListenerClosed { reason, .. } => {
                    assert!(reason.is_err());
                    return false;
                }
                _ => {}
            }
        }
    })
    .await
    .unwrap();
    reserved.then_some(swarm)
}

#[tokio::test]
async fn relay_role_hands_out_slots_within_its_limit() {
    let addr = start_config(|c| {
        c.roles = vec![Role::Store, Role::Relay];
        c.relay.max_reservations = 1;
    })
    .await;
    let _slot = reserve(&addr).await.unwrap();
    assert!(
        reserve(&addr).await.is_none(),
        "the second slot is over max_reservations"
    );
    let plain = start_config(|c| c.roles = vec![Role::Store]).await;
    assert!(
        reserve(&plain).await.is_none(),
        "a node without role relay serves no slots"
    );
}
