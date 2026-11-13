//! Read and bind actual signed source bytes; never reconstruct the source digest by re-signing.
use super::{
    Error, Failure, Identity, Node, Profile, ProfilePublication, SignedRecord, Status, Store,
};

pub(super) fn foreign_source<'a>(
    store: &Store,
    identity: &Identity,
    attempt: &ProfilePublication,
    held: Option<&'a Profile>,
    next: &Profile,
) -> Result<Option<&'a Profile>, Failure> {
    if !attempt.source_matches(held, next) {
        return Err(Failure::Missed);
    }
    let draft = attempt.draft();
    let seen = held.map_or(0, |held| held.version);
    if attempt.source_checked() {
        super::admit_directory_version(store, seen)?;
    } else if store.own_profile_seen()? > seen.max(draft.version) {
        return Err(Error::Reply("the node holds an older own profile").into());
    }
    let Some(held) = held else {
        return Ok(None);
    };
    let foreign = attempt.directory_source().is_none()
        && !same_content(held, next)
        && super::from_another_device(store, identity, held, draft)?;
    Ok(foreign.then_some(held))
}

pub(super) fn publication_error(error: Error) -> Failure {
    match error {
        Error::Status(Status::Stale) => Failure::Missed,
        error => Failure::Node(error),
    }
}
use dyapp_profile::ProfileCondition;

pub(super) async fn newest(
    node: &mut Node,
    identity: &Identity,
) -> Result<Option<(SignedRecord, Profile)>, Error> {
    match node.get_profile_signed(&identity.peer_id()).await {
        Ok((record, held)) => Ok(Some((record, held.profile))),
        Err(Error::Status(Status::NotFound)) => Ok(None),
        Err(error) => Err(error),
    }
}

pub(super) fn same_content(left: &Profile, right: &Profile) -> bool {
    let mut normalized = left.clone();
    normalized.previous.clone_from(&right.previous);
    normalized == *right
}

pub(super) fn bind(
    next: &mut Profile,
    observed: Option<&(SignedRecord, Profile)>,
) -> Result<(), Error> {
    // A lost response retries the original authorization, not a new fence around its own target.
    if let Some((_, held)) = observed.filter(|(_, held)| same_content(next, held)) {
        next.previous.clone_from(&held.previous);
    } else {
        next.previous = Some(ProfileCondition::observed(
            observed.map(|(record, _)| record),
        )?);
    }
    Ok(())
}
