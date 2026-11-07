use super::*;

#[test]
fn fresh_request_rotates_generation_with_exact_outbox_and_preserves_clicked_hash() {
    let (mut store, owner, path, peer) = database();
    let (_, answer) = request(&mut store, &peer, 1);
    admit(&mut store, &peer, 1, &answer);
    let job = store.due_sticker_pack_downloads(0).unwrap().remove(0);
    store
        .defer_sticker_pack_download(&job, PackDownloadFailure::Invalid, 100)
        .unwrap();
    let snapshot = store.sealed_device().unwrap();
    let status = store
        .sticker_pack_download_state(&peer, &job.pack_id)
        .unwrap();
    let write = crate::prepare_put(&[1; 32], vec![9]).unwrap();
    assert!(store
        .stage_fresh_sticker_pack_request(b"", std::slice::from_ref(&write), &[9], &job)
        .is_err());
    assert!(store
        .stage_fresh_sticker_pack_request(b"fresh", &[], &[9], &job)
        .is_err());
    assert!(store.sealed_device().unwrap() == snapshot);
    assert_eq!(
        store
            .sticker_pack_download_state(&peer, &job.pack_id)
            .unwrap(),
        status
    );
    assert!(
        store
            .sticker_pack_answer(&[9], &peer, &job.pack_id, &job.token)
            .unwrap()
            == Some(answer)
    );
    assert!(store.due_outbound(i64::MAX).unwrap().is_empty());

    let next = store
        .stage_fresh_sticker_pack_request(b"fresh", std::slice::from_ref(&write), &[9], &job)
        .unwrap();
    assert!(next.token != job.token && next.sticker_hash == job.sticker_hash);
    assert!(!store
        .finish_sticker_pack(&peer, &job.pack_id, &job.token)
        .unwrap());
    assert!(!store
        .defer_sticker_pack_download(&job, PackDownloadFailure::Fetch, 200)
        .unwrap());
    assert!(store
        .stage_fresh_sticker_pack_request(b"stale", std::slice::from_ref(&write), &[9], &job)
        .is_err());
    drop(store);
    let store = Store::open(&path, &owner).unwrap();
    assert!(store.pending_sticker_pack(&peer, &job.pack_id).unwrap() == Some(next.clone()));
    assert!(store
        .sticker_pack_answer(&[9], &peer, &job.pack_id, &next.token)
        .unwrap()
        .is_none());
    let state = store
        .sticker_pack_download_state(&peer, &job.pack_id)
        .unwrap()
        .unwrap();
    assert!(
        !state.has_answer
            && state.failure.is_none()
            && state.attempts == 0
            && state.retry_at_ms == 0
    );
    assert_eq!(store.sealed_device().unwrap().unwrap(), b"fresh");
    let due = store.due_outbound(i64::MAX).unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].as_bytes(), write.as_bytes());
}

#[test]
fn outbox_failure_rolls_back_answer_reset_and_policy_refusal_keeps_generation() {
    let (mut store, _, _, peer) = database();
    let (_, answer) = request(&mut store, &peer, 1);
    admit(&mut store, &peer, 1, &answer);
    let job = store.due_sticker_pack_downloads(0).unwrap().remove(0);
    let snapshot = store.sealed_device().unwrap();
    let write = crate::prepare_put(&[1; 32], vec![9]).unwrap();
    store
        .db
        .execute_batch(
            "CREATE TRIGGER `reject_fresh_outbox` BEFORE INSERT ON `outbox`
         BEGIN SELECT RAISE(ABORT, 'fixture fresh request failure'); END;",
        )
        .unwrap();
    assert!(store
        .stage_fresh_sticker_pack_request(b"fresh", std::slice::from_ref(&write), &[9], &job)
        .is_err());
    assert!(
        store
            .sticker_pack_answer(&[9], &peer, &job.pack_id, &job.token)
            .unwrap()
            == Some(answer)
    );
    assert!(store.sealed_device().unwrap() == snapshot);
    assert!(store.due_outbound(i64::MAX).unwrap().is_empty());
    store.set_blocked(&peer, true).unwrap();
    assert!(matches!(
        store.stage_fresh_sticker_pack_request(
            b"blocked",
            std::slice::from_ref(&write),
            &[9],
            &job
        ),
        Err(Error::BlockedPeer)
    ));
    assert!(
        store
            .pending_sticker_pack(&peer, &job.pack_id)
            .unwrap()
            .unwrap()
            .token
            == job.token
    );
}
