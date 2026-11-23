//! Ordered explicit seed-row connections, independent of discovery and node ownership.

use crate::Node;
use dyapp_identity::network::Network;
use dyapp_p2p_net::{seed, Protocols};
use std::future::Future;
use std::sync::Arc;

/// Why explicit seeds supplied no usable connection.
#[derive(Debug, thiserror::Error)]
pub enum ConnectError {
    /// Invalid network protocol configuration.
    #[error("{0}")]
    Configuration(String),
    /// No valid seed address was available.
    #[error("notConnected")]
    NotConnected,
    /// The final attempted address failed.
    #[error("{0}")]
    Dial(String),
}

#[derive(Clone, Copy)]
enum Selection {
    First,
    PerRow,
}

async fn select<T, E, F, Fut>(
    network: &Network,
    selection: Selection,
    mut dial: F,
) -> (Vec<T>, Option<E>)
where
    F: FnMut(libp2p::Multiaddr) -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let mut nodes = Vec::new();
    let mut last_error = None;
    for row in &network.seeds {
        for address in seed::parse(row.trim()).into_iter().flatten() {
            match dial(address).await {
                Ok(node) => {
                    nodes.push(node);
                    if matches!(selection, Selection::First) {
                        return (nodes, last_error);
                    }
                    break;
                }
                Err(error) => last_error = Some(error),
            }
        }
    }
    (nodes, last_error)
}

/// Connect the first answering address, preserving seed-row and TCP/QUIC order.
///
/// # Errors
/// Returns invalid protocol configuration, the final dial failure, or `notConnected`
/// when no valid address was attempted.
pub async fn first(network: &Network) -> Result<Node, ConnectError> {
    let protocols = Arc::new(
        Protocols::new(&network.name)
            .map_err(|error| ConnectError::Configuration(error.to_string()))?,
    );
    let (mut nodes, last_error) = select(network, Selection::First, |address| {
        Node::connect(address, Arc::clone(&protocols))
    })
    .await;
    nodes.pop().ok_or_else(|| {
        last_error.map_or(ConnectError::NotConnected, |error| {
            ConnectError::Dial(error.to_string())
        })
    })
}

/// Connect at most one answering address per seed row, preserving row order.
/// Invalid rows and failed addresses are skipped; no answering nodes means an empty vector.
///
/// # Errors
/// Returns invalid network protocol configuration.
pub async fn reachable(network: &Network) -> Result<Vec<Node>, ConnectError> {
    let protocols = Arc::new(
        Protocols::new(&network.name)
            .map_err(|error| ConnectError::Configuration(error.to_string()))?,
    );
    let (nodes, _) = select(network, Selection::PerRow, |address| {
        Node::connect(address, Arc::clone(&protocols))
    })
    .await;
    Ok(nodes)
}

#[cfg(test)]
#[path = "seed_connections/tests.rs"]
mod tests;
