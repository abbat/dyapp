//! Verified profile search on the selected identity folder's active network.

use crate::profile_draft::database;
use crate::profile_network::{active_network, connect};
use crate::profile_publish::{today, POW_BITS};
use crate::CoreError;
use crate::{IdentityKey, PublicProfileDraft};
use dyapp_client::compatibility;
use dyapp_client::profiles::Profiles;
use dyapp_client::search::{self, Employer, SearchFailure};
use dyapp_client::store::Store;
use dyapp_p2p_net::search_results::Filter;
use dyapp_p2p_net::{proto, Protocols};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Search providers one search asks.
const SEARCH_NODES: usize = 3;

fn failed(error: impl std::fmt::Display) -> CoreError {
    CoreError::Failed(error.to_string())
}

/// How a condition compares a field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum SearchOp {
    /// The field equals the one value.
    Eq,
    /// The field equals any of up to 8 values.
    In,
    /// The field lies in two inclusive int values.
    Range,
}

/// One search filter on a profile field number from `offered`.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct SearchCondition {
    /// Profile field number, or the active-day field.
    pub field: u32,
    /// Comparison.
    pub op: SearchOp,
    /// Int values.
    pub ints: Vec<i64>,
    /// String values.
    pub strings: Vec<String>,
}

impl From<SearchCondition> for proto::Condition {
    fn from(condition: SearchCondition) -> Self {
        let op = match condition.op {
            SearchOp::Eq => proto::Op::Eq,
            SearchOp::In => proto::Op::In,
            SearchOp::Range => proto::Op::Range,
        };
        Self {
            field: condition.field,
            op: op.into(),
            ints: condition.ints,
            strings: condition.strings,
        }
    }
}

/// The birth-date condition for an inclusive age range in years on today's date; the lower bound
/// is raised to 18, and a range no birth date fits fails with `invalid search age`.
#[uniffi::export]
pub fn search_age_condition(min: u32, max: u32) -> Result<SearchCondition, CoreError> {
    let filter = Filter {
        age: Some((min, max)),
        ..Filter::default()
    };
    let condition = filter
        .conditions(today()?)
        .ok()
        .and_then(|conditions| conditions.into_iter().next())
        .ok_or_else(|| CoreError::Failed("invalid search age".into()))?;
    Ok(SearchCondition {
        field: condition.field,
        op: SearchOp::Range,
        ints: condition.ints,
        strings: condition.strings,
    })
}

/// Which companies verified results may have; applied only on this device, so no node learns it.
#[derive(Clone, Debug, Default, PartialEq, Eq, uniffi::Enum)]
pub enum SearchEmployer {
    /// Any company or none.
    #[default]
    Any,
    /// Only this company, compared without case and with whitespace collapsed.
    Same {
        /// Company name.
        company: String,
    },
    /// None of these companies; profiles without a company pass.
    Exclude {
        /// Company names.
        companies: Vec<String>,
    },
}

impl From<SearchEmployer> for Employer {
    fn from(employer: SearchEmployer) -> Self {
        match employer {
            SearchEmployer::Any => Self::Any,
            SearchEmployer::Same { company } => Self::Same(company),
            SearchEmployer::Exclude { companies } => Self::Exclude(companies),
        }
    }
}

/// Why a search node gave no usable reply; the UI words it from the variant.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ProfileSearchFailure {
    /// No search node was found.
    NoNodes,
    /// A node could not be reached or would not serve the search.
    Unavailable,
    /// A node did not reply in time.
    Timeout,
    /// A node is busy.
    RateLimited {
        /// Wait before asking again.
        retry_after_ms: u64,
    },
    /// A node set a proof-of-work puzzle the client would not solve.
    Puzzle,
    /// A node sent a malformed reply or a forged record.
    Invalid,
}

impl From<SearchFailure> for ProfileSearchFailure {
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

/// A profile whose signature, conditions and proof of work the shared core checked.
#[derive(Clone, Debug, uniffi::Record)]
pub struct FoundProfile {
    /// The owner's Peer ID.
    pub peer_id: String,
    /// The public profile fields.
    pub draft: PublicProfileDraft,
    /// The highest seen version.
    pub version: u64,
}

/// One search over the active network's search nodes.
#[derive(Clone, Debug, uniffi::Record)]
pub struct ProfileSearchResult {
    /// Field numbers every reached node filters on and the client checks, ascending.
    pub offered: Vec<u32>,
    /// Verified matches in no set order, without blocked peers.
    pub profiles: Vec<FoundProfile>,
    /// Coverage may be incomplete.
    pub partial: bool,
    /// Why nodes were dropped or failed, in order.
    pub failures: Vec<ProfileSearchFailure>,
}

/// Per-identity search session with a verified profile cache; its calls block and run off the
/// UI thread.
#[derive(uniffi::Object)]
pub struct ProfileSearcher {
    dir: PathBuf,
    runtime: tokio::runtime::Runtime,
    cache: Mutex<Profiles>,
}

#[uniffi::export]
impl ProfileSearcher {
    /// Prepare a private Tokio runtime for this existing identity directory.
    #[uniffi::constructor]
    pub fn new(dir: String) -> Result<Arc<Self>, CoreError> {
        database(&dir)?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(failed)?;
        Ok(Arc::new(Self {
            dir: dir.into(),
            runtime,
            cache: Mutex::new(Profiles::default()),
        }))
    }

