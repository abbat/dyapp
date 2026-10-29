//! Checks an identity's seed rows before they are activated: which nodes answer on this network.
//!
//! The check dials from a throwaway key, so it never touches or replaces the identity key. It
//! is cancelled by dropping its future and retried by calling it again.

use dyapp_identity::network::Network;
use dyapp_p2p_net::seed::{self, SeedError};
use dyapp_p2p_net::{build_configured_swarm, BehaviourEvent, Mode, Protocols, SwarmOptions};
use libp2p::futures::StreamExt;
use libp2p::identity::Keypair;
use libp2p::multiaddr::Protocol;
use libp2p::swarm::dial_opts::{DialOpts, PeerCondition};
use libp2p::swarm::{ConnectionId, DialError, SwarmEvent};
use libp2p::{identify, Multiaddr, PeerId};
use std::collections::HashMap;
use std::time::Duration;

/// What one seed row turned out to be, in the order the rows were given.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Row {
    /// A node of this network answered.
    Connected(PeerId),
    /// The row is not `host:port[/peer ID]`.
    Invalid(SeedError),
    /// The row names the same addresses as an earlier one.
    Duplicate,
    /// No dial succeeded within the wait; the last error, or empty on timeout.
    Unreachable(String),
    /// A node answered with a different key than the row's pinned peer ID.
    WrongPin,
    /// A node answered from another network name.
    WrongNetwork,
}

/// Why the rows were not dialed at all.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum CheckError {
    /// The list has no rows.
    #[error("no seed rows")]
    NoSeeds,
    /// The network name breaks ADR 0017.
    #[error("{0}")]
    Name(String),
    /// The client swarm could not be built.
    #[error("swarm: {0}")]
    Swarm(String),
}

fn dial_opts(address: Multiaddr) -> DialOpts {
    match address.iter().last() {
        // Always: TCP and QUIC of a pinned row, or two rows pinning one peer, dial in parallel.
        Some(Protocol::P2p(peer)) => DialOpts::peer_id(peer)
            .addresses(vec![address])
            .condition(PeerCondition::Always)
            .build(),
        _ => DialOpts::unknown_peer_id().address(address).build(),
    }
}

