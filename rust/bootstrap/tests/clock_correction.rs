//! Client clock correction through real startup checks and signed node requests.

use dyapp_bootstrap::config::{Limits, Role};
use dyapp_bootstrap::search::SearchIndex;
use dyapp_bootstrap::service::Service;
use dyapp_bootstrap::{node, BootstrapStore, NodeConfig};
use dyapp_client::media_ownership::{Intent, Replaced};
use dyapp_client::{attachment, search, seeds, Node};
use dyapp_identity::{network::Network, Identity};
use dyapp_p2p_net::{DialFilter, Protocols};
use dyapp_profile::Profile;
use libp2p::futures::StreamExt;
use libp2p::identity::Keypair;
use libp2p::multiaddr::Protocol;
use libp2p::swarm::SwarmEvent;
use libp2p::Multiaddr;
use std::sync::Arc;
use std::time::Duration;

fn config(dir: &std::path::Path) -> NodeConfig {
    let mut config = NodeConfig::default();
    config.storage.dir = dir.to_path_buf();
    config.network.id_pow_bits = 0;
    config.search.min_profile_pow_bits = 0;
    config.search.min_heartbeat_pow_bits = 0;
    config.roles = vec![Role::Store, Role::Media, Role::Search];
    config.limits = Limits {
        requests_per_second: 1_000,
        media_requests_per_second: 100,
        min_free_mb: 0,
        ..Default::default()
    };
    config
}

async fn start() -> Multiaddr {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let dir = test_dir.to_path_buf();
    let config = config(&dir);
    let keypair = Keypair::generate_ed25519();
    let peer = keypair.public().to_peer_id();
    let mut service =
        Service::new(BootstrapStore::new(dir.to_str().unwrap()).unwrap(), config).unwrap();
    service.protocols = Arc::new(Protocols::public());
    service.media = Some(dyapp_bootstrap::media::MediaStore::open(&dir).unwrap());
    service.search = Some(Arc::new(
        SearchIndex::open(
            &service.config.storage.search_path(),
            &service.config.search,
        )
        .unwrap(),
    ));
    let mut swarm = Box::new(
        node::swarm_with_protocols(
            keypair,
            &service.config.limits,
            &service.config.roles,
            service.wire.clone(),
            service.protocols.clone(),
            false,
            DialFilter::default(),
            service.config.relay_hop(),
        )
        .unwrap(),
    );
    swarm
        .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .unwrap();
    let address = loop {
        if let SwarmEvent::NewListenAddr { address, .. } = swarm.select_next_some().await {
            break address;
        }
    };
    swarm.add_external_address(address.clone());
    tokio::spawn(async move {
        let _test_dir = test_dir;
        Box::pin(node::run(*swarm, Arc::new(service))).await;
    });
    address.with(Protocol::P2p(peer))
}

fn seed_row(address: &Multiaddr) -> String {
    let host = address.iter().find_map(|part| match part {
        Protocol::Ip4(host) => Some(host.to_string()),
        _ => None,
    });
    let port = address.iter().find_map(|part| match part {
        Protocol::Tcp(port) => Some(port),
        _ => None,
    });
    let peer = address.iter().find_map(|part| match part {
        Protocol::P2p(peer) => Some(peer),
        _ => None,
    });
    format!("{}:{}/{}", host.unwrap(), port.unwrap(), peer.unwrap())
}

async fn startup(network: &Network, local_time: u64) {
    let rows = seeds::check(network, Duration::from_secs(5), local_time)
        .await
        .unwrap();
    assert_eq!(rows.len(), network.seeds.len());
    assert!(
        rows.iter()
            .all(|row| matches!(row, seeds::Row::Connected(_))),
        "{rows:?}"
    );
}

async fn connect(addresses: &[Multiaddr]) -> Vec<Node> {
    let protocols = Arc::new(Protocols::public());
    let mut nodes = Vec::new();
    for address in addresses {
        nodes.push(
            Node::connect(address.clone(), Arc::clone(&protocols))
                .await
                .unwrap(),
        );
    }
    nodes
}

async fn collect_invalid_hints(nodes: &mut [Node], owner: &Identity, local_time: u64) {
    for (index, node) in nodes.iter_mut().enumerate() {
        let rejection = node
            .keep_media(owner, 1, Vec::new(), local_time.saturating_add(7_200))
            .await;
        if index < 2 {
            assert!(matches!(rejection, Err(dyapp_client::Error::Clock)));
        } else {
            assert!(matches!(
                rejection,
                Err(dyapp_client::Error::Status(
                    dyapp_p2p_net::proto::Status::Invalid
                ))
            ));
        }
    }
}

async fn corrected_requests_succeed(nodes: &mut [Node], owner: &Identity, local_time: u64) {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    nodes[0]
        .keep_media(owner, 1, Vec::new(), local_time)
        .await
        .unwrap();
    let state = nodes[0].media_keep_state(owner, local_time).await.unwrap();
    let replaced = nodes[0]
        .replace_media_keep(
            owner,
            &state,
            Vec::new(),
            Intent {
                profile_hash: [0; 32],
                pending: false,
            },
            local_time,
        )
        .await
        .unwrap();
    assert!(matches!(replaced, Replaced::Applied { .. }));
    let (sealed, _) = attachment::seal(b"clock-corrected").unwrap();
    nodes[0]
        .attach_media(owner, &sealed, local_time)
        .await
        .unwrap();
    nodes[0]
        .search_publish(
            Profile {
                version: 1,
                ..Default::default()
            }
            .sign(owner),
        )
        .await
        .unwrap();
    let store_dir = test_dir.to_path_buf();
    std::fs::create_dir_all(&store_dir).unwrap();
    let store = dyapp_client::store::Store::open(&store_dir.join("client.db"), owner).unwrap();
    assert!(search::beat_due(&store, owner, 1, 0, nodes, local_time)
        .await
        .unwrap());
}

#[tokio::test]
async fn startup_median_repairs_all_signed_retention_times() {
    let addresses = [start().await, start().await, start().await];
    let local_time = dyapp_client::now().saturating_sub(3_600);
    let network = |count: usize| Network {
        seeds: addresses[..count].iter().map(seed_row).collect(),
        name: String::new(),
    };
    startup(&network(2), local_time).await;
    assert!(!dyapp_client::clock::has_median());

    let mut nodes = connect(&addresses).await;
    let owner = Identity::try_generate().unwrap();
    collect_invalid_hints(&mut nodes, &owner, local_time).await;
    assert!(dyapp_client::clock::has_median());
    assert_eq!(dyapp_client::clock::corrected(local_time), local_time);

    let refresh_started = dyapp_client::now();
    startup(&network(3), local_time).await;
    let refresh_finished = dyapp_client::now();
    assert!(dyapp_client::clock::has_median());
    let corrected = dyapp_client::clock::corrected(local_time);
    assert!((refresh_started..=refresh_finished).contains(&corrected));
    corrected_requests_succeed(&mut nodes, &owner, local_time).await;
}
