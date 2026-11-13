//! Complete read-only editable data, including fields outside the current native editor.
use super::LinkReviewField;
use crate::PublicProfileDraft;
use dyapp_profile::Profile;

/// Ordered photo references; every original chunk is retained.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LinkReviewPhoto {
    /// Thumbnail content hash.
    pub thumbnail: Vec<u8>,
    /// Original image chunk hashes in order.
    pub full: Vec<Vec<u8>>,
}

/// Immutable editable values for comparison, never a client-supplied publication request.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LinkReviewProfile {
    /// Version of this exact snapshot, not an editable field.
    pub version: u64,
    /// Fields already supported by the native profile editor.
    pub fields: PublicProfileDraft,
    /// Optional lower income bound.
    pub income_from: Option<u64>,
    /// Optional upper income bound.
    pub income_to: Option<u64>,
    /// Whether the owner has children, including an unpublished value.
    pub has_kids: Option<bool>,
    /// Whether the owner wants more children, including an unpublished value.
    pub wants_more_kids: Option<bool>,
    /// Exact complete interest list.
    pub interests: Vec<String>,
    /// Ordered full photo references.
    pub photos: Vec<LinkReviewPhoto>,
    /// Education scale code, preserving forward-compatible remote values.
    pub education_level: u32,
    /// Profession text.
    pub profession: String,
    /// Company text.
    pub company: String,
    /// Industry text.
    pub industry: String,
    /// Career stage text.
    pub career_stage: String,
    /// Work-life balance text.
    pub work_life_balance: String,
}
impl From<&Profile> for LinkReviewProfile {
    fn from(profile: &Profile) -> Self {
        Self {
            version: profile.version,
            fields: profile.into(),
            income_from: profile.income_from,
            income_to: profile.income_to,
            has_kids: profile.has_kids,
            wants_more_kids: profile.wants_more_kids,
            interests: profile.interests.clone(),
            photos: profile
                .photos
                .iter()
                .map(|photo| LinkReviewPhoto {
                    thumbnail: photo.thumbnail.clone(),
                    full: photo.full.clone(),
                })
                .collect(),
            education_level: profile.education_level,
            profession: profile.profession.clone(),
            company: profile.company.clone(),
            industry: profile.industry.clone(),
            career_stage: profile.career_stage.clone(),
            work_life_balance: profile.work_life_balance.clone(),
        }
    }
}

/// Exact immutable values and the core's required choices.
#[derive(Clone, Debug, uniffi::Record)]
pub struct LinkReviewSnapshot {
    /// The local baseline captured before the remote read.
    pub local: LinkReviewProfile,
    /// Verified ordinary account profile, never overlaid with local choices.
    pub remote: LinkReviewProfile,
    /// Every one of these groups requires an explicit decision.
    pub differences: Vec<LinkReviewField>,
    /// Existing search privacy state.
    pub hidden: bool,
}

/// Saved local recovery draft; ordinary publication remains a separate consent-gated action.
#[derive(Clone, Debug, uniffi::Record)]
pub struct LinkReviewSaved {
    /// Exact ordinary remote profile that remains published.
    pub published: LinkReviewProfile,
    /// Full current local draft.
    pub draft: LinkReviewProfile,
    /// Search privacy after the atomic save.
    pub hidden: bool,
    /// Local values still differ from ordinary public content.
    pub pending: bool,
}
impl From<&dyapp_client::store::ProfilePublicationOutcome> for LinkReviewSaved {
    fn from(outcome: &dyapp_client::store::ProfilePublicationOutcome) -> Self {
        Self {
            published: (&outcome.published).into(),
            draft: (&outcome.draft).into(),
            hidden: outcome.hidden,
            pending: outcome.pending,
        }
    }
}
