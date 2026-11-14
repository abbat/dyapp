use super::*;
use crate::sticker_requests::{self as requests, Action, Kind};

fn queued(session: &Session) -> Vec<Vec<u8>> {
    session
        .due_outbound(i64::MAX)
        .unwrap()
        .iter()
        .map(|write| write.as_bytes().to_vec())
        .collect()
}
fn published(pair: &Invite) {
    pair.session
        .store()
        .save_profile_draft(&dyapp_profile::Profile {
            version: 1,
            devices: vec![pair.session.certificate().unwrap()],
            ..Default::default()
        })
        .unwrap();
}
fn prepare(
    pair: &Invite,
    path: &std::path::Path,
    key: &MessageKey,
    kind: Kind,
    generation: Option<[u8; 16]>,
) -> Result<Option<requests::Stage>, requests::Error> {
    let mut store = Store::open(path, &pair.bob_id).unwrap();
    requests::prepare(
        &mut store,
        &SEAL,
        &pair.alice_id.peer_id(),
        key,
        Action { kind, generation },
    )
}
fn fixture() -> (Invite, std::path::PathBuf, MessageKey, Pack) {
    let (mut pair, path) = pair();
    let pack = sticker_packs::tests::pack("Cats");
    let key = selected(&mut pair, &pack);
    published(&pair);
    (pair, path, key, pack)
}
fn pending(pair: &Invite, pack: &Pack) -> PendingPack {
    pair.session
        .store()
        .pending_sticker_pack(&pair.alice_id.peer_id(), pack.id())
        .unwrap()
        .unwrap()
}

#[test]
fn shared_actions_queue_offline_fresh_generation_survives_restart_and_stop_keeps_outbox() {
    let (mut pair, path, key, pack) = fixture();
    prepare(&pair, &path, &key, Kind::Request, None)
        .unwrap()
        .unwrap()
        .stage(&mut pair.session, &SEAL)
        .unwrap();
    let first = pending(&pair, &pack);
    let sealed = pair.session.store().sealed_device().unwrap();
    let before = queued(&pair.session);
    assert!(prepare(&pair, &path, &key, Kind::Request, None)
        .unwrap()
        .is_none());
    assert_eq!(pair.session.store().sealed_device().unwrap(), sealed);
    assert_eq!(queued(&pair.session), before);
    prepare(&pair, &path, &key, Kind::Fresh, Some(first.token))
        .unwrap()
        .unwrap()
        .stage(&mut pair.session, &SEAL)
        .unwrap();
    let next = pending(&pair, &pack);
    assert_ne!(next.token, first.token);
    assert_eq!(next.sticker_hash, first.sticker_hash);
    assert!(matches!(
        prepare(&pair, &path, &key, Kind::StopWaiting, Some(first.token)),
        Err(requests::Error::RequestChanged)
    ));
    let writes = queued(&pair.session);
    assert!(writes.len() > before.len());
    drop(pair.session);
    pair.session = Session::open(Store::open(&path, &pair.bob_id).unwrap(), &SEAL).unwrap();
    assert!(pending(&pair, &pack) == next);
    assert_eq!(queued(&pair.session), writes);
    prepare(&pair, &path, &key, Kind::StopWaiting, Some(next.token)).unwrap();
    assert!(pair
        .session
        .store()
        .pending_sticker_pack(&key.author, pack.id())
        .unwrap()
        .is_none());
    assert_eq!(queued(&pair.session), writes);
}

#[test]
fn prepared_action_refuses_other_owner_without_advancing_its_session() {
    let (pair, path, key, _) = fixture();
    let stage = prepare(&pair, &path, &key, Kind::Request, None)
        .unwrap()
        .unwrap();
    let (mut other, _) = super::pair();
    let snapshot = other.session.store().sealed_device().unwrap();
    let writes = queued(&other.session);
    assert!(matches!(
        stage.stage(&mut other.session, &SEAL),
        Err(requests::Error::Media(
            crate::sticker_media::Error::IdentityChanged
        ))
    ));
    assert_eq!(other.session.store().sealed_device().unwrap(), snapshot);
    assert_eq!(queued(&other.session), writes);
}

#[test]
fn prepared_action_rechecks_device_listing_without_advancing_mls() {
    let (mut pair, path, key, _) = fixture();
    let stage = prepare(&pair, &path, &key, Kind::Request, None)
        .unwrap()
        .unwrap();
    let snapshot = pair.session.store().sealed_device().unwrap();
    let writes = queued(&pair.session);
    pair.session
        .store()
        .save_profile_draft(&dyapp_profile::Profile {
            version: 1,
            ..Default::default()
        })
        .unwrap();
    assert!(matches!(
        stage.stage(&mut pair.session, &SEAL),
        Err(requests::Error::Unpublished)
    ));
    assert_eq!(pair.session.store().sealed_device().unwrap(), snapshot);
    assert_eq!(queued(&pair.session), writes);
}

#[test]
fn prepared_action_refuses_block_and_clear_without_advancing_mls() {
    let (mut pair, path, key, _) = fixture();
    let snapshot = pair.session.store().sealed_device().unwrap();
    let writes = queued(&pair.session);
    let stage = prepare(&pair, &path, &key, Kind::Request, None)
        .unwrap()
        .unwrap();
    pair.session.store().set_blocked(&key.author, true).unwrap();
    assert!(matches!(
        stage.stage(&mut pair.session, &SEAL),
        Err(requests::Error::Media(crate::sticker_media::Error::Blocked))
    ));
    assert_eq!(queued(&pair.session), writes);
    pair.session
        .store()
        .set_blocked(&key.author, false)
        .unwrap();
    let stage = prepare(&pair, &path, &key, Kind::Request, None)
        .unwrap()
        .unwrap();
    pair.session.store.clear_chat_pair(&key.author).unwrap();
    let after_clear = queued(&pair.session);
    assert!(matches!(
        stage.stage(&mut pair.session, &SEAL),
        Err(requests::Error::Media(
            crate::sticker_media::Error::UnknownMessage
        ))
    ));
    assert_eq!(pair.session.store().sealed_device().unwrap(), snapshot);
    assert_eq!(queued(&pair.session), after_clear);
}