/// Dials every row of `network` over TCP and QUIC and reports each within `wait`.
pub async fn check(network: &Network, wait: Duration) -> Result<Vec<Row>, CheckError> {
    if network.seeds.is_empty() {
        return Err(CheckError::NoSeeds);
    }
    let protocols =
        Protocols::new(&network.name).map_err(|error| CheckError::Name(error.to_string()))?;
    let version = protocols.version.clone();
    let options = SwarmOptions {
        mode: Mode::Client,
        protocols: protocols.into(),
        ..SwarmOptions::default()
    };
    let mut swarm = build_configured_swarm(Keypair::generate_ed25519(), options)
        .map_err(|error| CheckError::Swarm(error.to_string()))?;

    let mut rows = Vec::with_capacity(network.seeds.len());
    let mut seen = Vec::new();
    let mut dials = HashMap::new();
    let mut pending = vec![0usize; network.seeds.len()];
    for (index, row) in network.seeds.iter().enumerate() {
        let addresses = match seed::parse(row.trim()) {
            Err(error) => {
                rows.push(Some(Row::Invalid(error)));
                continue;
            }
            Ok(addresses) if seen.contains(&addresses) => {
                rows.push(Some(Row::Duplicate));
                continue;
            }
            Ok(addresses) => addresses,
        };
        seen.push(addresses.clone());
        rows.push(None);
        let mut error = String::new();
        for address in addresses {
            let opts = dial_opts(address);
            let id = opts.connection_id();
            match swarm.dial(opts) {
                Ok(()) => {
                    dials.insert(id, index);
                    pending[index] += 1;
                }
                Err(dial) => error = dial.to_string(),
            }
        }
        if pending[index] == 0 {
            rows[index] = Some(Row::Unreachable(error));
        }
    }

    let decide = |rows: &mut Vec<Option<Row>>, id: &ConnectionId, row: Row| {
        if let Some(&index) = dials.get(id) {
            rows[index].get_or_insert(row);
        }
    };
    let mut errors: HashMap<usize, String> = HashMap::new();
    let _ = tokio::time::timeout(wait, async {
        while rows.iter().any(Option::is_none) {
            match swarm.select_next_some().await {
                // The node's identify names its network; ours goes out at the same time.
                SwarmEvent::Behaviour(BehaviourEvent::Identify(identify::Event::Received {
                    connection_id,
                    peer_id,
                    info,
                })) => {
                    let row = if info.protocol_version == version {
                        Row::Connected(peer_id)
                    } else {
                        Row::WrongNetwork
                    };
                    decide(&mut rows, &connection_id, row);
                }
                SwarmEvent::OutgoingConnectionError {
                    connection_id,
                    error,
                    ..
                } => {
                    let Some(&index) = dials.get(&connection_id) else {
                        continue;
                    };
                    if matches!(error, DialError::WrongPeerId { .. }) {
                        decide(&mut rows, &connection_id, Row::WrongPin);
                        continue;
                    }
                    errors.insert(index, error.to_string());
                    pending[index] -= 1;
                    if pending[index] == 0 {
                        let error = errors.remove(&index).unwrap_or_default();
                        decide(&mut rows, &connection_id, Row::Unreachable(error));
                    }
                }
                _ => {}
            }
        }
    })
    .await;
    Ok(rows
        .into_iter()
        .enumerate()
        .map(|(index, row)| {
            row.unwrap_or_else(|| Row::Unreachable(errors.remove(&index).unwrap_or_default()))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[tokio::test]
    async fn rows_are_classified_and_a_foreign_network_is_named() {
        let options = SwarmOptions {
            mode: Mode::Auto,
            ..SwarmOptions::default()
        };
        let mut server = build_configured_swarm(Keypair::generate_ed25519(), options).unwrap();
        let real = *server.local_peer_id();
        server
            .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
            .unwrap();
        let port = loop {
            if let SwarmEvent::NewListenAddr { address, .. } = server.select_next_some().await {
                break address
                    .iter()
                    .find_map(|part| match part {
                        Protocol::Tcp(port) => Some(port),
                        _ => None,
                    })
                    .unwrap();
            }
        };
        let server = tokio::spawn(async move {
            loop {
                server.select_next_some().await;
            }
        });
        let closed = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let live = format!("127.0.0.1:{port}");
        let network = |name: &str| Network {
            seeds: vec![
                live.clone(),
                format!("127.0.0.1:{closed}"),
                format!("{live}/{}", PeerId::random()),
                format!("{live}/{real}"),
                "/ip4/127.0.0.1/tcp/1".into(),
                format!(" {live} "),
            ],
            name: name.into(),
        };
        // A QUIC dial to the closed port may only end with the wait: still unreachable.
        let wait = Duration::from_secs(5);

        let rows = check(&network(""), wait).await.unwrap();
        assert_eq!(rows[0], Row::Connected(real));
        assert!(matches!(rows[1], Row::Unreachable(_)), "{:?}", rows[1]);
        assert_eq!(rows[2], Row::WrongPin);
        assert_eq!(rows[3], Row::Connected(real));
        assert_eq!(rows[4], Row::Invalid(SeedError::Address));
        assert_eq!(rows[5], Row::Duplicate);

        let rows = check(&network("city"), wait).await.unwrap();
        assert_eq!(rows[0], Row::WrongNetwork);
        assert_eq!(rows[2], Row::WrongPin);
        assert_eq!(rows[3], Row::WrongNetwork);

        assert_eq!(
            check(&Network::default(), wait).await,
            Err(CheckError::NoSeeds)
        );
        let bad_name = Network {
            seeds: vec![live],
            name: " city".into(),
        };
        assert!(matches!(
            check(&bad_name, wait).await,
            Err(CheckError::Name(_))
        ));
        server.abort();
    }
}
