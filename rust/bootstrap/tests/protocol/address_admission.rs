//! Real TCP checks for Kademlia address admission at a bootstrap node.

use super::*;
use libp2p::kad;
use libp2p::multiaddr::Protocol;

async fn providers(
    observer: &mut Swarm<Behaviour>,
    key: kad::RecordKey,
    search: bool,
) -> std::collections::HashSet<PeerId> {
    let id = if search {
        observer.behaviour_mut().search_kad.get_providers(key)
    } else {
        observer.behaviour_mut().kad.get_providers(key)
    };
    let mut found = std::collections::HashSet::new();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let event = match observer.select_next_some().await {
                SwarmEvent::Behaviour(BehaviourEvent::Kad(
                    kad::Event::OutboundQueryProgressed {
                        id: got,
                        result: kad::QueryResult::GetProviders(result),
                        step,
                        ..
                    },
                )) if !search && got == id => Some((result, step.last)),
                SwarmEvent::Behaviour(BehaviourEvent::SearchKad(
                    kad::Event::OutboundQueryProgressed {
                        id: got,
                        result: kad::QueryResult::GetProviders(result),
                        step,
                        ..
                    },
                )) if search && got == id => Some((result, step.last)),
                _ => None,
            };
            if let Some((result, last)) = event {
                if let Ok(kad::GetProvidersOk::FoundProviders { providers, .. }) = result {
                    found.extend(providers);
                }
                if last {
                    break;
                }
            }
        }
    })
    .await
    .expect("provider lookup timed out");
    found
}

async fn closest(
    observer: &mut Swarm<Behaviour>,
    target: PeerId,
    search: bool,
) -> Vec<kad::PeerInfo> {
    let id = if search {
        observer
            .behaviour_mut()
            .search_kad
            .get_closest_peers(target)
    } else {
        observer.behaviour_mut().kad.get_closest_peers(target)
    };
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let result = match observer.select_next_some().await {
                SwarmEvent::Behaviour(BehaviourEvent::Kad(
                    kad::Event::OutboundQueryProgressed {
                        id: got,
                        result: kad::QueryResult::GetClosestPeers(result),
                        step,
                        ..
                    },
                )) if !search && got == id && step.last => Some(result),
                SwarmEvent::Behaviour(BehaviourEvent::SearchKad(
                    kad::Event::OutboundQueryProgressed {
                        id: got,
                        result: kad::QueryResult::GetClosestPeers(result),
                        step,
                        ..
                    },
                )) if search && got == id && step.last => Some(result),
                _ => None,
            };
            if let Some(result) = result {
                return result.expect("closest-peer query failed").peers;
            }
        }
    })
    .await
    .expect("closest-peer query timed out")
}

#[tokio::test]
async fn foreign_identify_address_stays_out_of_both_dhts() {
    let (address, node_peer) = start_dht_node().await;
    let fake: Multiaddr = "/ip4/203.0.113.7/tcp/9".parse().unwrap();
    let advertiser_key = Keypair::generate_ed25519();
    let advertiser_peer = advertiser_key.public().to_peer_id();
    let fake_routed = fake.clone().with(Protocol::P2p(advertiser_peer));
    let mut advertiser = dyapp_p2p_net::build_configured_swarm(
        advertiser_key,
        dyapp_p2p_net::SwarmOptions {
            mode: Mode::Auto,
            search: true,
            ..Default::default()
        },
    )
    .unwrap();
    advertiser
        .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .unwrap();
    loop {
        if matches!(
            advertiser.select_next_some().await,
            SwarmEvent::NewListenAddr { .. }
        ) {
            break;
        }
    }
    advertiser
        .listen_on("/ip4/127.0.0.2/tcp/0".parse().unwrap())
        .unwrap();
    let alternative = loop {
        if let SwarmEvent::NewListenAddr { address, .. } = advertiser.select_next_some().await {
            break address;
        }
    };
    let alternative = alternative.with(Protocol::P2p(advertiser_peer));
    advertiser.add_external_address(fake.clone());
    advertiser.dial(address.clone()).unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let SwarmEvent::ConnectionEstablished { peer_id, .. } =
                advertiser.select_next_some().await
            {
                assert_eq!(peer_id, node_peer);
                break;
            }
        }
    })
    .await
    .expect("advertiser did not connect");
    let advertiser_task = tokio::spawn(async move {
        loop {
            advertiser.select_next_some().await;
        }
    });

    let mut observer =
        dyapp_p2p_net::build_swarm(Keypair::generate_ed25519(), Mode::Client).unwrap();
    observer.dial(address.clone()).unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let SwarmEvent::ConnectionEstablished { peer_id, .. } =
                observer.select_next_some().await
            {
                assert_eq!(peer_id, node_peer);
                break;
            }
        }
    })
    .await
    .expect("observer did not connect");
    observer
        .behaviour_mut()
        .kad
        .add_address(&node_peer, address.clone());
    observer
        .behaviour_mut()
        .search_kad
        .add_address(&node_peer, address);
    for search in [false, true] {
        let mut last = Vec::new();
        let result = tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                let peers = closest(&mut observer, advertiser_peer, search).await;
                let Some(advertised) = peers.iter().find(|peer| peer.peer_id == advertiser_peer)
                else {
                    continue;
                };
                assert!(
                    !advertised.addrs.contains(&fake_routed),
                    "foreign identify address entered a DHT"
                );
                last = advertised.addrs.clone();
                if last.contains(&alternative) {
                    break;
                }
            }
        })
        .await;
        assert!(
            result.is_ok(),
            "alternative {alternative} missing from {last:?} (search={search})"
        );
    }
    let found = providers(
        &mut observer,
        kad::RecordKey::new(&dyapp_p2p_net::SEARCH_KEY),
        false,
    )
    .await;
    assert!(
        found.contains(&node_peer),
        "local search provider not discoverable"
    );
    advertiser_task.abort();
}

