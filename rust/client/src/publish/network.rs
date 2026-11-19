//! Profile storage reads, conditional publication and signed readback health outcomes.
use super::{
    condition, photos, take_photos, Error, Failure, ForeignDraft, Identity, Node, Profile,
    ProfilePublication, ProfilePublicationOutcome, Store,
};
use crate::scores::Scores;
use crate::selection::Operation;
use dyapp_p2p_net::proto::Status;

pub(super) async fn send_prepared(
    store: &Store,
    node: &mut Node,
    identity: &Identity,
    attempt: &ProfilePublication,
    stay_hidden: bool,
    foreign: ForeignDraft,
    scores: &mut Scores,
) -> Result<ProfilePublicationOutcome, Failure> {
    let mut next = store.profile_publication_record(identity, attempt)?;
    let observed = condition::newest(node, identity, scores).await?;
    let held = observed.as_ref().map(|(_, profile)| profile);
    if let Some(held) = condition::foreign_source(store, identity, attempt, held, &next)? {
        if matches!(foreign, ForeignDraft::Import) {
            take_photos(store, node, held, scores).await?;
            store.commit_profile_import(identity, attempt, held)?;
        }
        return Err(Failure::Missed);
    }
    condition::bind(&mut next, observed.as_ref())?;
    let staged = photos::prepare(store, node, identity, &next, scores).await?;
    let profile = send(node, identity, next, scores)
        .await
        .map_err(condition::publication_error)?;
    photos::finish(store, node, identity, staged, &profile, scores).await?;
    Ok(store.commit_profile_publication(identity, attempt, &profile, stay_hidden)?)
}

// Publish `profile` to `node` and read it back. Refused as stale unless the node holds this same
// profile, from a publish whose reply was lost.
pub(super) async fn send(
    node: &mut Node,
    identity: &Identity,
    profile: Profile,
    scores: &mut Scores,
) -> Result<Profile, Error> {
    let peer = node.peer();
    let started = std::time::Instant::now();
    let result = send_inner(node, identity, profile).await;
    match &result {
        Ok(_) => scores.success(peer, Operation::ProfilePublish, started.elapsed()),
        Err(Error::Reply("readback differs from the published profile")) => {
            scores.invalid(peer, Operation::ProfilePublish);
        }
        Err(_) => scores.fault(peer, crate::now(), Operation::ProfilePublish),
    }
    result
}

async fn send_inner(
    node: &mut Node,
    identity: &Identity,
    profile: Profile,
) -> Result<Profile, Error> {
    let peer_id = identity.peer_id();
    let signed = profile.sign(identity);
    let result = if profile.previous.is_some() {
        node.publish_profile_conditional(signed.clone()).await
    } else {
        node.publish_profile(signed.clone()).await
    };
    match result {
        Ok(()) => {}
        Err(Error::Status(Status::Stale))
            if node.get_profile_signed(&peer_id).await?.0 == signed => {}
        Err(error) => return Err(error),
    }
    if node.get_profile_signed(&peer_id).await?.0 != signed {
        return Err(Error::Reply("readback differs from the published profile"));
    }
    Ok(profile)
}
