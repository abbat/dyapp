//! Staged ownership keeps old references until the exact public target is confirmed.
use super::{patiently, photo_hashes, take_photo_hashes, Failure, Identity, Node, Profile, Store};
use crate::media_ownership::{Intent, Replaced, State};
use crate::Error;
use dyapp_identity::{sha256, SignedRecord};
use dyapp_p2p_net::proto::Status;
use prost::Message as _;

const CONFLICT_ATTEMPTS: usize = 8;

pub(super) struct Staged {
    target: [u8; 32],
    state: State,
}

fn digest(record: &SignedRecord) -> [u8; 32] {
    sha256(&record.encode_to_vec())
}

fn union(state: &State, hashes: &[Vec<u8>]) -> Vec<Vec<u8>> {
    let mut all = state.hashes().to_vec();
    all.extend_from_slice(hashes);
    all.sort();
    all.dedup();
    all
}

async fn upload(
    store: &Store,
    node: &mut Node,
    identity: &Identity,
    missing: Vec<Vec<u8>>,
    required: &[Vec<u8>],
) -> Result<(), Failure> {
    let required_missing: Vec<_> = missing
        .into_iter()
        .filter(|hash| required.contains(hash))
        .collect();
    take_photo_hashes(store, node, required_missing.clone()).await?;
    for hash in required_missing {
        let blob = store
            .profile_photo(&hash)?
            .ok_or(Error::Reply("kept photo is not in the store"))?;
        patiently(async || node.put_media(&identity.peer_id(), blob.clone()).await).await?;
    }
    Ok(())
}

/// A keep write right after the photo uploads can meet the node's media rate limit.
async fn replace(
    node: &mut Node,
    identity: &Identity,
    state: &State,
    hashes: Vec<Vec<u8>>,
    intent: Intent,
) -> Result<Replaced, Error> {
    patiently(async || {
        node.replace_media_keep(identity, state, hashes.clone(), intent)
            .await
    })
    .await
}

async fn state_for(
    node: &mut Node,
    identity: &Identity,
    hashes: &[Vec<u8>],
) -> Result<Option<State>, Failure> {
    match patiently(async || node.media_keep_state(identity).await).await {
        Ok(state) => Ok(Some(state)),
        // A photo-free profile can use a store without the media role. No weaker write is made.
        Err(Error::Status(Status::Unsupported)) if hashes.is_empty() => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub(super) async fn prepare(
    store: &Store,
    node: &mut Node,
    identity: &Identity,
    profile: &Profile,
) -> Result<Option<Staged>, Failure> {
    let hashes = photo_hashes(profile);
    let Some(mut state) = state_for(node, identity, &hashes).await? else {
        return Ok(None);
    };
    let target = digest(&profile.sign(identity));
    for _ in 0..CONFLICT_ATTEMPTS {
        let all = union(&state, &hashes);
        let intent = Intent {
            profile_hash: target,
            pending: true,
        };
        match replace(node, identity, &state, all, intent).await? {
            Replaced::Applied { state, missing } => {
                upload(store, node, identity, missing, &hashes).await?;
                return Ok(Some(Staged { target, state }));
            }
            Replaced::Conflict(current) => state = current,
        }
    }
    Err(Error::Status(Status::Stale).into())
}

async fn live(node: &mut Node, identity: &Identity) -> Result<(Profile, [u8; 32]), Failure> {
    let (record, verified) = node.get_profile_signed(&identity.peer_id()).await?;
    Ok((verified.profile, digest(&record)))
}

pub(super) async fn finish(
    store: &Store,
    node: &mut Node,
    identity: &Identity,
    staged: Option<Staged>,
    profile: &Profile,
) -> Result<(), Failure> {
    let Some(Staged { target, mut state }) = staged else {
        return Ok(());
    };
    let hashes = photo_hashes(profile);
    for _ in 0..CONFLICT_ATTEMPTS {
        if state.intent_hash().is_none() {
            return Err(Error::Status(Status::Stale).into());
        }
        if state.intent_hash() != Some(target) || !state.pending() {
            // Another prepared publication owns the list; retain all of its references.
            return Ok(());
        }
        if live(node, identity).await?.1 != target {
            return Err(Error::Status(Status::Stale).into());
        }
        let intent = Intent {
            profile_hash: target,
            pending: false,
        };
        match replace(node, identity, &state, hashes.clone(), intent).await? {
            Replaced::Applied { missing, .. } => {
                return upload(store, node, identity, missing, &hashes).await;
            }
            Replaced::Conflict(current) => state = current,
        }
    }
    Err(Error::Status(Status::Stale).into())
}

fn renewal(state: &State, target: [u8; 32], hashes: &[Vec<u8>]) -> (Vec<Vec<u8>>, Intent) {
    match state.intent_hash() {
        Some(other) if other != target => (
            union(state, hashes),
            Intent {
                profile_hash: other,
                pending: state.pending(),
            },
        ),
        Some(_) => (
            hashes.to_vec(),
            Intent {
                profile_hash: target,
                pending: false,
            },
        ),
        None => (
            union(state, hashes),
            Intent {
                profile_hash: target,
                pending: false,
            },
        ),
    }
}

pub(super) async fn refresh(
    store: &Store,
    node: &mut Node,
    identity: &Identity,
) -> Result<(), Failure> {
    let (profile, target) = live(node, identity).await?;
    let hashes = photo_hashes(&profile);
    let Some(mut state) = state_for(node, identity, &hashes).await? else {
        return Ok(());
    };
    for _ in 0..CONFLICT_ATTEMPTS {
        let (all, intent) = renewal(&state, target, &hashes);
        match replace(node, identity, &state, all, intent).await? {
            Replaced::Applied { missing, .. } => {
                upload(store, node, identity, missing, &hashes).await?;
                return Ok(());
            }
            Replaced::Conflict(current) => state = current,
        }
    }
    Err(Error::Status(Status::Stale).into())
}
