//! Verified profile search on the open identity's active network, over the client core's
//! `search` module.

use std::path::Path;
use std::sync::{Arc, Mutex};

use dyapp_client::compatibility;
use dyapp_client::profiles::Profiles;
use dyapp_client::search::{self, Employer, SearchFailure};
use dyapp_client::store::Store;
use dyapp_identity::network::{self, Settings};
use dyapp_identity::Identity;
use dyapp_p2p_net::search_results::Filter;
use dyapp_p2p_net::{proto, Protocols};
use dyapp_profile::{CivilDate, VerifiedProfile};

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

/// The company filter as the page sends it, tagged by `kind`; it runs only on this device.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum EmployerFilter {
    #[default]
    Any,
    Same {
        company: String,
    },
    Exclude {
        companies: Vec<String>,
    },
}

impl From<EmployerFilter> for Employer {
    fn from(filter: EmployerFilter) -> Self {
        match filter {
            EmployerFilter::Any => Self::Any,
            EmployerFilter::Same { company } => Self::Same(company),
            EmployerFilter::Exclude { companies } => Self::Exclude(companies),
        }
    }
}

/// One search as the page sends it.
#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(default)]
pub struct Query {
    /// Inclusive years; the lower bound is raised to 18.
    pub age: Option<(u32, u32)>,
    pub conditions: Vec<Condition>,
    pub employer: EmployerFilter,
    /// Hits per node; 0 means 50.
    pub limit: u32,
}

impl Query {
    /// Node conditions with the age range resolved to birth-date bounds at `today`.
    fn conditions(self, today: CivilDate) -> Result<Vec<proto::Condition>, String> {
        let age = Filter {
            age: self.age,
            ..Filter::default()
        };
        let mut out = age.conditions(today).map_err(|_| "invalid search age")?;
        for condition in self.conditions {
            out.push(condition.try_into()?);
        }
        Ok(out)
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

impl From<&VerifiedProfile> for Found {
    fn from(found: &VerifiedProfile) -> Self {
        Self {
            peer_id: found.peer_id.clone(),
            draft: Draft::from(&found.profile),
            version: found.profile.version,
        }
    }
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
/// network, with the publish proof-of-work minimum and without blocked peers; `age` is an
/// inclusive range of years, checked against public birth dates today; no conditions asks for a
/// sample; `employer` then narrows the verified results on this device. Each call connects
/// again, so retry is a new call. Errors are `notConnected` (no active network or no answering
/// node), `invalid search op`, `invalid search age` or a storage message.
pub async fn search(
    cache: &Mutex<Profiles>,
    dir: &Path,
    identity: &Identity,
    query: Query,
) -> Result<Results, String> {
    let limit = query.limit;
    let employer = Employer::from(query.employer.clone());
    let conditions = query.conditions(profile::today()?)?;
    let store = Store::open(&dir.join("client.db"), identity).map_err(|e| e.to_string())?;
    let blocked = store.blocked_peers().map_err(|e| e.to_string())?;
    let own = profile::stored(&store)?;
    let active = network::load(dir, Settings::Active)
        .map_err(|e| e.to_string())?
        .ok_or("notConnected")?;
    let protocols = Arc::new(Protocols::new(&active.name).map_err(|e| e.to_string())?);
    let mut entry = profile::connect(&active).await?;
    let mut reached = search::connect_search_nodes(&mut entry, protocols, SEARCH_NODES).await;
    // A copy, so no lock is held across the network wait.
    let seen = cache.lock().map_err(|e| e.to_string())?.clone();
    let mut results = search::search(
        &mut reached.nodes,
        &conditions,
        limit,
        profile::POW_BITS,
        &seen,
        &blocked,
    )
    .await;
    employer.retain(&mut results.profiles);
    compatibility::rank(&own, &mut results.profiles);
    let mut cache = cache.lock().map_err(|e| e.to_string())?;
    let profiles = results
        .profiles
        .into_iter()
        .map(|found| {
            let shown = Found::from(&found);
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

/// Blocks `peer_id` on this device: later searches drop it. Only a peer some search of this run
/// returned can be blocked. Errors are `unknownPeer` or a storage message.
pub fn block(
    cache: &Mutex<Profiles>,
    dir: &Path,
    identity: &Identity,
    peer_id: &str,
) -> Result<(), String> {
    if cache.lock().map_err(|e| e.to_string())?.get(peer_id).is_none() {
        return Err("unknownPeer".into());
    }
    let store = Store::open(&dir.join("client.db"), identity).map_err(|e| e.to_string())?;
    store.set_blocked(peer_id, true).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use dyapp_identity::network::Network;
    use dyapp_p2p_net::search_results::BIRTH_AGE;
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
        let run = |conditions| {
            let query = Query {
                conditions,
                ..Query::default()
            };
            search(&cache, &dir, &alice, query)
        };
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
    fn only_a_peer_a_search_returned_can_be_blocked() {
        let (dir, alice, cache) = (temp_dir(), Identity::generate(), Mutex::default());
        let stranger = Identity::generate().peer_id().to_string();
        assert_eq!(
            block(&cache, &dir, &alice, &stranger),
            Err("unknownPeer".into())
        );
        let store = Store::open(&dir.join("client.db"), &alice).unwrap();
        assert!(store.blocked_peers().unwrap().is_empty());
    }

    #[test]
    fn an_age_range_becomes_birth_bounds_before_the_other_conditions() {
        let today = CivilDate::from_unix_days(20_000).unwrap();
        let gender = Condition {
            field: 26,
            op: "eq".into(),
            ints: vec![1],
            strings: Vec::new(),
        };
        let query = |age| Query {
            age,
            conditions: vec![gender.clone()],
            ..Query::default()
        };
        let out = query(Some((20, 30))).conditions(today).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].field, BIRTH_AGE);
        assert_eq!(out[0].op, i32::from(proto::Op::Range));
        assert_eq!((out[1].field, out[1].ints.clone()), (26, vec![1]));
        assert_eq!(query(None).conditions(today).unwrap().len(), 1);
        assert_eq!(
            query(Some((30, 20))).conditions(today),
            Err("invalid search age".into())
        );
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

    #[test]
    fn the_page_employer_filter_reaches_the_core_filter() {
        let query: Query = serde_json::from_str(
            r#"{"employer": {"kind": "exclude", "companies": ["Acme", "Other"]}}"#,
        )
        .unwrap();
        assert_eq!(
            Employer::from(query.employer),
            Employer::Exclude(vec!["Acme".into(), "Other".into()])
        );
        let same: EmployerFilter =
            serde_json::from_str(r#"{"kind": "same", "company": "Acme"}"#).unwrap();
        assert_eq!(Employer::from(same), Employer::Same("Acme".into()));
        let none: Query = serde_json::from_str("{}").unwrap();
        assert_eq!(none.employer, EmployerFilter::Any);
    }
}
