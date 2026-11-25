//! Ordered explicit seed-row connections with bounded, untrusted cached fallback.

use crate::node_cache::Cache;
use crate::{Error, Node, OPERATION_TIMEOUT};
use dyapp_identity::network::Network;
use dyapp_p2p_net::{seed, Protocols};
use std::future::Future;
use std::path::Path;
use std::sync::Arc;

const MAX_CACHED_ATTEMPTS: usize = 4;
const MAX_CACHED_REACHABLE: usize = 3;

/// Return the installation root for a legacy or `profiles/<uuid>` identity directory.
#[must_use]
pub fn installation_root(dir: &Path) -> &Path {
    let is_uuid = dir
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.len() == 36
                && name.bytes().enumerate().all(|(index, byte)| {
                    matches!(index, 8 | 13 | 18 | 23) && byte == b'-'
                        || !matches!(index, 8 | 13 | 18 | 23) && byte.is_ascii_hexdigit()
                })
        });
    if is_uuid
        && dir
            .parent()
            .and_then(|parent| parent.file_name())
            .is_some_and(|name| name == "profiles")
    {
        if let Some(root) = dir.parent().and_then(Path::parent) {
            return root;
        }
    }
    dir
}

/// Why explicit seeds and cached candidates supplied no usable connection.
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

#[cfg(test)]
async fn fallback_candidates<T, E, F, Fut>(
    candidates: Vec<crate::node_cache::Candidate>,
    limit: usize,
    mut dial: F,
) -> (Vec<T>, Option<E>)
where
    F: FnMut(crate::node_cache::Candidate) -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let mut nodes = Vec::new();
    let mut last_error = None;
    for candidate in candidates.into_iter().take(MAX_CACHED_ATTEMPTS) {
        match dial(candidate).await {
            Ok(node) => {
                nodes.push(node);
                if nodes.len() == limit {
                    break;
                }
            }
            Err(error) => last_error = Some(error),
        }
    }
    (nodes, last_error)
}

fn protocols(network: &Network) -> Result<Arc<Protocols>, ConnectError> {
    Protocols::new(&network.name)
        .map(Into::into)
        .map_err(|error| ConnectError::Configuration(error.to_string()))
}

async fn connect_seed(
    address: libp2p::Multiaddr,
    protocols: Arc<Protocols>,
    cache: Option<&Cache>,
) -> Result<Node, Error> {
    Node::connect_optional_cache(address, protocols, cache.cloned()).await
}

async fn fallback(
    cache: &Cache,
    protocols: Arc<Protocols>,
    limit: usize,
) -> (Vec<Node>, Option<Error>) {
    let candidates = cache.candidates();
    let mut nodes = Vec::new();
    let mut last_error = None;
    let result = tokio::time::timeout(OPERATION_TIMEOUT, async {
        for candidate in candidates.into_iter().take(MAX_CACHED_ATTEMPTS) {
            let reached = Node::connect_with_cache(
                candidate.address,
                Arc::clone(&protocols),
                cache.clone(),
                true,
            )
            .await;
            match reached {
                Ok(node) if node.peer() == candidate.peer => {
                    nodes.push(node);
                    if nodes.len() == limit {
                        return;
                    }
                }
                Ok(_) => last_error = Some(Error::Connect("wrong cached peer".into())),
                Err(error) => last_error = Some(error),
            }
        }
    })
    .await;
    if result.is_err() {
        last_error = Some(Error::Timeout);
    }
    (nodes, last_error)
}

fn no_connection(error: Option<Error>) -> ConnectError {
    error.map_or(ConnectError::NotConnected, |error| {
        ConnectError::Dial(error.to_string())
    })
}

async fn first_inner(network: &Network, cache: Option<Cache>) -> Result<Node, ConnectError> {
    let protocols = protocols(network)?;
    let (mut nodes, mut last_error) = select(network, Selection::First, |address| {
        connect_seed(address, Arc::clone(&protocols), cache.as_ref())
    })
    .await;
    if let Some(node) = nodes.pop() {
        return Ok(node);
    }
    if let Some(cache) = cache {
        let (mut cached, error) = fallback(&cache, protocols, 1).await;
        if let Some(node) = cached.pop() {
            return Ok(node);
        }
        last_error = error.or(last_error);
    }
    Err(no_connection(last_error))
}

/// Connect the first answering seed without a fallback cache.
///
/// # Errors
/// Returns invalid protocol configuration, the final dial failure, or `notConnected`.
pub async fn first(network: &Network) -> Result<Node, ConnectError> {
    first_inner(network, None).await
}

/// Connect a seed, falling back to the installation cache only after every seed fails.
///
/// # Errors
/// Returns invalid protocol configuration, the final dial failure, or `notConnected`.
pub async fn first_cached(network: &Network, root: &Path) -> Result<Node, ConnectError> {
    let cache = Cache::at(installation_root(root), &network.name);
    first_inner(network, Some(cache)).await
}

async fn reachable_inner(
    network: &Network,
    cache: Option<Cache>,
) -> Result<Vec<Node>, ConnectError> {
    let protocols = protocols(network)?;
    let (nodes, _) = select(network, Selection::PerRow, |address| {
        connect_seed(address, Arc::clone(&protocols), cache.as_ref())
    })
    .await;
    if !nodes.is_empty() {
        return Ok(nodes);
    }
    if let Some(cache) = cache {
        return Ok(fallback(&cache, protocols, MAX_CACHED_REACHABLE).await.0);
    }
    Ok(nodes)
}

/// Connect at most one answering address per seed row without a fallback cache.
///
/// # Errors
/// Returns invalid network protocol configuration.
pub async fn reachable(network: &Network) -> Result<Vec<Node>, ConnectError> {
    reachable_inner(network, None).await
}

/// Connect seed rows, or bounded cached candidates when no seed answers.
///
/// # Errors
/// Returns invalid network protocol configuration.
pub async fn reachable_cached(network: &Network, root: &Path) -> Result<Vec<Node>, ConnectError> {
    let cache = Cache::at(installation_root(root), &network.name);
    reachable_inner(network, Some(cache)).await
}

#[cfg(test)]
#[path = "seed_connections/tests.rs"]
mod tests;
