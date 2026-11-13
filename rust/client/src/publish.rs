//! Publishing the owner's profile draft: the checks before each publish, the cancellable proof
//! of work, and the publish with its signed readback.
//!
//! A caller runs [`check`], calls [`work`] in short steps (saving the draft after each, so a
//! cancelled publish resumes where it stopped), then [`publish`].

use crate::store::profile_photos::{fits, MAX_PHOTO_BYTES};
use crate::store::{self, ProfilePublication, ProfilePublicationOutcome, Store};
use crate::{Error, Node};
use dyapp_identity::{key_hash, Identity, SignedRecord};
use dyapp_p2p_net::proto::Status;
use dyapp_profile::{CivilDate, DeviceCertificate, Profile};
use prost::Message as _;

mod photos;

/// Why a draft cannot be published yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Block {
    /// The owner has not confirmed being 18 or older for this publish.
    #[error("confirm you are 18 or older")]
    NotAttested,
    /// The year, month and day parts describe no calendar date.
    #[error("invalid birth date")]
    InvalidBirthDate,
    /// With a year, the youngest possible age is unknown or under 18.
    #[error("the birth date gives an age under 18")]
    Underage,
}

/// Check `draft` at `today`, the current UTC civil date. `attested` is the owner's fresh 18+
/// confirmation for this publish; without a birth year it is the only age check.
///
/// # Errors
///
/// Returns an error if fresh age attestation is missing, the birth-date parts are
/// invalid, or a supplied birth year does not establish an age of at least 18.
pub fn check(draft: &Profile, attested: bool, today: CivilDate) -> Result<(), Block> {
    if !attested {
        return Err(Block::NotAttested);
    }
    if !draft.valid_birth_parts() {
        return Err(Block::InvalidBirthDate);
    }
    if draft.birth_year.is_some()
        && draft
            .possible_age_range(today)
            .is_none_or(|(youngest, _)| youngest < 18)
    {
        return Err(Block::Underage);
    }
    Ok(())
}

/// List `certificate` among `draft`'s devices once, in the mailbox order a profile requires, with
/// the device's [`owner_binding`](crate::owner_binding) that lets another own device close its
/// mailbox.
pub fn list_device(draft: &mut Profile, certificate: SignedRecord, binding: SignedRecord) {
    if !draft.device_owners.contains(&binding) {
        draft.device_owners.push(binding);
    }
    if !draft.devices.contains(&certificate) {
        draft.devices.push(certificate);
        draft.devices.sort_by_key(|signed| {
            let device_key = DeviceCertificate::decode(signed.payload.as_slice())
                .ok()?
                .device_key;
            Some(key_hash(&device_key.try_into().ok()?))
        });
    }
}

/// The mailbox keys of the devices `profile` lists.
#[must_use]
pub fn device_keys(profile: &Profile) -> Vec<Vec<u8>> {
    profile
        .devices
        .iter()
        .filter_map(|signed| DeviceCertificate::decode(signed.payload.as_slice()).ok())
        .map(|certificate| certificate.device_key)
        .collect()
}

/// Takes the device with the mailbox key `device_key` out of `draft`. Returns its owner binding,
/// or `None` if the draft holds none and its mailbox cannot be closed.
pub fn unlist_device(draft: &mut Profile, device_key: &[u8]) -> Option<SignedRecord> {
    let binding = draft
        .device_owners
        .iter()
        .position(|binding| binding.public_key == device_key)?;
    draft.devices.retain(|signed| {
        DeviceCertificate::decode(signed.payload.as_slice())
            .map_or(true, |certificate| certificate.device_key != device_key)
    });
    Some(draft.device_owners.remove(binding))
}

