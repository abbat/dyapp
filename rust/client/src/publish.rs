//! Publishing the owner's profile draft: the checks before each publish, the cancellable proof
//! of work, and the publish with its signed readback.
//!
//! A caller runs [`check`], calls [`work`] in short steps (saving the draft after each, so a
//! cancelled publish resumes where it stopped), then [`publish`].

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
pub fn check(draft: &Profile, attested: bool, today: CivilDate) -> Result<(), Block> {
    if !attested {
        return Err(Block::NotAttested);
    }
    if !draft.valid_birth_parts() {
        return Err(Block::InvalidBirthDate);
    }
    if draft.birth_year.is_some()
        && !draft
            .possible_age_range(today)
            .is_some_and(|(youngest, _)| youngest >= 18)
    {
        return Err(Block::Underage);
    }
    Ok(())
}

/// List `certificate` among `draft`'s devices once, in the mailbox order a profile requires.
pub fn list_device(draft: &mut Profile, certificate: SignedRecord) {
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

/// Try up to `tries` more nonces for a profile proof of work of `bits` for `key`, the owner's
/// public key. Progress stays in `draft.pow_nonce`; `draft.pow_bits` is set only once solved,
/// and the proof does not depend on other fields, so later versions reuse it. Returns whether
/// it is solved.
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
            false
        }
    }
}

/// Publish `draft` (version: the last one published) to `node` at the next version and read it
/// back. Succeeds only when the signed readback equals the published profile, which is then
/// saved as the draft and held for search ([`Store::hold_search_profile`]); on failure the draft
/// is unchanged and the publish may be retried. A hidden profile is shown again.
pub async fn publish(
    store: &Store,
    node: &mut Node,
    identity: &Identity,
    draft: &Profile,
) -> Result<Profile, Failure> {
    let next = Profile {
        version: draft.version + 1,
        ..draft.clone()
    };
    let profile = send(node, identity, next).await?;
    store.save_profile_draft(&profile)?;
    store.hold_search_profile(&profile.sign(identity))?;
    store.set_profile_hidden(false)?;
    Ok(profile)
}

/// Hide from search with a signed tombstone held for search nodes, leaving the last ordinary
/// profile and device directory at store nodes. The caller sends the held record and reports a
/// pending withdrawal until a search node accepts it. The saved draft advances to that version.
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
pub async fn withdraw(
    store: &Store,
    node: &mut Node,
    identity: &Identity,
    draft: &Profile,
) -> Result<Profile, Failure> {
    let next = Profile::tombstone(draft.version + 1, draft.pow_nonce, draft.pow_bits);
    let tombstone = send(node, identity, next).await?;
    let draft = Profile {
        version: tombstone.version,
        ..draft.clone()
    };
    store.save_profile_draft(&draft)?;
    store.hold_search_profile(&tombstone.sign(identity))?;
    store.set_profile_hidden(true)?;
    Ok(draft)
}

// Publish `profile` to `node` and read it back; a newer version held there moves it past that.
async fn send(
    node: &mut Node,
    identity: &Identity,
    mut profile: Profile,
) -> Result<Profile, Error> {
    let peer_id = identity.peer_id();
    match node.publish_profile(profile.sign(identity)).await {
        Ok(()) => {}
        // A lost reply to this same publish, or a newer version from another device.
        Err(Error::Status(Status::Stale)) => {
            let held = node.get_profile(&peer_id).await?.profile;
            if held != profile {
                profile.version = held.version + 1;
                node.publish_profile(profile.sign(identity)).await?;
            }
        }
        Err(error) => return Err(error),
    }
    if node.get_profile(&peer_id).await?.profile != profile {
        return Err(Error::Reply("readback differs from the published profile"));
    }
    Ok(profile)
}

/// Why [`publish`] failed; both are retryable.
#[derive(Debug, thiserror::Error)]
pub enum Failure {
    /// The node refused, did not answer, or read back something else.
    #[error(transparent)]
    Node(#[from] Error),
    /// The published profile could not be saved locally.
    #[error(transparent)]
    Store(#[from] store::Error),
}

#[cfg(test)]
mod tests {
    use super::{check, hide, list_device, publish, withdraw, work, Block, Failure};
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
        let key = Identity::generate().public_key();
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
        let mut server =
            build_configured_swarm(Keypair::generate_ed25519(), SwarmOptions::default()).unwrap();
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
    }

    #[tokio::test]
    async fn a_newer_version_from_another_device_is_superseded() {
        let alice = Identity::generate();
        let store = Store::open(&temp_db_path(), &alice).unwrap();
        let (mut node, held) = profile_node(Vec::new()).await;
        let other = Profile {
            version: 5,
            gender: dyapp_profile::Gender::Man.code(),
            ..Profile::default()
        };
        *held.lock().unwrap() = Some(other.sign(&alice));
        let published = publish(&store, &mut node, &alice, &draft(None, None, None))
            .await
            .unwrap();
        assert_eq!(
            (published.version, published.gender),
            (6, dyapp_profile::Gender::Woman.code())
        );
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
                dyapp_messaging::mls::Device::new(&alice, Identity::generate().public_key(), 1)
                    .unwrap()
                    .certificate()
                    .clone()
            })
            .collect();
        for certificate in certificates.iter().chain(&certificates) {
            list_device(&mut draft, certificate.clone());
        }
        assert_eq!(draft.devices.len(), 3);
        assert!(dyapp_profile::verify(&draft.sign(&alice)).is_ok());
    }
}
