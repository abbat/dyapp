//! Explicit owner choices update the local draft; public profile publication stays a separate action.
use super::{
    AdoptionField, AdoptionReview, AdoptionSelectionError, AdoptionSource, Failure, Identity, Node,
    ProfilePublicationOutcome,
};
use crate::session::Session;

/// Confirmation can fail without changing the local projection; cache fetches may already have run.
#[derive(Debug, thiserror::Error)]
pub enum AdoptionConfirmError {
    /// Incomplete or malformed owner choices.
    #[error(transparent)]
    Selection(#[from] AdoptionSelectionError),
    /// Selected content violates the shared profile wire contract.
    #[error(transparent)]
    Profile(#[from] dyapp_profile::Error),
    /// The original sealed device credential cannot be read.
    #[error("linked-device credential is unavailable: {0}")]
    Credential(#[source] Box<crate::session::Error>),
    /// This device is not already listed by the reviewed remote profile.
    #[error("device listing must be resolved before confirming the profile review")]
    DeviceNotListed,
    /// Changed context, unavailable remote readback or failed storage completion.
    #[error(transparent)]
    Recovery(#[from] Failure),
}

/// Save explicitly reviewed fields locally, preserving exact remote authorization and visibility.
/// No account profile is published; chosen local values remain pending for normal publication.
/// The pending link stays until its existing key-package/fallback completion succeeds.
///
/// # Errors
/// Rejects invalid selections/content, another or changed owner/context, an unlisted credential,
/// changed/unavailable remote readback, version exhaustion or local transaction failures.
pub async fn confirm_adoption_review(
    session: &Session,
    node: &mut Node,
    identity: &Identity,
    review: &AdoptionReview,
    choices: &[(AdoptionField, AdoptionSource)],
) -> Result<ProfilePublicationOutcome, AdoptionConfirmError> {
    let store = session.store();
    let selected = review.select_fields(choices)?;
    store
        .validate_reviewed_adoption(
            identity,
            &review.context,
            review.remote_profile(),
            &selected,
        )
        .map_err(review_failure)?;
    let certificate = session
        .certificate()
        .map_err(|error| AdoptionConfirmError::Credential(Box::new(error)))?;
    if !review.remote_profile().devices.contains(&certificate) {
        return Err(AdoptionConfirmError::DeviceNotListed);
    }
    dyapp_profile::verify(&selected.sign(identity))?;
    super::take_photos(store, node, &selected).await?;
    let current = super::newest(node, identity).await.map_err(Failure::from)?;
    if current.as_ref() != Some(review.remote_profile()) {
        return Err(Failure::AdoptionReviewRequired.into());
    }
    store
        .commit_reviewed_adoption(
            identity,
            &review.context,
            review.remote_profile(),
            &selected,
        )
        .map_err(review_failure)
        .map_err(AdoptionConfirmError::from)
}

fn review_failure(error: crate::store::Error) -> Failure {
    match error {
        crate::store::Error::ProfileCompletionSuperseded => Failure::AdoptionReviewRequired,
        error => error.into(),
    }
}
