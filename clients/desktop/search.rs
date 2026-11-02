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

/// Verified profiles seen in the active identity and network context.
#[derive(Default)]
pub struct SearchState(pub Arc<Mutex<ProfileCache>>);

/// Profiles and their context epoch share one lock, so a late result cannot repopulate a cache
/// cleared by network activation.
#[derive(Default)]
pub struct ProfileCache {
    epoch: u64,
    profiles: Profiles,
}

impl ProfileCache {
    pub fn snapshot(&self) -> (u64, Profiles) {
        (self.epoch, self.profiles.clone())
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    pub fn current(&self, epoch: u64) -> bool {
        self.epoch == epoch
    }

    pub fn accept_if_current(&mut self, epoch: u64, profile: VerifiedProfile) -> bool {
        self.current(epoch) && self.profiles.accept(profile).is_ok()
    }

    pub fn clear(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
        self.profiles = Profiles::default();
    }
}

impl SearchState {
    /// Discard verified profiles held for the previous active context.
    pub fn clear(&self) -> Result<(), String> {
        self.0.lock().map_err(|e| e.to_string())?.clear();
        Ok(())
    }

    /// Save the checked network and clear profiles if it changed, at one cache boundary.
    pub fn activate_network(
        &self,
        checks: &crate::network::NetworkState,
        dir: &Path,
    ) -> Result<crate::network::Draft, String> {
        let mut cache = self.0.lock().map_err(|e| e.to_string())?;
        let previous = network::load(dir, Settings::Active).map_err(|e| e.to_string())?;
        let draft = crate::network::activate(checks, dir)?;
        if previous != Some(draft.clone().into()) {
            cache.clear();
        }
        Ok(draft)
    }
}

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
    /// Verified matches in no set order, without blocked or passed peers.
    pub profiles: Vec<Found>,
    /// Coverage may be incomplete.
    pub partial: bool,
    /// Why providers were dropped or requests failed, in order.
    pub failures: Vec<Failure>,
}

/// Searches up to `limit` hits per node (0 means 50) of up to 3 search providers of the active
/// network, with the publish proof-of-work minimum and without blocked or passed peers; `age` is an
/// inclusive range of years, checked against public birth dates today; no conditions asks for a
/// sample; `employer` then narrows the verified results on this device. Each call connects
/// again, so retry is a new call. Errors are `notConnected` (no active network or no answering
/// node), `invalid search op`, `invalid search age` or a storage message.
pub async fn search(
    cache: &Mutex<ProfileCache>,
    dir: &Path,
    identity: &Identity,
    query: Query,
) -> Result<Results, String> {
    let limit = query.limit;
    let employer = Employer::from(query.employer.clone());
    let conditions = query.conditions(profile::today()?)?;
    let store = Store::open(&dir.join("client.db"), identity).map_err(|e| e.to_string())?;
    let mut hidden = store.blocked_peers().map_err(|e| e.to_string())?;
    hidden.extend(store.passed_peers().map_err(|e| e.to_string())?);
    let own = profile::stored(&store)?;
    // Capture the context before loading settings: activation saves settings and advances this
    // epoch under the same cache lock.
    let (epoch, seen) = cache.lock().map_err(|e| e.to_string())?.snapshot();
    let active = network::load(dir, Settings::Active)
        .map_err(|e| e.to_string())?
        .ok_or("notConnected")?;
    if !cache.lock().map_err(|e| e.to_string())?.current(epoch) {
        return Err("notConnected".into());
    }
    let protocols = Arc::new(Protocols::new(&active.name).map_err(|e| e.to_string())?);
    let mut entry = profile::connect(&active).await?;
    let mut reached = search::connect_search_nodes(&mut entry, protocols, SEARCH_NODES).await;
    let mut results = search::search(
        &mut reached.nodes,
        &conditions,
        limit,
        profile::POW_BITS,
        &seen,
        &hidden,
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
            // An older, equal or previous-network result is not cached.
            cache.accept_if_current(epoch, found);
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

/// Blocks a well-formed peer on this device; later searches drop it. A known Peer ID need
/// not come from this run's search cache (for example it may have sent a like).
pub fn block(dir: &Path, identity: &Identity, peer_id: &str) -> Result<(), String> {
    if peer_id.len() != 64
        || !peer_id
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
        || peer_id == identity.peer_id()
    {
        return Err("invalidPeer".into());
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

    #[tokio::test]
    async fn activation_rejects_a_paused_previous_network_profile_result() {
        let dir = temp_dir();
        let state = SearchState::default();
        let checks = crate::network::NetworkState::default();
        let first = Network {
            seeds: vec!["first:1".into()],
            name: "first".into(),
        };
        let second = Network {
            seeds: vec!["second:1".into()],
            name: "second".into(),
        };
        network::save(&dir, Settings::Active, &first).unwrap();
        let verified = |owner: &Identity| {
            dyapp_profile::verify(
                &dyapp_profile::Profile {
                    version: 1,
                    ..dyapp_profile::Profile::default()
                }
                .sign(owner),
            )
            .unwrap()
        };
        let stale = verified(&Identity::generate());
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (resume_tx, resume_rx) = tokio::sync::oneshot::channel();
        let cache = Arc::clone(&state.0);
        let worker = tokio::spawn(async move {
            let (epoch, _) = cache.lock().unwrap().snapshot();
            started_tx.send(()).unwrap();
            resume_rx.await.unwrap();
            cache.lock().unwrap().accept_if_current(epoch, stale)
        });
        started_rx.await.unwrap();
        let current = verified(&Identity::generate());
        assert!(state.0.lock().unwrap().accept_if_current(0, current));
        checks.remember_connected(0, dir.clone(), second.clone()).unwrap();
        assert_eq!(state.activate_network(&checks, &dir).unwrap().name, "second");
        assert_eq!(network::load(&dir, Settings::Active).unwrap(), Some(second.clone()));
        assert!(state.0.lock().unwrap().snapshot().1.all().is_empty());
        resume_tx.send(()).unwrap();
        assert!(!worker.await.unwrap());
        let new_epoch = state.0.lock().unwrap().snapshot().0;
        assert!(state
            .0
            .lock()
            .unwrap()
            .accept_if_current(new_epoch, verified(&Identity::generate())));
        assert_eq!(state.0.lock().unwrap().snapshot().1.all().len(), 1);
        // Re-activating the same settings consumes the check but keeps valid profiles.
        let epoch = state.0.lock().unwrap().epoch();
        checks
            .remember_connected(checks.check_epoch().unwrap(), dir.clone(), second)
            .unwrap();
        state.activate_network(&checks, &dir).unwrap();
        let cached = state.0.lock().unwrap();
        assert_eq!(cached.epoch(), epoch);
        assert_eq!(cached.snapshot().1.all().len(), 1);
    }

    #[test]
    fn known_peer_id_can_be_blocked_without_a_search_result() {
        let (dir, alice) = (temp_dir(), Identity::generate());
        let peer = Identity::generate().peer_id();
        assert_eq!(block(&dir, &alice, "bad"), Err("invalidPeer".into()));
        assert_eq!(
            block(&dir, &alice, &alice.peer_id()),
            Err("invalidPeer".into())
        );
        assert!(block(&dir, &alice, &peer).is_ok());
        let store = Store::open(&dir.join("client.db"), &alice).unwrap();
        assert!(store.blocked(&peer).unwrap());
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
