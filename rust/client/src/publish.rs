//! Publishing the owner's profile draft: the checks before each publish, the cancellable proof
//! of work, and the publish with its signed readback.
//!
//! A caller runs [`check`], calls [`work`] in short steps (saving the draft after each, so a
//! cancelled publish resumes where it stopped), then [`publish`].

use crate::store::profile_photos::{fits, MAX_PHOTO_BYTES};
use crate::store::{self, Store};
use crate::{Error, Node};
use dyapp_identity::{key_hash, Identity, SignedRecord};
use dyapp_p2p_net::proto::Status;
use dyapp_profile::{CivilDate, DeviceCertificate, Profile};
use prost::Message as _;

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
/// Succeeds only when the signed readback equals the published profile, which is then
/// saved as the draft and held for search ([`Store::hold_search_profile`]). A failure may leave
/// remote publication or local updates applied; the publish may be retried. A hidden profile is
/// shown again. Photos reach the node first, so the profile never lists a blob the node has
/// refused; blobs the published profile no longer lists then leave the store.
///
/// # Errors
///
/// Returns an error if storage or node requests fail, a photo to upload is absent,
/// the node holds an older profile than this device has seen, or signed readback
/// differs. Returns [`Failure::Missed`] after saving a newer foreign profile.
/// Remote publication or some local updates may already have succeeded.
pub async fn publish(
    store: &Store,
    node: &mut Node,
    identity: &Identity,
    draft: &Profile,
) -> Result<Profile, Failure> {
    send_draft(store, node, identity, draft, false).await
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
    send_draft(store, node, identity, draft, store.profile_hidden()?).await
}

/// Revokes the owner's device with the mailbox key `device_key`: closes its mailbox, then [`relist`]s
/// `draft` without it. A newer profile at the node first replaces the saved draft, as in [`publish`],
/// so a device listed elsewhere can go and no other device is lost. A failure may be retried, as a
/// repeated close is harmless and the draft keeps the binding until the relist succeeds. Returns the
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
    let mut draft = match newest(node, identity).await? {
        Some(held) if held.version > draft.version => {
            take_photos(store, node, &held).await?;
            store.save_profile_draft(&held)?;
            held
        }
        _ => draft.clone(),
    };
    let binding =
        unlist_device(&mut draft, device_key).ok_or(Error::Input("device has no owner binding"))?;
    node.close_mailbox(identity, binding).await?;
    relist(store, node, identity, &draft).await
}

/// Starts the fresh store of a linked device from the record's `state` and the newest profile with
/// its photos, kept as the draft. A failure may be retried. The caller then lists the device and
/// calls [`relist`]; the draft of a hidden profile goes past it, as [`hide`] leaves it.
///
/// # Errors
///
/// Returns an error if importing link state, reading or saving local state,
/// fetching the profile or its photos fails, or the account profile is missing
/// or deleted. Earlier imports and local writes remain applied on later failure.
pub async fn adopt(
    store: &mut Store,
    node: &mut Node,
    identity: &Identity,
    state: &[u8],
    now: u64,
) -> Result<Profile, Failure> {
    store.import_link_state(state)?;
    let held = newest(node, identity)
        .await?
        .ok_or(Error::Reply("the account has no profile"))?;
    if held.deleted {
        return Err(Error::Reply("the account was removed").into());
    }
    take_photos(store, node, &held).await?;
    store.own_profile_age(held.version, now)?;
    let draft = Profile {
        version: held.version + u64::from(store.profile_hidden()?),
        ..held
    };
    store.save_profile_draft(&draft)?;
    Ok(draft)
}

async fn send_draft(
    store: &Store,
    node: &mut Node,
    identity: &Identity,
    draft: &Profile,
    stay_hidden: bool,
) -> Result<Profile, Failure> {
    let next = Profile {
        version: draft.version + 1,
        ..draft.clone()
    };
    let held = newest(node, identity).await?;
    let seen = held.as_ref().map_or(0, |held| held.version);
    if store.own_profile_seen()? > seen.max(draft.version) {
        return Err(Error::Reply("the node holds an older own profile").into());
    }
    if let Some(held) = held {
        // Hiding moves the draft past the live profile, so an equal version came from elsewhere,
        // unless it is the tombstone `withdraw` left here.
        let ours = Profile::tombstone(draft.version, draft.pow_nonce, draft.pow_bits);
        let foreign = held.version == draft.version && held != ours && store.profile_hidden()?;
        if held != next && (held.version > draft.version || foreign) {
            take_photos(store, node, &held).await?;
            store.save_profile_draft(&held)?;
            return Err(Failure::Missed);
        }
    }
    let hashes = photo_hashes(&next);
    if !hashes.is_empty() || store.has_profile_photos()? {
        upload_photos(store, node, identity, next.version, &hashes).await?;
    }
    let profile = send(node, identity, next).await?;
    store.save_profile_draft(&profile)?;
    store.retain_profile_photos(&hashes)?;
    if stay_hidden {
        return hide(store, identity, &profile);
    }
    store.hold_search_profile(&profile.sign(identity))?;
    store.set_profile_hidden(false)?;
    Ok(profile)
}

