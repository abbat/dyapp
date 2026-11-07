use super::download::admitted;
use super::*;

#[test]
fn fresh_request_survives_cleared_selection_and_rejects_old_generation_before_mls() {
    let (mut pair, path, pack, job) = admitted();
    pair.session.store.clear_chat_pair(&job.peer).unwrap();
    let (next, writes) = pair
        .session
        .stage_fresh_sticker_pack_request(&job, &SEAL)
        .unwrap();
    assert!(next.token != job.token && next.sticker_hash == job.sticker_hash && !writes.is_empty());
    let snapshot = pair.session.store.sealed_device().unwrap();
    assert!(pair
        .session
        .stage_fresh_sticker_pack_request(&job, &SEAL)
        .is_err());
    assert!(pair.session.device.is_some());
    assert!(pair.session.store.sealed_device().unwrap() == snapshot);
    drop(pair.session);
    let restored = Session::open(Store::open(&path, &pair.bob_id).unwrap(), &SEAL).unwrap();
    assert!(
        restored
            .store
            .pending_sticker_pack(&job.peer, pack.id())
            .unwrap()
            == Some(next)
    );
    let due = restored.due_outbound(i64::MAX).unwrap();
    assert!(writes
        .iter()
        .all(|write| due.iter().any(|held| held.as_bytes() == write.as_bytes())));
}

#[test]
fn failed_fresh_request_commit_invalidates_only_advanced_memory_and_keeps_old_answer() {
    let (mut pair, path, _, job) = admitted();
    let answer = pair
        .session
        .store
        .sticker_pack_answer(pair.group.id(), &job.peer, &job.pack_id, &job.token)
        .unwrap();
    let snapshot = pair.session.store.sealed_device().unwrap();
    let count = pair.session.due_outbound(i64::MAX).unwrap().len();
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER `reject_fresh_request` BEFORE UPDATE ON `sticker_pack_requests`
         BEGIN SELECT RAISE(ABORT, 'fixture fresh request failure'); END;",
        )
        .unwrap();
    assert!(pair
        .session
        .stage_fresh_sticker_pack_request(&job, &SEAL)
        .is_err());
    assert!(pair.session.device.is_none() && pair.session.groups.is_empty());
    assert!(pair.session.store.sealed_device().unwrap() == snapshot);
    assert!(
        pair.session
            .store
            .sticker_pack_answer(pair.group.id(), &job.peer, &job.pack_id, &job.token)
            .unwrap()
            == answer
    );
    assert_eq!(pair.session.due_outbound(i64::MAX).unwrap().len(), count);
    drop(pair.session);
    let mut restored = Session::open(Store::open(&path, &pair.bob_id).unwrap(), &SEAL).unwrap();
    restored
        .stage_text(pair.group.id(), b"still usable", &SEAL)
        .unwrap();
}
