//! Verified profile search on the selected identity folder's active network.

use crate::peer_photos::PhotoCache;
mod service;

use crate::profile_draft::database;
use crate::profile_publish::today;
use crate::slot_operation::SlotWorker;
use crate::{CoreError, MobileIdentityOperation, MobileIdentityRegistry};
use crate::{IdentityKey, PublicProfileDraft};
use dyapp_client::profiles::Profiles;
use dyapp_client::search::{Employer, SearchFailure};
use dyapp_p2p_net::proto;
use dyapp_p2p_net::search_results::{mutual_conditions, Filter};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

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
///
/// # Errors
///
/// Returns an error if the current UTC date is unsupported or the requested
/// age range admits no supported adult birth date.
pub(crate) fn search_age_condition(min: u32, max: u32) -> Result<SearchCondition, CoreError> {
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

/// The height condition for optional inclusive bounds in centimetres; `None` without a bound. A
/// range no height fits fails with `invalid search height`.
#[uniffi::export]
///
/// # Errors
///
/// Returns an error if the current UTC date is unsupported or the requested
/// height bounds form an invalid range.
pub(crate) fn search_height_condition(
    from: Option<u32>,
    to: Option<u32>,
) -> Result<Option<SearchCondition>, CoreError> {
    let filter = Filter {
        min_height: from,
        max_height: to,
        ..Filter::default()
    };
    let conditions = filter
        .conditions(today()?)
        .map_err(|_| CoreError::Failed("invalid search height".into()))?;
    Ok(conditions
        .into_iter()
        .next()
        .map(|condition| SearchCondition {
            field: condition.field,
            op: SearchOp::Range,
            ints: condition.ints,
            strings: condition.strings,
        }))
}

/// The conditions of a mutual search for the owner of `draft` on today's date: results must seek
/// the owner's gender and allow their age. Empty when the draft states no known gender or the
/// age is not exact or adult; the option is then not offered.
#[uniffi::export]
///
/// # Errors
///
/// Returns an error if the current UTC date cannot be represented.
#[expect(
    clippy::needless_pass_by_value,
    reason = "UniFFI lifts owned arguments across the native language boundary"
)]
pub(crate) fn search_mutual_conditions(
    draft: PublicProfileDraft,
) -> Result<Vec<SearchCondition>, CoreError> {
    Ok(mutual_at(&draft, today()?))
}

fn mutual_at(draft: &PublicProfileDraft, day: dyapp_profile::CivilDate) -> Vec<SearchCondition> {
    let mut me = dyapp_profile::Profile::default();
    draft.apply_to(&mut me);
    let conditions = mutual_conditions(&me, day).unwrap_or_default();
    conditions
        .into_iter()
        .map(|condition| SearchCondition {
            field: condition.field,
            op: SearchOp::Eq,
            ints: condition.ints,
            strings: condition.strings,
        })
        .collect()
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
    /// The thumbnail hash of each photo in order; the first is the main photo.
    pub photos: Vec<Vec<u8>>,
    /// The full image hash of each photo in `photos`, empty where a photo has none.
    pub full_photos: Vec<Vec<u8>>,
    /// The highest seen version.
    pub version: u64,
}

impl From<&dyapp_profile::VerifiedProfile> for FoundProfile {
    fn from(found: &dyapp_profile::VerifiedProfile) -> Self {
        let photos = &found.profile.photos;
        Self {
            peer_id: found.peer_id.clone(),
            draft: PublicProfileDraft::from(&found.profile),
            photos: photos.iter().map(|photo| photo.thumbnail.clone()).collect(),
            full_photos: photos
                .iter()
                .map(|photo| photo.full.first().cloned().unwrap_or_default())
                .collect(),
            version: found.profile.version,
        }
    }
}

/// One search over the active network's search nodes.
#[derive(Clone, Debug, uniffi::Record)]
pub struct ProfileSearchResult {
    /// Field numbers every reached node filters on and the client checks, ascending.
    pub offered: Vec<u32>,
    /// Verified matches in no set order, without blocked or passed peers.
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
    pub(crate) dir: PathBuf,
    pub(crate) runtime: tokio::runtime::Runtime,
    /// Verified profiles from searches and fetches; see `contact_names`.
    pub(crate) cache: Mutex<Profiles>,
    /// Other people's photos in memory; see `peer_photos`.
    pub(crate) photos: Mutex<PhotoCache>,
    binding: Option<SlotWorker>,
}

#[uniffi::export]
impl ProfileSearcher {
    /// Prepare a private Tokio runtime for this existing identity directory.
    #[uniffi::constructor]
    ///
    /// # Errors
    ///
    /// Returns an error if the identity directory is invalid or runtime creation fails.
    pub fn new(dir: String) -> Result<Arc<Self>, CoreError> {
        Self::create(dir, None)
    }

    /// Bind an idle searcher to a verified identity without retaining its registry lock.
    ///
    /// # Errors
    /// Returns identity admission, path or runtime errors without creating a database.
    #[uniffi::constructor]
    pub fn for_slot(
        registry: Arc<MobileIdentityRegistry>,
        id: String,
        expected_peer: String,
    ) -> Result<Arc<Self>, CoreError> {
        let binding = SlotWorker::new(registry, id, expected_peer);
        let operation = binding.operation()?;
        Self::create(operation.slot().directory, Some(binding))
    }
}

impl ProfileSearcher {
    fn create(dir: String, binding: Option<SlotWorker>) -> Result<Arc<Self>, CoreError> {
        database(&dir)?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(failed)?;
        Ok(Arc::new(Self {
            dir: dir.into(),
            runtime,
            cache: Mutex::new(Profiles::default()),
            photos: Mutex::default(),
            binding,
        }))
    }

    pub(crate) fn admit(
        &self,
        key: Option<&IdentityKey>,
    ) -> Result<Option<Arc<MobileIdentityOperation>>, CoreError> {
        let operation = self
            .binding
            .as_ref()
            .map(SlotWorker::operation)
            .transpose()?;
        if operation.as_ref().zip(key).is_some_and(|(operation, key)| {
            operation.operation.identity().peer_id() != key.identity.peer_id()
        }) {
            return Err(failed("The identity key does not match this worker"));
        }
        Ok(operation)
    }

    pub(crate) fn bound_operation(&self) -> Result<Arc<MobileIdentityOperation>, CoreError> {
        self.binding
            .as_ref()
            .ok_or_else(|| failed("Searcher is not bound to an identity"))?
            .operation()
    }
}

/// A node that never answered makes the sample partial, as a short answer does.
fn is_partial<T>(short_answer: bool, unreached: &[T]) -> bool {
    short_answer || !unreached.is_empty()
}

#[cfg(test)]
mod tests;
