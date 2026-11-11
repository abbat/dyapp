//! Verified profile search on the open identity's active network, over the client core's
//! `search` module.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use dyapp_client::compatibility;
use dyapp_client::profiles::Profiles;
use dyapp_client::search::{self, Employer, SearchFailure};
use dyapp_client::search_input::{employer_filter, EmployerMode, SearchChoiceInputs};
use dyapp_client::store::Store;
use dyapp_identity::network::{self, Settings};
use dyapp_identity::Identity;
use dyapp_p2p_net::search_results::{
    mutual_conditions, Filter, BIRTH_AGE, SEEKING_AGE, SEEKING_GENDERS,
};
use dyapp_p2p_net::{proto, Protocols};
use dyapp_profile::search_input::SearchRangeInputs;
use dyapp_profile::{CivilDate, Profile, VerifiedProfile, HEIGHT_FIELD};

use crate::peer_photos::PhotoCache;
use crate::profile::{self, Draft};

/// Search providers one search asks.
const SEARCH_NODES: usize = 3;

/// Verified profiles seen in the active identity and network context.
#[derive(Default)]
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub struct SearchState(pub Arc<Mutex<ProfileCache>>);

/// Profiles and their context epoch share one lock, so a late result cannot repopulate a cache
/// cleared by network activation.
#[derive(Default)]
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub struct ProfileCache {
    epoch: u64,
    profiles: Profiles,
    pub photos: PhotoCache,
}

impl ProfileCache {
    #[expect(unreachable_pub, reason = "Retained module API visibility")]
    pub fn snapshot(&self) -> (u64, Profiles) {
        (self.epoch, self.profiles.clone())
    }

    #[expect(unreachable_pub, reason = "Retained module API visibility")]
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    #[expect(unreachable_pub, reason = "Retained module API visibility")]
    pub fn current(&self, epoch: u64) -> bool {
        self.epoch == epoch
    }

    #[expect(unreachable_pub, reason = "Retained module API visibility")]
    pub fn accept_if_current(&mut self, epoch: u64, profile: VerifiedProfile) -> bool {
        self.current(epoch) && self.profiles.accept(profile).is_ok()
    }

    #[expect(unreachable_pub, reason = "Retained module API visibility")]
    pub fn clear(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
        self.profiles = Profiles::default();
        self.photos = PhotoCache::default();
    }
}

impl SearchState {
    /// Discard verified profiles held for the previous active context.
    #[expect(unreachable_pub, reason = "Retained module API visibility")]
    pub fn clear(&self) -> Result<(), String> {
        self.0.lock().map_err(|e| e.to_string())?.clear();
        Ok(())
    }

    /// Save the checked network and clear profiles if it changed, at one cache boundary.
    #[expect(unreachable_pub, reason = "Retained module API visibility")]
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

const COUNTRY_FIELD: u32 = 4;
const INTEREST_FIELD: u32 = 19;
const PLACE_FIELD: u32 = 20;

/// Which of the page's fixed filters the reached nodes offer; scale filters follow `offered`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[expect(unreachable_pub, reason = "Retained module API visibility")]
#[expect(
    clippy::struct_excessive_bools,
    reason = "Independent filter rows the page shows or hides over IPC"
)]
pub struct Filters {
    pub age: bool,
    pub height: bool,
    pub interest: bool,
    pub country: bool,
    /// A place is picked within a country, so it needs both.
    pub place: bool,
    /// A mutual search needs both the sought genders and the sought age.
    pub mutual: bool,
}

impl Filters {
    fn offered(offered: &[u32]) -> Self {
        let has = |field| offered.contains(&field);
        Self {
            age: has(BIRTH_AGE),
            height: has(HEIGHT_FIELD),
            interest: has(INTEREST_FIELD),
            country: has(COUNTRY_FIELD),
            place: has(COUNTRY_FIELD) && has(PLACE_FIELD),
            mutual: has(SEEKING_GENDERS) && has(SEEKING_AGE),
        }
    }
}