// Fetch the blobs of `profile` this store lacks, for a publish that may have to put them back. One
// a node lost stays missing: nothing here can restore it.
async fn take_photos(store: &Store, node: &mut Node, profile: &Profile) -> Result<(), Failure> {
    for hash in photo_hashes(profile) {
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

/// Refused keeps before a photo upload gives up. Each failed publish can leave one more version.
const KEEP_REFUSALS: u32 = 8;
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
    if !store.profile_keep_due(now)? || !store.has_profile_photos()? {
        return Ok(());
    }
    store.profile_keep_refreshed(now)?;
    let live = node.get_profile(&identity.peer_id()).await?.profile;
    let hashes = photo_hashes(&live);
    if !hashes.is_empty() {
        upload_photos(store, node, identity, live.version, &hashes).await?;
    }
    Ok(())
}

// Replace the node's list with `hashes` and put what it lacks. The node refuses another list at an
// equal version, and a failed publish may leave one there, so a refusal moves past both that and
// any newer profile, one version at a time because the refusal does not name the node's version.
async fn upload_photos(
    store: &Store,
    node: &mut Node,
    identity: &Identity,
    version: u64,
    hashes: &[Vec<u8>],
) -> Result<(), Failure> {
    let (mut version, mut refused) = (version, 0);
    let missing = loop {
        match node.keep_media(identity, version, hashes.to_vec()).await {
            Err(Error::Status(Status::Stale)) if refused < KEEP_REFUSALS => {
                refused += 1;
                let held = node.get_profile(&identity.peer_id()).await;
                version = held.map_or(0, |held| held.profile.version).max(version) + 1;
            }
            missing => break missing?,
        }
    };
    for hash in missing {
        let blob = store
            .profile_photo(&hash)?
            .ok_or(Error::Reply("kept photo is not in the store"))?;
        patiently(async || node.put_media(&identity.peer_id(), blob.clone()).await).await?;
    }
    Ok(())
}

/// Hide from search with a signed tombstone held for search nodes, leaving the last ordinary
/// profile and device directory at store nodes. The caller sends the held record and reports a
/// pending withdrawal until a search node accepts it. The saved draft advances to that version.
///
/// # Errors
///
/// Returns an error if saving the advanced draft, holding the search tombstone,
/// or recording hidden state fails. Earlier local writes remain applied.
pub fn hide(store: &Store, identity: &Identity, draft: &Profile) -> Result<Profile, Failure> {
    let tombstone = Profile::tombstone(draft.version + 1, draft.pow_nonce, draft.pow_bits);
    let draft = Profile {
        version: tombstone.version,
        ..draft.clone()
    };
    store.save_profile_draft(&draft)?;
    store.hold_search_profile(&tombstone.sign(identity))?;
    store.set_profile_hidden(true)?;
    Ok(draft)
}

/// Withdraw an identity before local removal: publish a final tombstone to profile storage,
/// read it back, then hold it for search. No local key is removed here; the caller decides
/// whether to retry a failed node request or remove anyway.
///
/// # Errors
///
/// Returns an error if storage or node requests fail or the signed tombstone
/// readback differs. The tombstone may already be published or local state
/// partially updated. Failure to release the photo list is ignored.
pub async fn withdraw(
    store: &Store,
    node: &mut Node,
    identity: &Identity,
    draft: &Profile,
) -> Result<Profile, Failure> {
    let held = newest(node, identity).await?.map_or(0, |held| held.version);
    let version = draft.version.max(held).max(store.own_profile_seen()?) + 1;
    let next = Profile::tombstone(version, draft.pow_nonce, draft.pow_bits);
    let tombstone = send(node, identity, next).await?;
    if store.has_profile_photos()? {
        // Best effort: an unrefreshed list expires with the owner's inactivity anyway.
        let _ = upload_photos(store, node, identity, tombstone.version, &[]).await;
    }
    let draft = Profile {
        version: tombstone.version,
        ..draft.clone()
    };
    store.save_profile_draft(&draft)?;
    store.hold_search_profile(&tombstone.sign(identity))?;
    store.set_profile_hidden(true)?;
    Ok(draft)
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
    /// before publishing again; unpublished edits on this device give way to it.
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
        let mut server = Box::new(
            build_configured_swarm(Keypair::generate_ed25519(), SwarmOptions::default()).unwrap(),
        );
        server
            .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
            .unwrap();
        let address = loop {
            if let SwarmEvent::NewListenAddr { address, .. } = server.select_next_some().await {
                break address;
            }
        };
        let held = Arc::new(Mutex::new(None));
        let copy = Arc::clone(&held);
        let mut refuse = refuse.into_iter();
        tokio::spawn(async move {
            loop {
                let SwarmEvent::Behaviour(BehaviourEvent::Profile(RequestEvent::Message {
                    message:
                        RequestMessage::Request {
                            request, channel, ..
                        },
                    ..
                })) = server.select_next_some().await
                else {
                    continue;
                };
                let response = answer(
                    &mut copy.lock().unwrap(),
                    &mut refuse,
                    request.request.unwrap(),
                );
                server
                    .behaviour_mut()
                    .profile
                    .send_response(channel, response)
                    .unwrap();
            }
        });
        let node = Node::connect(address, Arc::new(Protocols::public()))
            .await
            .unwrap();
        (node, held)
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