    /// Search up to `limit` hits per node (0 means 50); no conditions asks for a sample, and
    /// `employer` then narrows the verified results on this device. Each call connects again, so
    /// retry is a new call; without an answering active network it fails with `notConnected`.
    pub fn search(
        &self,
        key: Arc<IdentityKey>,
        conditions: Vec<SearchCondition>,
        employer: SearchEmployer,
        limit: u32,
    ) -> Result<ProfileSearchResult, CoreError> {
        let mut cache = self.cache.lock().map_err(failed)?;
        let store = Store::open(&self.dir.join("client.db"), &key.identity).map_err(failed)?;
        let blocked = store.blocked_peers().map_err(failed)?;
        let own = store.profile_draft().map_err(failed)?.unwrap_or_default();
        let network = active_network(&self.dir)?;
        let conditions: Vec<proto::Condition> = conditions.into_iter().map(Into::into).collect();
        let (mut reached, mut results) = self.runtime.block_on(async {
            let protocols = Arc::new(Protocols::new(&network.name).map_err(failed)?);
            let mut entry = connect(&network).await?;
            let mut reached =
                search::connect_search_nodes(&mut entry, protocols, SEARCH_NODES).await;
            let results = search::search(
                &mut reached.nodes,
                &conditions,
                limit,
                POW_BITS,
                &cache,
                &blocked,
            )
            .await;
            Ok::<_, CoreError>((reached, results))
        })?;
        Employer::from(employer).retain(&mut results.profiles);
        compatibility::rank(&own, &mut results.profiles);
        let profiles = results
            .profiles
            .into_iter()
            .map(|found| {
                let profile = FoundProfile {
                    peer_id: found.peer_id.clone(),
                    draft: PublicProfileDraft::from(&found.profile),
                    version: found.profile.version,
                };
                // An older or equal version is already cached.
                let _ = cache.accept(found);
                profile
            })
            .collect();
        let partial = results.partial || !reached.failures.is_empty();
        reached.failures.extend(results.failures);
        Ok(ProfileSearchResult {
            offered: reached.offered,
            profiles,
            partial,
            failures: reached.failures.into_iter().map(Into::into).collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dyapp_identity::network::{self, Network, Settings};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[test]
    fn search_needs_an_answering_active_network() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("dyapp-ffi-search-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let key = IdentityKey::generate();
        let searcher = ProfileSearcher::new(dir.to_str().unwrap().to_owned()).unwrap();
        let network = Network {
            seeds: Vec::new(),
            name: String::new(),
        };
        network::save(&dir, Settings::Draft, &network).unwrap();
        let search = || searcher.search(key.clone(), Vec::new(), SearchEmployer::Any, 0);
        assert_eq!(search().unwrap_err().to_string(), "notConnected");
        network::save(&dir, Settings::Active, &network).unwrap();
        // A retry is a new call on the same searcher.
        assert_eq!(search().unwrap_err().to_string(), "notConnected");
    }

    #[test]
    fn an_age_range_becomes_one_birth_range_condition() {
        let condition = search_age_condition(10, 30).unwrap();
        assert_eq!(condition.field, dyapp_p2p_net::search_results::BIRTH_AGE);
        assert_eq!(condition.op, SearchOp::Range);
        // The lower bound is raised to 18, so the range is shorter than 21 years.
        assert!(condition.ints[0] < condition.ints[1]);
        assert!(condition.ints[1] - condition.ints[0] < 14 * 10_000);
        assert_eq!(
            search_age_condition(30, 20).unwrap_err().to_string(),
            "invalid search age"
        );
    }

    #[test]
    fn conditions_and_failures_keep_their_meaning() {
        let condition = proto::Condition::from(SearchCondition {
            field: 1,
            op: SearchOp::In,
            ints: vec![1, 2],
            strings: Vec::new(),
        });
        assert_eq!(condition.op, i32::from(proto::Op::In));
        assert_eq!(condition.ints, vec![1, 2]);
        assert_eq!(
            ProfileSearchFailure::from(SearchFailure::RateLimited(Duration::from_secs(3))),
            ProfileSearchFailure::RateLimited {
                retry_after_ms: 3000
            }
        );
        assert_eq!(
            ProfileSearchFailure::from(SearchFailure::Invalid),
            ProfileSearchFailure::Invalid
        );
        assert_eq!(
            Employer::from(SearchEmployer::Exclude {
                companies: vec!["Acme".into()]
            }),
            Employer::Exclude(vec!["Acme".into()])
        );
    }
}
