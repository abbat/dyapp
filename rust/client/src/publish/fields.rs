//! High-level editable-field consent preserves verified network device authority.
use super::{
    admit_directory_version, newest, publish_prepared, Failure, Identity, Node, Profile,
    ProfilePublication, ProfilePublicationOutcome, Store,
};

/// Publish editable fields against verified ordinary directory authority, without enrolling devices.
/// Session-aware clients use `Session::publish_profile` to admit their original device as well.
///
/// # Errors
/// Pending enrollment, absent/withdrawn/older authority and normal publication failures are refused.
/// The low-level `publish`/`publish_prepared` owner-signing capability remains separate.
pub async fn publish_fields_prepared(
    store: &Store,
    node: &mut Node,
    identity: &Identity,
    mut attempt: ProfilePublication,
) -> Result<ProfilePublicationOutcome, Failure> {
    prepare_fields(store, node, identity, &mut attempt).await?;
    publish_prepared(store, node, identity, &attempt).await
}

pub(crate) async fn prepare_fields(
    store: &Store,
    node: &mut Node,
    identity: &Identity,
    attempt: &mut ProfilePublication,
) -> Result<Option<Profile>, Failure> {
    // Validate the captured owner/version before any network work or local admission changes.
    store.profile_publication_record(identity, attempt)?;
    if store.link_pending()?.is_some() {
        return Err(Failure::LinkPending);
    }
    let source = newest(node, identity).await?;
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
