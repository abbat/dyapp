use super::*;

#[test]
fn full_clear_releases_pending_quota_and_late_answers_cannot_restart_intent() {
    let (mut store, owner, path, peer) = database();
    let (pending, answer) = request(&mut store, &peer, 1);
    for id in 2..=63 {
        request(&mut store, &peer, id);
    }
    let other = dyapp_identity::Identity::generate().peer_id();
    // Independent peer metadata makes the global pending cap full.
    store.db.execute(
        "INSERT INTO `sticker_pack_requests` (`peer_id`, `pack_id`, `sticker_hash`, `request_token`) VALUES (?1, ?2, ?3, ?4)",
        params![&other, [90u8; 32].as_slice(), [7u8; 32].as_slice(), [9u8; 16].as_slice()]).unwrap();
    let over = selected(&mut store, &peer, 65);
    let next = store.sticker_pack_request(&[9], &over).unwrap();
    assert!(matches!(
        store.stage_sticker_pack_request(b"over cap", &[], &[9], &over, &next),
        Err(Error::PackRequestsFull)
    ));
    let job = PackDownloadJob {
        peer: peer.clone(),
        pack_id: answer.pack_id,
        sticker_hash: pending.sticker_hash,
        token: pending.token,
    };
    store.clear_chat_pair(&peer).unwrap();
    assert!(store
        .pending_sticker_pack(&peer, &answer.pack_id)
        .unwrap()
        .is_none());
    assert!(store
        .pending_sticker_pack(&other, &[90; 32])
        .unwrap()
        .is_some());
    assert!(!admit(&mut store, &peer, 1, &answer));
    assert!(store.due_sticker_pack_downloads(0).unwrap().is_empty());
    assert!(!store
        .defer_sticker_pack_download(&job, PackDownloadFailure::Fetch, 100)
        .unwrap());
    let write = crate::prepare_put(&[1; 32], vec![9]).unwrap();
    assert!(store
        .stage_fresh_sticker_pack_request(b"stale", &[write], &[9], &job)
        .is_err());
    assert!(store.due_outbound(i64::MAX).unwrap().is_empty());
    let key = selected(&mut store, &peer, 200);
    let request = store.sticker_pack_request(&[9], &key).unwrap();
    store
        .stage_sticker_pack_request(b"new", &[], &[9], &key, &request)
        .unwrap();
    drop(store);
    let store = Store::open(&path, &owner).unwrap();
    assert!(store
        .pending_sticker_pack(&peer, &answer.pack_id)
        .unwrap()
        .is_none());
    assert!(store
        .pending_sticker_pack(&peer, &[200; 32])
        .unwrap()
        .is_some());
    assert!(store
        .pending_sticker_pack(&other, &[90; 32])
        .unwrap()
        .is_some());
}

#[test]
fn cancellation_failure_rolls_back_history_outbox_and_pending_answer() {
    let (mut store, _, _, peer) = database();
    let (_, answer) = request(&mut store, &peer, 1);
    admit(&mut store, &peer, 1, &answer);
    let job = store.due_sticker_pack_downloads(0).unwrap().remove(0);
    let key = selected(&mut store, &peer, 1);
    let request = store.sticker_pack_request(&[9], &key).unwrap();
    let write = crate::prepare_put(&[1; 32], vec![9]).unwrap();
    store
        .stage_sticker_pack_request(b"before", &[write], &[9], &key, &request)
        .unwrap();
    store
        .db
        .execute_batch(
            "CREATE TRIGGER `reject_pack_cancellation` BEFORE DELETE ON `sticker_pack_requests`
         BEGIN SELECT RAISE(ABORT, 'fixture clear failure'); END;",
        )
        .unwrap();
    assert!(store.clear_chat_pair(&peer).is_err());
    assert_eq!(store.current_chat_stickers(&peer).unwrap().len(), 1);
    assert_eq!(store.due_outbound(i64::MAX).unwrap().len(), 1);
    assert_eq!(store.sealed_device().unwrap().unwrap(), b"before");
    assert!(
        store
            .sticker_pack_answer(&[9], &peer, &job.pack_id, &job.token)
            .unwrap()
            == Some(answer)
    );
}