/// Try up to `tries` more nonces for a profile proof of work of `bits` for `key`, the owner's
/// public key. Progress stays in `draft.pow_nonce`; `draft.pow_bits` is set only once solved,
/// and the proof does not depend on other fields, so later versions reuse it. Returns whether
/// it is solved. An unfinished scratch step clears the proof claim. Persistent clients use
/// `Store::prepare_profile_work` and `Store::commit_profile_work` to retain existing valid proofs
/// separately from resume cursors.
pub fn work(draft: &mut Profile, key: &[u8; 32], bits: u32, tries: u64) -> bool {
    if draft.pow_bits >= bits {
        return true;
    }
    match dyapp_profile::solve(key, bits, draft.pow_nonce, tries) {
        Ok(nonce) => {
            (draft.pow_nonce, draft.pow_bits) = (nonce, bits);
            true
        }
        Err(next) => {
            draft.pow_nonce = next;
            // This is a scratch cursor, not a proof for a previously solved lower target.
            draft.pow_bits = 0;
            false
        }
    }
}

/// Publish `draft` (version: the last one published) to `node` at the next version and read it
/// back. A profile another device published first is never overwritten: it replaces the saved
/// draft and the publish fails with [`Failure::Missed`]. A node holding less than this device has
/// seen is refused, since it may have lost a device list change.
/// Succeeds only when signed readback equals the submitted profile. Own completion atomically
/// preserves post-start draft value changes, queues exact published data and projects visibility
/// and protected photo cleanup. The return value is the exact published profile, not pending edits.
/// This compatibility wrapper captures at invocation; callers with earlier awaits use
/// [`publish_prepared`]. A failure may follow remote success; local completion rolls back together.
/// A hidden profile is shown again unless a newer local operation superseded this completion.
/// Photos reach the node first, so the profile never lists a blob the node has
/// refused; blobs the published profile no longer lists then leave the store.
///
/// # Errors
///
/// Returns an error if storage or node requests fail, a photo to upload is absent,
/// the node holds an older profile than this device has seen, or signed readback
/// differs, or the requested version cannot be advanced. Returns [`Failure::Missed`] after
/// saving a newer foreign profile. Version exhaustion is refused before submitting a profile.
/// Remote publication or some local updates may already have succeeded.
pub async fn publish(
    store: &Store,
    node: &mut Node,
    identity: &Identity,
    draft: &Profile,
) -> Result<Profile, Failure> {
    let mut attempt = store.prepare_profile_publication(identity)?;
    *attempt.draft_mut() = draft.clone();
    Ok(publish_prepared(store, node, identity, &attempt)
        .await?
        .published)
}

/// Publish a prepared request captured before earlier network work, returning exact and pending state.
///
/// # Errors
/// Returns the same network/storage failures as [`publish`], or a superseded local completion.
/// Remote publication can succeed before local completion is refused.
pub async fn publish_prepared(
    store: &Store,
    node: &mut Node,
    identity: &Identity,
    attempt: &ProfilePublication,
) -> Result<ProfilePublicationOutcome, Failure> {
    send_prepared(store, node, identity, attempt, false, ForeignDraft::Import).await
}

/// Relist a prepared device change, retaining the visibility captured with its baseline.
///
/// # Errors
/// Returns the same publication/completion errors as [`publish_prepared`].
pub async fn relist_prepared(
    store: &Store,
    node: &mut Node,
    identity: &Identity,
    attempt: &ProfilePublication,
) -> Result<ProfilePublicationOutcome, Failure> {
    send_prepared(
        store,
        node,
        identity,
        attempt,
        attempt.hidden(),
        ForeignDraft::Import,
    )
    .await
}

/// [`publish`] a device list change, which leaves a hidden profile hidden. Returns the new draft.
///
/// # Errors
///
/// Returns the same publication errors as [`publish`], or an error reading the
/// saved visibility state. Remote or local updates may already have succeeded.
pub async fn relist(
    store: &Store,
    node: &mut Node,
    identity: &Identity,
    draft: &Profile,
) -> Result<Profile, Failure> {
    let mut attempt = store.prepare_profile_publication(identity)?;
    *attempt.draft_mut() = draft.clone();
    Ok(relist_prepared(store, node, identity, &attempt)
        .await?
        .draft)
}

