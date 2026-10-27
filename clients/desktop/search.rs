//! Verified profile search on the open identity's active network, over the client core's
//! `search` module.

use std::path::Path;
use std::sync::{Arc, Mutex};

use dyapp_client::profiles::Profiles;
use dyapp_client::search::{self, SearchFailure};
use dyapp_client::store::Store;
use dyapp_identity::network::{self, Settings};
use dyapp_identity::Identity;
use dyapp_p2p_net::{proto, Protocols};

use crate::profile::{self, Draft};

/// Search providers one search asks.
const SEARCH_NODES: usize = 3;

/// Verified profiles seen in this run, so a search fetches only versions it lacks.
#[derive(Default)]
pub struct SearchState(pub Arc<Mutex<Profiles>>);

/// One filter as the page sends it; `op` is `eq`, `in` or `range`.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
pub struct Condition {
    pub field: u32,
    pub op: String,
    #[serde(default)]
    pub ints: Vec<i64>,
    #[serde(default)]
    pub strings: Vec<String>,
}

impl TryFrom<Condition> for proto::Condition {
    type Error = String;

    fn try_from(condition: Condition) -> Result<Self, String> {
        let op = match condition.op.as_str() {
            "eq" => proto::Op::Eq,
            "in" => proto::Op::In,
            "range" => proto::Op::Range,
            _ => return Err("invalid search op".into()),
        };
        Ok(Self {
            field: condition.field,
            op: op.into(),
            ints: condition.ints,
            strings: condition.strings,
        })
    }
}

/// Why a node gave no usable reply, tagged by `kind` for the page's messages.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Failure {
    NoNodes,
    Unavailable,
    Timeout,
    #[serde(rename_all = "camelCase")]
    RateLimited {
        retry_after_ms: u64,
    },
    Puzzle,
    Invalid,
}

impl From<SearchFailure> for Failure {
    fn from(failure: SearchFailure) -> Self {
        match failure {
            SearchFailure::NoNodes => Self::NoNodes,
            SearchFailure::Unavailable => Self::Unavailable,
            SearchFailure::Timeout => Self::Timeout,
            SearchFailure::RateLimited(after) => Self::RateLimited {
                retry_after_ms: u64::try_from(after.as_millis()).unwrap_or(u64::MAX),
            },
            SearchFailure::Puzzle => Self::Puzzle,
            SearchFailure::Invalid => Self::Invalid,
        }
    }
}

/// A profile the client core verified.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    pub peer_id: String,
    pub draft: Draft,
    pub version: u64,
}

/// One search over the active network's search nodes.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Results {
    /// Field numbers every reached node filters on and the client checks, ascending.
    pub offered: Vec<u32>,
    /// Verified matches in no set order, without blocked peers.
    pub profiles: Vec<Found>,
    /// Coverage may be incomplete.
    pub partial: bool,
    /// Why providers were dropped or requests failed, in order.
    pub failures: Vec<Failure>,
}

/// Searches up to `limit` hits per node (0 means 50) of up to 3 search providers of the active
/// network, with the publish proof-of-work minimum and without blocked peers; no conditions
/// asks for a sample. Each call connects again, so retry is a new call. Errors are
/// `notConnected` (no active network or no answering node), `invalid search op` or a storage
/// message.
pub async fn search(
    cache: &Mutex<Profiles>,
    dir: &Path,
    identity: &Identity,
    conditions: Vec<Condition>,
    limit: u32,
) -> Result<Results, String> {
    let conditions = conditions
        .into_iter()
        .map(proto::Condition::try_from)
        .collect::<Result<Vec<_>, _>>()?;
    let store = Store::open(&dir.join("client.db"), identity).map_err(|e| e.to_string())?;
    let blocked = store.blocked_peers().map_err(|e| e.to_string())?;
    let active = network::load(dir, Settings::Active)
        .map_err(|e| e.to_string())?
        .ok_or("notConnected")?;
    let protocols = Arc::new(Protocols::new(&active.name).map_err(|e| e.to_string())?);
    let mut entry = profile::connect(&active).await?;
    let mut reached = search::connect_search_nodes(&mut entry, protocols, SEARCH_NODES).await;
    // A copy, so no lock is held across the network wait.
    let seen = cache.lock().map_err(|e| e.to_string())?.clone();
    let results = search::search(
        &mut reached.nodes,
        &conditions,
        limit,
        profile::POW_BITS,
        &seen,
        &blocked,
    )
    .await;
    let mut cache = cache.lock().map_err(|e| e.to_string())?;
    let profiles = results
        .profiles
        .into_iter()
        .map(|found| {
            let shown = Found {
                peer_id: found.peer_id.clone(),
                draft: Draft::from(&found.profile),
                version: found.profile.version,
            };
            // An older or equal version is already cached.
            let _ = cache.accept(found);
            shown
        })
        .collect();
    let partial = results.partial || !reached.failures.is_empty();
    reached.failures.extend(results.failures);
    Ok(Results {
        offered: reached.offered,
        profiles,
        partial,
        failures: reached.failures.into_iter().map(Failure::from).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dyapp_identity::network::Network;
    use std::net::TcpListener;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn temp_dir() -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("dyapp-desktop-search-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn search_needs_an_answering_active_network_and_retries_by_calling_again() {
        let (dir, alice, cache) = (temp_dir(), Identity::generate(), Mutex::default());
        let run = |conditions| search(&cache, &dir, &alice, conditions, 0);
        assert_eq!(run(Vec::new()).await, Err("notConnected".into()));
        let bad = Condition {
            field: 1,
            op: "like".into(),
            ints: Vec::new(),
            strings: Vec::new(),
        };
        assert_eq!(run(vec![bad]).await, Err("invalid search op".into()));
        let closed = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();
        let unreachable = Network {
            seeds: vec![closed.to_string()],
            name: String::new(),
        };
        network::save(&dir, Settings::Active, &unreachable).unwrap();
        assert!(run(Vec::new()).await.is_err());
        assert!(run(Vec::new()).await.is_err());
    }

    #[test]
    fn failures_keep_their_kind_and_wait() {
        assert_eq!(
            Failure::from(SearchFailure::RateLimited(Duration::from_secs(2))),
            Failure::RateLimited {
                retry_after_ms: 2000
            }
        );
        assert_eq!(Failure::from(SearchFailure::NoNodes), Failure::NoNodes);
    }
}