/// The company filter mode the page selected; the text is interpreted by the client core.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EmployerChoice {
    #[default]
    Any,
    Same,
    Exclude,
}

impl From<EmployerChoice> for EmployerMode {
    fn from(choice: EmployerChoice) -> Self {
        match choice {
            EmployerChoice::Any => Self::Any,
            EmployerChoice::Same => Self::Same,
            EmployerChoice::Exclude => Self::Exclude,
        }
    }
}

/// One search as the page's form holds it, uninterpreted; disabled fields arrive blank.
#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub struct Query {
    /// Field numbers of the last results; only these fields constrain the search.
    pub offered: Vec<u32>,
    /// Scale key to chosen code; 0 is any.
    pub picked: BTreeMap<String, i64>,
    pub country: String,
    pub interest: String,
    /// The selected place suggestion, if any.
    pub place: Option<u32>,
    pub age_min: String,
    pub age_max: String,
    pub height_min: String,
    pub height_max: String,
    pub employer: EmployerChoice,
    pub company: String,
    /// Hits per node; 0 means 50.
    pub limit: u32,
    /// Also ask for profiles that seek the saved profile's gender and age.
    pub mutual: bool,
}

impl Query {
    /// Node conditions by the shared input rules: age as birth-date bounds at `today`, then
    /// height, then the offered choices. Errors are `invalid search age`, `invalid search height`
    /// or `invalidInterests`.
    fn conditions(&self, today: CivilDate) -> Result<Vec<proto::Condition>, String> {
        let ranges = SearchRangeInputs {
            age_min: self.age_min.clone(),
            age_max: self.age_max.clone(),
            height_min: self.height_min.clone(),
            height_max: self.height_max.clone(),
        }
        .parse(
            self.offered.contains(&BIRTH_AGE),
            self.offered.contains(&HEIGHT_FIELD),
        )
        .map_err(|error| error.to_string())?;
        let age = Filter {
            age: ranges.age,
            ..Filter::default()
        };
        let mut out = age.conditions(today).map_err(|_| "invalid search age")?;
        let height = Filter {
            min_height: ranges.min_height,
            max_height: ranges.max_height,
            ..Filter::default()
        };
        out.extend(
            height
                .conditions(today)
                .map_err(|_| "invalid search height")?,
        );
        let choices = SearchChoiceInputs {
            picked: self.picked.clone(),
            country: self.country.clone(),
            interest: self.interest.clone(),
            place: self.place,
        };
        for mut condition in choices.conditions(&self.offered) {
            if condition.field == INTEREST_FIELD {
                condition.strings = profile::canonical_interests(&condition.strings)?;
            }
            out.push(condition);
        }
        Ok(out)
    }

    fn employer(&self) -> Employer {
        employer_filter(self.employer.into(), &self.company)
    }
}

/// The query's conditions, then a mutual search's for `own`; `mutualUnavailable` when `own` has no
/// gender or no exact age to tell the nodes.
fn node_conditions(
    query: &Query,
    own: &Profile,
    today: CivilDate,
) -> Result<Vec<proto::Condition>, String> {
    let mut conditions = query.conditions(today)?;
    if query.mutual {
        conditions.extend(mutual_conditions(own, today).ok_or("mutualUnavailable")?);
    }
    Ok(conditions)
}

/// Why a node gave no usable reply, tagged by `kind` for the page's messages.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[expect(unreachable_pub, reason = "Retained module API visibility")]
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
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub struct Found {
    pub peer_id: String,
    pub draft: Draft,
    /// Youngest and oldest possible age today from the public birth fields.
    pub age: Option<(u32, u32)>,
    /// The full image of each photo in `draft.photos`, empty where a photo has none.
    pub full_photos: Vec<Vec<u8>>,
    pub version: u64,
}

