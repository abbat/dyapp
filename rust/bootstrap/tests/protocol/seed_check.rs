use super::*;
use dyapp_client::seeds::{check, Row};
use dyapp_identity::network::Network;
use libp2p::multiaddr::Protocol;

#[tokio::test]
async fn client_seed_check_names_a_real_node_of_each_network() {
    for (node_name, client_name, joins) in [
        ("", "", true),
        ("", "city", false),
        ("city", "", false),
        ("city", "city", true),
    ] {
        let protocols = std::sync::Arc::new(dyapp_p2p_net::Protocols::new(node_name).unwrap());
        let (address, _, _) = start_config_in_protocols(|_| {}, protocols).await;
        let (mut port, mut peer) = (0, None);
        for part in address.iter() {
            match part {
                Protocol::Tcp(p) => port = p,
                Protocol::P2p(id) => peer = Some(id),
                _ => {}
            }
        }
        let peer = peer.unwrap();
        let closed = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let network = Network {
            seeds: vec![
                format!("127.0.0.1:{port}"),
                format!("127.0.0.1:{port}/{peer}"),
                format!("127.0.0.1:{port}/{}", PeerId::random()),
                format!("127.0.0.1:{closed}"),
            ],
            name: client_name.into(),
        };
        let rows = check(&network, Duration::from_secs(5)).await.unwrap();
        let reached = if joins {
            Row::Connected(peer)
        } else {
            Row::WrongNetwork
        };
        let case = format!("node {node_name:?}, client {client_name:?}");
        assert_eq!(rows[0], reached, "{case}");
        assert_eq!(rows[1], reached, "{case}");
        assert_eq!(rows[2], Row::WrongPin, "{case}");
        assert!(
            matches!(rows[3], Row::Unreachable(_)),
            "{case}: {:?}",
            rows[3]
        );
    }
}
