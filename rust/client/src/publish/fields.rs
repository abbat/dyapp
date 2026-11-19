//! High-level editable-field consent preserves verified network device authority.
use super::{
    admit_directory_version, Failure, Identity, Node, Profile, ProfilePublication,
    ProfilePublicationOutcome, Store,
};
use crate::scores::Scores;

/// Publish editable fields using the caller's persistent selection state.
///
/// # Errors
/// Refuses pending, absent, withdrawn or older authority and ordinary publication failures.
pub async fn publish_fields_prepared_with_scores(
    store: &Store,
    node: &mut Node,
    identity: &Identity,
    mut attempt: ProfilePublication,
    scores: &mut Scores,
) -> Result<ProfilePublicationOutcome, Failure> {
    prepare_fields(store, node, identity, &mut attempt, scores).await?;
    super::publish_prepared_with_scores(store, node, identity, &attempt, scores).await
}

pub(crate) async fn prepare_fields(
    store: &Store,
    node: &mut Node,
    identity: &Identity,
    attempt: &mut ProfilePublication,
    scores: &mut Scores,
) -> Result<Option<Profile>, Failure> {
    // Validate the captured owner/version before any network work or local admission changes.
    store.profile_publication_record(identity, attempt)?;
    if store.link_pending()?.is_some() {
        return Err(Failure::LinkPending);
    }
    let source = super::newest_with_scores(node, identity, scores).await?;
    match &source {
        Some(profile) if profile.deleted => return Err(Failure::DirectoryAuthorityRequired),
        Some(profile) => admit_directory_version(store, profile.version)?,
        None if store.ordinary_profile_floor()? != 0
            || store.is_linked_device()?
            || !attempt.draft().devices.is_empty() =>
        {
            return Err(Failure::DirectoryAuthorityRequired);
        }
        None => {}
    }
    attempt.fields_from(source.as_ref());
    Ok(source)
}