/// Revokes the owner's device with the mailbox key `device_key`: closes its mailbox, then [`relist`]s
/// `draft` without it. A newer profile at the node enriches the prepared request, retaining the
/// local baseline captured before network work so concurrent edits survive completion. A failure may
/// be retried: repeated close is harmless and the saved draft keeps its binding until relist. Returns the
/// new draft.
///
/// # Errors
///
/// Returns an error if the device has no owner binding, storage or node requests
/// fail, or relisting fails as described in [`relist`]. A newer draft may already
/// be saved or the device mailbox closed when a later step fails.
pub async fn revoke_device(
    store: &Store,
    node: &mut Node,
    identity: &Identity,
    draft: &Profile,
    device_key: &[u8],
) -> Result<Profile, Failure> {
    let mut attempt = store.prepare_profile_publication(identity)?;
    let request = match newest(node, identity).await? {
        Some(mut held) if held.version > draft.version => {
            held.version = held
                .version
                .checked_add(u64::from(attempt.hidden()))
                .ok_or(store::Error::ProfileVersionExhausted)?;
            take_photos(store, node, &held).await?;
            held
        }
        _ => draft.clone(),
    };
    *attempt.draft_mut() = request;
    let binding = unlist_device(attempt.draft_mut(), device_key)
        .ok_or(Error::Input("device has no owner binding"))?;
    // Refuse exhausted/invalid requests before the irreversible mailbox closure.
    store.profile_publication_record(identity, &attempt)?;
    node.close_mailbox(identity, binding).await?;
    Ok(send_prepared(
        store,
        node,
        identity,
        &attempt,
        attempt.hidden(),
        ForeignDraft::PreserveLocal,
    )
    .await?
    .draft)
}

/// Starts the fresh store of a linked device from the record's `state` and the newest profile with
/// its photos, kept as the draft. Returns exact ordinary readback separately from current draft
/// and visibility. A failure may be retried. The caller then prepares its device change and calls
/// [`relist_prepared`]; a hidden draft's later search version never proves device listing.
///
/// # Errors
///
/// Returns an error if importing link state, reading or saving local state,
/// fetching the profile or its photos fails, or the account profile is missing
/// or deleted, or its hidden draft version cannot be advanced. Exhaustion is checked before
/// fetching photos or replacing the draft. Earlier link-state imports remain applied on failure.
pub async fn adopt(
    store: &mut Store,
    node: &mut Node,
    identity: &Identity,
    state: &[u8],
    now: u64,
) -> Result<ProfilePublicationOutcome, Failure> {
    store.import_link_state(state)?;
    let held = newest(node, identity)
        .await?
        .ok_or(Error::Reply("the account has no profile"))?;
    if held.deleted {
        return Err(Error::Reply("the account was removed").into());
    }
    let version = held
        .version
        .checked_add(u64::from(store.profile_hidden()?))
        .ok_or(store::Error::ProfileVersionExhausted)?;
    take_photos(store, node, &held).await?;
    store.own_profile_age(held.version, now)?;
    let draft = Profile {
        version,
        ..held.clone()
    };
    store.save_profile_draft(&draft)?;
    Ok(store.profile_publication_state(identity, &held)?)
}

enum ForeignDraft {
    Import,
    PreserveLocal,
}