impl From<&VerifiedProfile> for Found {
    fn from(found: &VerifiedProfile) -> Self {
        Self {
            peer_id: found.peer_id.clone(),
            draft: Draft::from(&found.profile),
            age: profile::today()
                .ok()
                .and_then(|today| found.profile.possible_age_range(today)),
            full_photos: found
                .profile
                .photos
                .iter()
                .map(|photo| photo.full.first().cloned().unwrap_or_default())
                .collect(),
            version: found.profile.version,
        }
    }
}

/// One search over the active network's search nodes.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub struct Results {
    /// Field numbers every reached node filters on and the client checks, ascending.
    pub offered: Vec<u32>,
    pub filters: Filters,
    /// Verified matches in no set order, without blocked or passed peers.
    pub profiles: Vec<Found>,
    /// Coverage may be incomplete.
    pub partial: bool,
    /// Why providers were dropped or requests failed, in order.
    pub failures: Vec<Failure>,
}

/// Searches up to `limit` hits per node (0 means 50) of up to 3 search providers of the active
/// network, with the publish proof-of-work minimum and without blocked or passed peers; an age
/// range is checked against public birth dates today; no conditions asks for a sample; the
/// employer filter then narrows the verified results on this device. Each call connects
/// again, so retry is a new call. Errors are `notConnected` (no active network or no answering
/// node), `mutualUnavailable`, those of `check` or a storage message.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub async fn search(
    cache: &Mutex<ProfileCache>,
    dir: &Path,
    identity: &Identity,
    query: Query,
) -> Result<Results, String> {
    let limit = query.limit;
    let employer = query.employer();
    let today = profile::today()?;
    let store = Store::open(&dir.join("client.db"), identity).map_err(|e| e.to_string())?;
    let mut hidden = store.blocked_peers().map_err(|e| e.to_string())?;
    hidden.extend(store.passed_peers().map_err(|e| e.to_string())?);
    let own = profile::stored(&store)?;
    let conditions = node_conditions(&query, &own, today)?;
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
        dyapp_profile::required_pow_bits(),
        &seen,
        &hidden,
    )
    .await;
    employer.retain(&mut results.profiles);
    compatibility::rank(&own, &mut results.profiles);
    let profiles = project_current(cache, epoch, results.profiles)?;
    let partial = results.partial || !reached.failures.is_empty();
    reached.failures.extend(results.failures);
    Ok(Results {
        filters: Filters::offered(&reached.offered),
        offered: reached.offered,
        profiles,
        partial,
        failures: reached.failures.into_iter().map(Failure::from).collect(),
    })
}

// Finalize under the activation lock: suppress returned results as well as stale cache writes.
fn project_current(
    cache: &Mutex<ProfileCache>,
    epoch: u64,
    profiles: Vec<VerifiedProfile>,
) -> Result<Vec<Found>, String> {
    let mut cache = cache.lock().map_err(|e| e.to_string())?;
    if !cache.current(epoch) {
        return Err("notConnected".into());
    }
    Ok(profiles
        .into_iter()
        .map(|found| {
            let shown = Found::from(&found);
            // Equal versions remain displayable even when they add nothing to the cache.
            cache.accept_if_current(epoch, found);
            shown
        })
        .collect())
}