async fn start_dht_node() -> (Multiaddr, PeerId) {
    dyapp_p2p_net::set_id_pow_bits(0);
    let dir = format!("/tmp/ai/test-address-admission-{}", uuid::Uuid::new_v4());
    let mut config = NodeConfig::default();
    config.storage.dir = dir.clone().into();
    config.network.id_pow_bits = 0;
    let key = Keypair::generate_ed25519();
    let peer = key.public().to_peer_id();
    let mut service = Service::new(BootstrapStore::new(&dir).unwrap(), config);
    if service.config.roles.contains(&Role::Search) {
        service.search = Some(std::sync::Arc::new(
            SearchIndex::open(
                &service.config.storage.search_path(),
                &service.config.search,
            )
            .unwrap(),
        ));
    }
    let mut swarm = node::swarm(key, &service.config.limits, &service.config.roles).unwrap();
    swarm
        .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .unwrap();
    let address = loop {
        if let SwarmEvent::NewListenAddr { address, .. } = swarm.select_next_some().await {
            break address;
        }
    };
    // An explicitly confirmed local address makes Auto-mode Kademlia answer test queries.
    swarm.add_external_address(address.clone());
    tokio::spawn(node::run(swarm, std::sync::Arc::new(service)));
    (address, peer)
}

#[tokio::test]
async fn foreign_provider_address_is_not_returned_to_a_third_peer() {
    let (address, node_peer) = start_dht_node().await;
    let fake: Multiaddr = "/ip4/203.0.113.7/tcp/9".parse().unwrap();
    let key = kad::RecordKey::new(&b"foreign-provider");
    let attacker_key = Keypair::generate_ed25519();
    let attacker_peer = attacker_key.public().to_peer_id();
    let mut attacker = dyapp_p2p_net::build_swarm(attacker_key, Mode::Client).unwrap();
    attacker.add_external_address(fake);
    attacker.dial(address.clone()).unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let SwarmEvent::ConnectionEstablished { peer_id, .. } =
                attacker.select_next_some().await
            {
                assert_eq!(peer_id, node_peer);
                break;
            }
        }
    })
    .await
    .expect("attacker did not connect");
    attacker
        .behaviour_mut()
        .kad
        .add_address(&node_peer, address.clone());
    attacker
        .behaviour_mut()
        .search_kad
        .add_address(&node_peer, address.clone());
    let announce = attacker
        .behaviour_mut()
        .kad
        .start_providing(key.clone())
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let SwarmEvent::Behaviour(BehaviourEvent::Kad(
                kad::Event::OutboundQueryProgressed {
                    id,
                    result: kad::QueryResult::StartProviding(result),
                    step,
                    ..
                },
            )) = attacker.select_next_some().await
            {
                if id == announce && step.last {
                    result.expect("provider announcement failed");
                    break;
                }
            }
        }
    })
    .await
    .expect("provider announcement timed out");
    let search_announce = attacker
        .behaviour_mut()
        .search_kad
        .start_providing(key.clone())
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let SwarmEvent::Behaviour(BehaviourEvent::SearchKad(
                kad::Event::OutboundQueryProgressed {
                    id,
                    result: kad::QueryResult::StartProviding(result),
                    step,
                    ..
                },
            )) = attacker.select_next_some().await
            {
                if id == search_announce && step.last {
                    result.expect("search provider announcement failed");
                    break;
                }
            }
        }
    })
    .await
    .expect("search provider announcement timed out");

    let mut observer =
        dyapp_p2p_net::build_swarm(Keypair::generate_ed25519(), Mode::Client).unwrap();
    observer.dial(address.clone()).unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let SwarmEvent::ConnectionEstablished { peer_id, .. } =
                observer.select_next_some().await
            {
                assert_eq!(peer_id, node_peer);
                break;
            }
        }
    })
    .await
    .expect("observer did not connect");
    observer
        .behaviour_mut()
        .kad
        .add_address(&node_peer, address.clone());
    observer
        .behaviour_mut()
        .search_kad
        .add_address(&node_peer, address);
    for search in [false, true] {
        let found = providers(&mut observer, key.clone(), search).await;
        assert!(
            !found.contains(&attacker_peer),
            "foreign provider address leaked (search={search})"
        );
        assert!(
            found.is_empty(),
            "unexpected provider for a unique test key"
        );
    }
}