async fn send_prepared(
    store: &Store,
    node: &mut Node,
    identity: &Identity,
    attempt: &ProfilePublication,
    stay_hidden: bool,
    foreign: ForeignDraft,
) -> Result<ProfilePublicationOutcome, Failure> {
    let draft = attempt.draft();
    let next = store.profile_publication_record(identity, attempt)?;
    let held = newest(node, identity).await?;
    let seen = held.as_ref().map_or(0, |held| held.version);
    if store.own_profile_seen()? > seen.max(draft.version) {
        return Err(Error::Reply("the node holds an older own profile").into());
    }
    if let Some(held) = held {
        if held != next && from_another_device(store, identity, &held, draft)? {
            if matches!(foreign, ForeignDraft::Import) {
                take_photos(store, node, &held).await?;
                store.commit_profile_import(identity, attempt, &held)?;
            }
            return Err(Failure::Missed);
        }
    }
    let staged = photos::prepare(store, node, identity, &next).await?;
    let profile = send(node, identity, next).await?;
    photos::finish(store, node, identity, staged, &profile).await?;
    Ok(store.commit_profile_publication(identity, attempt, &profile, stay_hidden)?)
}

// Equal hidden versions are foreign except our withdrawal. Its exact remembered bytes also
// identify it after a concurrent proof improvement makes the draft's rebuilt tombstone differ.
fn from_another_device(
    store: &Store,
    identity: &Identity,
    held: &Profile,
    draft: &Profile,
) -> Result<bool, store::Error> {
    if held.version != draft.version {
        return Ok(held.version > draft.version);
    }
    if !store.profile_hidden()? {
        return Ok(false);
    }
    let legacy = Profile::tombstone(draft.version, draft.pow_nonce, draft.pow_bits);
    Ok(held != &legacy && !(held.deleted && store.search_profile_known(&held.sign(identity))?))
}

// Fetch the blobs of `profile` this store lacks, for a publish that may have to put them back. One
// a node lost stays missing: nothing here can restore it.
async fn take_photos(store: &Store, node: &mut Node, profile: &Profile) -> Result<(), Failure> {
    take_photo_hashes(store, node, photo_hashes(profile)).await
}