/// Blocks a well-formed peer on this device; later searches drop it. A known Peer ID need
/// not come from this run's search cache (for example it may have sent a like).
#[expect(unreachable_pub, reason = "Retained module API visibility")]
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
        // Parallel tests can read the same clock value, notably on Windows.
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("dyapp-desktop-search-{nanos}-{n}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn search_needs_an_answering_active_network_and_retries_by_calling_again() {
        let (dir, alice, cache) = (temp_dir(), Identity::generate(), Mutex::default());
        let run = |age_max: &str| {
            let query = Query {
                offered: vec![BIRTH_AGE],
                age_min: "30".into(),
                age_max: age_max.into(),
                ..Query::default()
            };
            search(&cache, &dir, &alice, query)
        };
        assert_eq!(Box::pin(run("")).await, Err("notConnected".into()));
        assert_eq!(Box::pin(run("20")).await, Err("invalid search age".into()));
        let closed = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();
        let unreachable = Network {
            seeds: vec![closed.to_string()],
            name: String::new(),
        };
        network::save(&dir, Settings::Active, &unreachable).unwrap();
        assert!(Box::pin(run("")).await.is_err());
        assert!(Box::pin(run("")).await.is_err());
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
        let verified = verified_fixture_profile;
        let prior_profile = verified(&Identity::generate());
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (resume_tx, resume_rx) = tokio::sync::oneshot::channel();
        let cache = Arc::clone(&state.0);
        let worker = tokio::spawn(async move {
            let (epoch, _) = cache.lock().unwrap().snapshot();
            started_tx.send(()).unwrap();
            resume_rx.await.unwrap();
            project_current(&cache, epoch, vec![prior_profile])
        });
        started_rx.await.unwrap();
        let current = verified(&Identity::generate());
        assert!(state.0.lock().unwrap().accept_if_current(0, current));
        checks
            .remember_connected(0, dir.clone(), second.clone())
            .unwrap();
        assert_eq!(
            state.activate_network(&checks, &dir).unwrap().name,
            "second"
        );
        assert_eq!(
            network::load(&dir, Settings::Active).unwrap(),
            Some(second.clone())
        );
        assert_eq!(state.0.lock().unwrap().snapshot().1.all(), []);
        resume_tx.send(()).unwrap();
        assert_eq!(worker.await.unwrap(), Err("notConnected".into()));
        let new_epoch = state.0.lock().unwrap().snapshot().0;
        assert!(state
            .0
            .lock()
            .unwrap()
            .accept_if_current(new_epoch, verified(&Identity::generate())));
        assert_eq!(state.0.lock().unwrap().snapshot().1.all().len(), 1);
        assert_reactivation_keeps_profiles(&state, &checks, &dir, second);
    }

    fn verified_fixture_profile(owner: &Identity) -> dyapp_profile::VerifiedProfile {
        dyapp_profile::verify(
            &dyapp_profile::Profile {
                version: 1,
                ..dyapp_profile::Profile::default()
            }
            .sign(owner),
        )
        .unwrap()
    }

    fn assert_reactivation_keeps_profiles(
        state: &SearchState,
        checks: &crate::network::NetworkState,
        dir: &Path,
        second: Network,
    ) {
        // Re-activating the same settings consumes the check but keeps valid profiles.
        let epoch = state.0.lock().unwrap().epoch();
        checks
            .remember_connected(checks.check_epoch().unwrap(), dir.to_path_buf(), second)
            .unwrap();
        state.activate_network(checks, dir).unwrap();
        let cached = state.0.lock().unwrap();
        assert_eq!(cached.epoch(), epoch);
        assert_eq!(cached.snapshot().1.all().len(), 1);
    }

    #[test]
    fn result_projection_rejects_old_network_but_allows_repeated_profiles() {
        let owner = Identity::generate();
        let verified = dyapp_profile::verify(
            &Profile {
                version: 1,
                ..Profile::default()
            }
            .sign(&owner),
        )
        .unwrap();
        let cache = Mutex::new(ProfileCache::default());
        let epoch = cache.lock().unwrap().epoch();
        let profiles = vec![verified];
        let first = project_current(&cache, epoch, profiles.clone()).unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].peer_id, owner.peer_id());
        assert_eq!(
            project_current(&cache, epoch, profiles.clone()).unwrap(),
            first
        );
        cache.lock().unwrap().clear();
        assert_eq!(
            project_current(&cache, epoch, profiles),
            Err("notConnected".into())
        );
        // Even an empty old-network result must not return obsolete provider metadata.
        assert_eq!(
            project_current(&cache, epoch, Vec::new()),
            Err("notConnected".into())
        );
        assert_eq!(cache.lock().unwrap().snapshot().1.all(), []);
    }

    #[test]
    fn interest_query_matches_canonical_published_tags() {
        let today = CivilDate::from_unix_days(20_000).unwrap();
        let conditions = |field, text: &str| {
            Query {
                offered: vec![field],
                interest: text.into(),
                country: text.into(),
                ..Query::default()
            }
            .conditions(today)
            .unwrap()
        };
        let [query] = <[_; 1]>::try_from(conditions(INTEREST_FIELD, "  E\u{301}COLE  ")).unwrap();
        assert_eq!(query.strings, ["école"]);
        let profile = Profile {
            interests: profile::canonical_interests(&[" ÉCOLE ".into()]).unwrap(),
            ..Profile::default()
        };
        assert!(dyapp_p2p_net::search_results::passes(&[query], 0, &profile));
        assert_eq!(conditions(INTEREST_FIELD, "  "), []);
        let country = conditions(COUNTRY_FIELD, "DE");
        assert_eq!(
            (country[0].field, country[0].strings.clone()),
            (4, vec!["DE".into()])
        );
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
    fn a_mutual_search_adds_the_own_gender_and_age_or_is_refused() {
        let today = CivilDate::from_unix_days(20_000).unwrap();
        let query = |mutual| Query {
            mutual,
            ..Query::default()
        };
        let unknown = Profile::default();
        let own = Profile {
            gender: dyapp_profile::Gender::Woman.code(),
            birth_year: Some(1980),
            birth_month: Some(1),
            birth_day: Some(1),
            ..Profile::default()
        };
        assert_eq!(
            node_conditions(&query(false), &unknown, today),
            Ok(Vec::new())
        );
        assert_eq!(
            node_conditions(&query(true), &unknown, today),
            Err("mutualUnavailable".into())
        );
        let wanted = mutual_conditions(&own, today).unwrap();
        assert_ne!(wanted, Vec::<dyapp_p2p_net::proto::Condition>::new());
        assert_eq!(node_conditions(&query(true), &own, today), Ok(wanted));
    }

    #[test]
    fn an_age_range_becomes_birth_bounds_before_the_other_conditions() {
        let today = CivilDate::from_unix_days(20_000).unwrap();
        let gender = dyapp_profile::Gender::FIELD;
        let query = |offered: &[u32], min: &str, max: &str| Query {
            offered: offered.to_vec(),
            picked: BTreeMap::from([("gender".into(), 1)]),
            age_min: min.into(),
            age_max: max.into(),
            height_min: max.into(),
            height_max: min.into(),
            ..Query::default()
        };
        let out = query(&[BIRTH_AGE, gender], "20", "30")
            .conditions(today)
            .unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].field, BIRTH_AGE);
        assert_eq!(out[0].op, i32::from(proto::Op::Range));
        assert_eq!((out[1].field, out[1].ints.clone()), (gender, vec![1]));
        // Fields the nodes did not offer stay out, even when filled or reversed.
        let unoffered = query(&[gender], "30", "20").conditions(today).unwrap();
        assert_eq!(unoffered.len(), 1);
        assert_eq!(
            query(&[BIRTH_AGE], "30", "20").conditions(today),
            Err("invalid search age".into())
        );
        assert_eq!(
            query(&[HEIGHT_FIELD], "170", "190").conditions(today),
            Err("invalid search height".into())
        );
        assert_eq!(
            Filters::offered(&[BIRTH_AGE, COUNTRY_FIELD, SEEKING_GENDERS]),
            Filters {
                age: true,
                country: true,
                ..Filters::default()
            }
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
        let query: Query =
            serde_json::from_str(r#"{"employer": "exclude", "company": " Acme; Other ;"}"#)
                .unwrap();
        assert_eq!(
            query.employer(),
            Employer::Exclude(vec!["Acme".into(), "Other".into()])
        );
        let same: Query =
            serde_json::from_str(r#"{"employer": "same", "company": "Acme"}"#).unwrap();
        assert_eq!(same.employer(), Employer::Same("Acme".into()));
        let none: Query = serde_json::from_str("{}").unwrap();
        assert_eq!(none.employer(), Employer::Any);
    }
}
