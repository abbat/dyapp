use super::*;
use crate::store::sticker_packs::exchange::download::{
    PackDownloadFailure as Failure, PackDownloadJob,
};

fn admitted() -> (Invite, std::path::PathBuf, Pack, PackDownloadJob) {
    let (mut pair, path) = pair();
    let pack = sticker_packs::tests::pack("Cats");
    let key = selected(&mut pair, &pack);
    let peer = pair.alice_id.peer_id();
    pair.session
        .stage_sticker_pack_request(&peer, &key, &SEAL)
        .unwrap();
    let (mut attachment, _) = crate::attachment::seal(&pack.encode()).unwrap();
    attachment.release_secret = [0; 32];
    deliver(
        &mut pair,
        &Body::PackAnswer(Box::new(PackAnswer {
            pack_id: *pack.id(),
            attachment,
        })),
        4,
    );
    let job = pair
        .session
        .store
        .due_sticker_pack_downloads(0)
        .unwrap()
        .remove(0);
    (pair, path, pack, job)
}

#[tokio::test]
async fn offline_failure_survives_restart_and_explicit_retry_finishes_from_saved_file() {
    let (mut pair, path, pack, job) = admitted();
    let snapshot = pair.session.store.sealed_device().unwrap();
    let count = pair.session.due_outbound(i64::MAX).unwrap().len();
    assert_eq!(
        pair.session
            .download_sticker_packs(&mut [], &SEAL, || false)
            .await
            .unwrap(),
        0
    );
    let state = pair
        .session
        .store
        .sticker_pack_download_state(&job.peer, &job.pack_id)
        .unwrap()
        .unwrap();
    assert_eq!(state.failure, Some(Failure::Fetch));
    assert_eq!(state.attempts, 1);
    assert!(pair
        .session
        .store
        .due_sticker_pack_downloads(state.retry_at_ms - 1)
        .unwrap()
        .is_empty());
    assert_eq!(pair.session.due_outbound(i64::MAX).unwrap().len(), count);
    assert!(pair.session.store.sealed_device().unwrap() == snapshot);
    drop(pair.session);
    let mut session = Session::open(Store::open(&path, &pair.bob_id).unwrap(), &SEAL).unwrap();
    assert_eq!(
        session
            .store
            .sticker_pack_download_state(&job.peer, &job.pack_id)
            .unwrap(),
        Some(state)
    );
    session
        .store
        .retry_sticker_pack_download(&job)
        .unwrap()
        .unwrap();
    session.store.save_sticker_pack(&SEAL, &pack).unwrap();
    assert_eq!(
        session
            .download_sticker_packs(&mut [], &SEAL, || false)
            .await
            .unwrap(),
        1
    );
    assert!(session
        .store
        .pending_sticker_pack(&job.peer, &job.pack_id)
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn stop_and_block_prevent_fetch_or_save_without_chat_or_mls_mutation() {
    let (mut pair, _, pack, job) = admitted();
    let snapshot = pair.session.store.sealed_device().unwrap();
    assert_eq!(
        pair.session
            .download_sticker_packs(&mut [], &SEAL, || true)
            .await
            .unwrap(),
        0
    );
    assert!(pair
        .session
        .store
        .sticker_pack_download_state(&job.peer, &job.pack_id)
        .unwrap()
        .unwrap()
        .failure
        .is_none());
    pair.session.store.set_blocked(&job.peer, true).unwrap();
    assert_eq!(
        pair.session
            .download_sticker_packs(&mut [], &SEAL, || false)
            .await
            .unwrap(),
        0
    );
    assert!(pair
        .session
        .store
        .pending_sticker_pack(&job.peer, pack.id())
        .unwrap()
        .is_none());
    assert!(pair
        .session
        .store
        .saved_sticker_pack(&SEAL, pack.id())
        .unwrap()
        .is_none());
    assert!(pair.session.store.sealed_device().unwrap() == snapshot);
}