async fn take_photo_hashes(
    store: &Store,
    node: &mut Node,
    hashes: Vec<Vec<u8>>,
) -> Result<(), Failure> {
    for hash in hashes {
        let Ok(key) = <[u8; 32]>::try_from(hash.as_slice()) else {
            continue;
        };
        if store.profile_photo(&hash)?.is_some() {
            continue;
        }
        match patiently(async || node.get_media(&key).await).await {
            Ok(blob) if fits(&blob, MAX_PHOTO_BYTES) => {
                store.save_profile_blob(&blob)?;
            }
            Ok(_) | Err(Error::Status(Status::NotFound)) => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

// Repeat a media request the node refused for its rate.
async fn patiently<T>(mut request: impl AsyncFnMut() -> Result<T, Error>) -> Result<T, Error> {
    let mut waits = 0;
    loop {
        match request().await {
            Err(Error::Status(Status::RateLimited)) if waits < PUT_WAITS => {
                waits += 1;
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
            done => return done,
        }
    }
}

/// The owner's profile at `node`, if it has one.
///
/// # Errors
///
/// Returns an error if the profile request or reply validation fails, except
/// that a node's not-found response returns `Ok(None)`.
pub async fn newest(node: &mut Node, identity: &Identity) -> Result<Option<Profile>, Error> {
    match node.get_profile(&identity.peer_id()).await {
        Ok(held) => Ok(Some(held.profile)),
        Err(Error::Status(Status::NotFound)) => Ok(None),
        Err(error) => Err(error),
    }
}

fn photo_hashes(profile: &Profile) -> Vec<Vec<u8>> {
    let mut hashes: Vec<Vec<u8>> = profile
        .photos
        .iter()
        .flat_map(|photo| photo.full.iter().chain([&photo.thumbnail]).cloned())
        .collect();
    hashes.sort();
    hashes.dedup();
    hashes
}

/// One-second waits for a blob the node refused for its rate. Six photos are 13 requests, and a
/// node takes ten media requests a second by default.
const PUT_WAITS: u32 = 5;

/// Renews the node's hold on the published photos at most once a day and puts back whatever it
/// lost. The list comes from the live profile: a draft may hold edits that are not published, and
/// a keep with them would release a photo readers still see. A failed attempt waits for the next
/// day, because a failure that repeats would otherwise cost requests on every pass.
///
/// # Errors
///
/// Returns an error if stored refresh state is invalid, database access or node
/// requests fail, or a photo requested for upload is absent locally. Once the
/// attempt is recorded, a later failure still delays the next attempt.
pub async fn refresh_photos(
    store: &Store,
    node: &mut Node,
    identity: &Identity,
    now: u64,
) -> Result<(), Failure> {
    if !store.profile_keep_due(now)? {
        return Ok(());
    }
    store.profile_keep_refreshed(now)?;
    photos::refresh(store, node, identity).await
}

/// Hide from search with a signed tombstone held for search nodes, leaving the last ordinary
/// profile and device directory at store nodes. The caller sends the held record and reports a
/// pending withdrawal until a search node accepts it. The saved draft advances to that version.
///
/// # Errors
///
/// Returns an error for owner/version/state or persistence failures. Draft, search tombstone
/// and hidden flag update atomically; a stale request never replaces newer local fields.
pub fn hide(store: &Store, identity: &Identity, draft: &Profile) -> Result<Profile, Failure> {
    Ok(store.hide_profile_draft(identity, draft)?)
}

/// Withdraw an identity before local removal: publish a final tombstone to profile storage,
/// read it back, then hold it for search. No local key is removed here; the caller decides
/// whether to retry a failed node request or remove anyway.
///
/// # Errors
///
/// Returns an error if storage or node requests fail or the signed tombstone
/// readback differs, or a newer local/observed/search version superseded this completion.
/// The tombstone may already be published, but draft/search/hidden changes commit atomically.
/// Current local fields, photos, devices and stronger proof survive. Conditional photo ownership
/// is staged before sending and finalized only for the exact accepted tombstone; failures retain
/// references and return an error so the caller can retry.
pub async fn withdraw(
    store: &Store,
    node: &mut Node,
    identity: &Identity,
    draft: &Profile,
) -> Result<Profile, Failure> {
    let held = newest(node, identity).await?.map_or(0, |held| held.version);
    let next = store.prepare_profile_withdrawal(identity, draft, held)?;
    let staged = photos::prepare(store, node, identity, &next).await?;
    let tombstone = send(node, identity, next).await?;
    photos::finish(store, node, identity, staged, &tombstone).await?;
    Ok(store.commit_profile_withdrawal(identity, draft, &tombstone)?)
}

// Publish `profile` to `node` and read it back. Refused as stale unless the node holds this same
// profile, from a publish whose reply was lost.
async fn send(node: &mut Node, identity: &Identity, profile: Profile) -> Result<Profile, Error> {
    let peer_id = identity.peer_id();
    match node.publish_profile(profile.sign(identity)).await {
        Ok(()) => {}
        Err(Error::Status(Status::Stale))
            if node.get_profile(&peer_id).await?.profile == profile => {}
        Err(error) => return Err(error),
    }
    if node.get_profile(&peer_id).await?.profile != profile {
        return Err(Error::Reply("readback differs from the published profile"));
    }
    Ok(profile)
}

/// Why [`publish`] failed; all are retryable.
#[derive(Debug, thiserror::Error)]
pub enum Failure {
    /// The node refused, did not answer, or read back something else.
    #[error(transparent)]
    Node(#[from] Error),
    /// The published profile could not be saved locally.
    #[error(transparent)]
    Store(#[from] store::Error),
    /// Another device published first. Its profile is now the saved draft, so the owner sees it
    /// before publishing again. Pre-attempt unpublished edits give way to it; later edits remain pending.
    #[error("the profile changed on another device")]
    Missed,
}

#[cfg(test)]
mod tests {
    use super::{
        check, device_keys, hide, list_device, publish, unlist_device, withdraw, work, Block,
        Failure,
    };
    use crate::store::{tests::temp_db_path, Store};
    use crate::{Error, Node};
    use dyapp_identity::{Identity, SignedRecord};
    use dyapp_p2p_net::proto::{self, profile_request::Request, Status};
    use dyapp_p2p_net::{build_configured_swarm, BehaviourEvent, Protocols, SwarmOptions};
    use dyapp_profile::{CivilDate, Profile};
    use libp2p::futures::StreamExt;
    use libp2p::identity::Keypair;
    use libp2p::request_response::{Event as RequestEvent, Message as RequestMessage};
    use libp2p::swarm::SwarmEvent;
    use std::sync::{Arc, Mutex};

    #[path = "foreign_import_tests.rs"]
    mod foreign_import_tests;
    #[path = "node.rs"]
    mod node;
    #[path = "publication_tests.rs"]
    mod publication_tests;
    #[path = "revocation_failure_tests.rs"]
    mod revocation_failure_tests;
    #[path = "revocation_tests.rs"]
    mod revocation_tests;
    #[path = "version_tests.rs"]
    mod version_tests;
    #[path = "withdrawal_identity_tests.rs"]
    mod withdrawal_identity_tests;
    #[path = "withdrawal_media_tests.rs"]
    mod withdrawal_media_tests;
    #[path = "withdrawal_tests.rs"]
    mod withdrawal_tests;

    fn draft(year: Option<u32>, month: Option<u32>, day: Option<u32>) -> Profile {
        Profile {
            gender: dyapp_profile::Gender::Woman.code(),
            birth_year: year,
            birth_month: month,
            birth_day: day,
            ..Profile::default()
        }
    }

    #[test]
    fn gates_each_publish_on_attestation_and_age() {
        let today = CivilDate::new(2026, 10, 26).unwrap();
        assert_eq!(check(&draft(None, None, None), true, today), Ok(()));
        assert_eq!(check(&draft(None, Some(2), Some(29)), true, today), Ok(()));
        assert_eq!(
            check(&draft(None, None, None), false, today),
            Err(Block::NotAttested)
        );
        let mut genderless = draft(None, None, None);
        genderless.gender = 0;
        assert_eq!(check(&genderless, true, today), Ok(()));
        assert_eq!(
            check(&draft(None, Some(4), Some(31)), true, today),
            Err(Block::InvalidBirthDate)
        );
        // Turns 18 today, turns 18 tomorrow, and a year that may still be under 18.
        assert_eq!(
            check(&draft(Some(2008), Some(10), Some(26)), true, today),
            Ok(())
        );
        assert_eq!(
            check(&draft(Some(2008), Some(10), Some(27)), true, today),
            Err(Block::Underage)
        );
        assert_eq!(
            check(&draft(Some(2008), None, None), true, today),
            Err(Block::Underage)
        );
        assert_eq!(check(&draft(Some(2007), None, None), true, today), Ok(()));
        // A future year gives no age at all.
        assert_eq!(
            check(&draft(Some(2030), None, None), true, today),
            Err(Block::Underage)
        );
    }

    #[test]
    fn puzzle_work_resumes_after_a_cancelled_step() {
        let key = Identity::from_secret(&[7; 32]).public_key();
        let mut draft = draft(None, None, None);
        let mut steps = 1;
        while !work(&mut draft, &key, 4, 1) {
            assert_eq!((draft.pow_nonce, draft.pow_bits), (steps, 0));
            steps += 1;
        }
        assert_eq!(draft.pow_bits, 4);
        assert!(dyapp_profile::pow_leading_bits(&key, draft.pow_nonce) >= 4);
        let solved = draft.clone();
        assert!(work(&mut draft, &key, 4, 0));
        assert_eq!(draft, solved);
        assert!(!work(&mut draft, &key, 257, 1));
        assert_eq!(draft.pow_bits, 0);
        assert_eq!(draft.pow_nonce, solved.pow_nonce.saturating_add(1));
    }

    // A profile node's answer: it keeps the newest record in `held`, but answers a publish with
    // the next of `refuse` while any is left.
    fn answer(
        held: &mut Option<SignedRecord>,
        refuse: &mut impl Iterator<Item = Status>,
        request: Request,
    ) -> proto::ProfileResponse {
        let (status, record) = match request {
            Request::Publish(record) => {
                let version = dyapp_profile::stored_version(&record).unwrap();
                let stored = held
                    .as_ref()
                    .map(|r| dyapp_profile::stored_version(r).unwrap());
                match refuse.next() {
                    Some(status) => (status, None),
                    None if stored >= Some(version) => (Status::Stale, None),
                    None => {
                        *held = Some(record);
                        (Status::Ok, None)
                    }
                }
            }
            Request::Get(_) => match held.clone() {
                Some(record) => (Status::Ok, Some(record)),
                None => (Status::NotFound, None),
            },
            other => panic!("unexpected request {other:?}"),
        };
        proto::ProfileResponse {
            status: status.into(),
            record,
            ..Default::default()
        }
    }

    async fn profile_node(refuse: Vec<Status>) -> (Node, Arc<Mutex<Option<SignedRecord>>>) {
        profile_node_observed(refuse, |_, _| {}).await
    }

    async fn profile_node_observed(
        refuse: Vec<Status>,
        on_reply: impl FnMut(&Request, &proto::ProfileResponse) + Send + 'static,
    ) -> (Node, Arc<Mutex<Option<SignedRecord>>>) {
        node::observed(refuse, on_reply, |_| {}).await
    }

    #[tokio::test]
    async fn a_refused_publish_keeps_the_draft_and_a_retry_reads_back_the_preview() {
        let alice = Identity::generate();
        let store = Store::open(&temp_db_path(), &alice).unwrap();
        let preview = draft(None, Some(5), None);
        store.save_profile_draft(&preview).unwrap();
        let (mut node, held) = profile_node(vec![Status::Denied]).await;
        assert!(matches!(
            publish(&store, &mut node, &alice, &preview).await,
            Err(Failure::Node(Error::Status(Status::Denied)))
        ));
        assert_eq!(store.profile_draft().unwrap(), Some(preview.clone()));
        let published = publish(&store, &mut node, &alice, &preview).await.unwrap();
        assert_eq!(
            published,
            Profile {
                version: 1,
                ..preview.clone()
            }
        );
        assert_eq!(store.profile_draft().unwrap(), Some(published.clone()));
        let record = held.lock().unwrap().clone().unwrap();
        assert_eq!(dyapp_profile::verify(&record).unwrap().profile, published);
        assert_eq!(store.due_search_profile(u64::MAX).unwrap(), Some(record));
        // A lost reply: the retry finds this version already held and succeeds.
        let again = Store::open(&temp_db_path(), &alice).unwrap();
        assert_eq!(
            publish(&again, &mut node, &alice, &preview).await.unwrap(),
            published
        );
    }

    #[tokio::test]
    async fn hide_only_withdraws_search_and_publish_shows_again() {
        let alice = Identity::generate();
        let store = Store::open(&temp_db_path(), &alice).unwrap();
        let (mut node, held) = profile_node(Vec::new()).await;
        let shown = draft(None, None, None);
        let shown = publish(&store, &mut node, &alice, &shown).await.unwrap();
        store
            .search_profile_sent(&store.due_search_profile(1_000).unwrap().unwrap(), 1_000)
            .unwrap();
        let hidden = hide(&store, &alice, &shown).unwrap();
        assert_eq!((hidden.version, hidden.gender), (2, shown.gender));
        assert_eq!(store.profile_draft().unwrap(), Some(hidden.clone()));
        assert!(store.profile_hidden().unwrap());
        let retained = held.lock().unwrap().clone().unwrap();
        assert_eq!(dyapp_profile::verify(&retained).unwrap().profile, shown);
        let tombstone = store.due_search_profile(1_001).unwrap().unwrap();
        assert_eq!(
            dyapp_profile::verify(&tombstone).unwrap().profile,
            Profile::tombstone(2, shown.pow_nonce, shown.pow_bits)
        );
        let again = publish(&store, &mut node, &alice, &hidden).await.unwrap();
        assert_eq!((again.version, again.gender), (3, shown.gender));
        assert!(!store.profile_hidden().unwrap());
        assert_eq!(store.due_search_profile(1_001).unwrap(), None);
        let (mut refusing, _) = profile_node(vec![Status::Denied]).await;
        assert!(withdraw(&store, &mut refusing, &alice, &again)
            .await
            .is_err());
        assert!(!store.profile_hidden().unwrap());
        let removed = withdraw(&store, &mut node, &alice, &again).await.unwrap();
        assert_eq!(removed.version, 4);
        assert!(store.profile_hidden().unwrap());
        assert!(
            dyapp_profile::verify(&held.lock().unwrap().clone().unwrap())
                .unwrap()
                .profile
                .deleted
        );
        // Its own tombstone is no conflict: the kept draft comes back.
        let back = publish(&store, &mut node, &alice, &removed).await.unwrap();
        assert_eq!((back.version, back.gender), (5, shown.gender));
    }

    #[tokio::test]
    async fn a_newer_version_from_another_device_replaces_the_draft() {
        let alice = Identity::generate();
        let store = Store::open(&temp_db_path(), &alice).unwrap();
        let (mut node, held) = profile_node(Vec::new()).await;
        let other = Profile {
            version: 5,
            gender: dyapp_profile::Gender::Man.code(),
            ..Profile::default()
        };
        let record = other.sign(&alice);
        *held.lock().unwrap() = Some(record.clone());
        assert!(matches!(
            publish(&store, &mut node, &alice, &draft(None, None, None)).await,
            Err(Failure::Missed)
        ));
        assert_eq!(store.profile_draft().unwrap(), Some(other));
        assert_eq!(held.lock().unwrap().clone(), Some(record));
    }

    #[test]
    fn listed_devices_stay_sorted_and_unique() {
        let alice = Identity::generate();
        let mut draft = Profile {
            version: 1,
            ..Profile::default()
        };
        let certificates: Vec<_> = (0..3)
            .map(|_| {
                let device_key = Identity::generate();
                let device = dyapp_messaging::mls::Device::new(&alice, device_key.public_key(), 1);
                let certificate = device.unwrap().certificate().clone();
                let binding = crate::owner_binding(&device_key, &alice.public_key());
                (certificate, binding)
            })
            .collect();
        for (certificate, binding) in certificates.iter().chain(&certificates) {
            list_device(&mut draft, certificate.clone(), binding.clone());
        }
        assert_eq!((draft.devices.len(), draft.device_owners.len()), (3, 3));
        assert!(dyapp_profile::verify(&draft.sign(&alice)).is_ok());
        let mut keys = device_keys(&draft);
        keys.sort();
        let mut bound: Vec<_> = certificates
            .iter()
            .map(|(_, binding)| binding.public_key.clone())
            .collect();
        bound.sort();
        assert_eq!(keys, bound);

        let (certificate, binding) = &certificates[1];
        let kept = unlist_device(&mut draft, &binding.public_key).unwrap();
        assert_eq!(kept, *binding);
        assert!(!draft.devices.contains(certificate));
        assert_eq!((draft.devices.len(), draft.device_owners.len()), (2, 2));
        assert!(dyapp_profile::verify(&draft.sign(&alice)).is_ok());
        assert_eq!(unlist_device(&mut draft, &binding.public_key), None);
    }
}
