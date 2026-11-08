use super::*;

#[test]
fn downloaded_bytes_require_pack_identity_and_clicked_hash() {
    for kind in 0..3 {
        let (mut pair, path, pack, mut job) = admitted();
        let bytes = match kind {
            0 => b"malformed".to_vec(),
            1 => sticker_packs::tests::pack("Different manifest").encode(),
            _ => {
                job.sticker_hash = [99; 32];
                rusqlite::Connection::open(path)
                    .unwrap()
                    .execute(
                        "UPDATE `sticker_pack_requests` SET `sticker_hash` = ?",
                        [job.sticker_hash.as_slice()],
                    )
                    .unwrap();
                pack.encode()
            }
        };
        assert!(!pair
            .session
            .complete_pack_download(pair.group.id(), &job, &bytes, &SEAL)
            .unwrap());
        assert_eq!(
            pair.session
                .store
                .sticker_pack_download_state(&job.peer, &job.pack_id)
                .unwrap()
                .unwrap()
                .failure,
            Some(Failure::Invalid)
        );
        assert!(pair
            .session
            .store
            .pending_sticker_pack(&job.peer, &job.pack_id)
            .unwrap()
            .is_some());
        assert!(pair
            .session
            .store
            .saved_sticker_pack(&SEAL, &job.pack_id)
            .unwrap()
            .is_none());
    }
}

#[test]
fn stale_unsolicited_and_blocked_fetch_completions_never_publish() {
    let (mut pair, _, pack, job) = admitted();
    let next = pair
        .session
        .store
        .retry_sticker_pack_download(&job)
        .unwrap()
        .unwrap();
    assert!(!pair
        .session
        .complete_pack_download(pair.group.id(), &job, &pack.encode(), &SEAL)
        .unwrap());
    let state = pair
        .session
        .store
        .sticker_pack_download_state(&job.peer, &job.pack_id)
        .unwrap()
        .unwrap();
    assert!(state.failure.is_none() && state.attempts == 0);
    assert_eq!(
        pair.session
            .store
            .pending_sticker_pack(&job.peer, &job.pack_id)
            .unwrap()
            .unwrap()
            .token,
        next.token
    );
    assert!(pair
        .session
        .store
        .saved_sticker_pack(&SEAL, &job.pack_id)
        .unwrap()
        .is_none());
    assert!(pair
        .session
        .complete_pack_download(pair.group.id(), &next, &pack.encode(), &SEAL)
        .unwrap());

    let (mut pair, _, pack, job) = admitted();
    pair.session
        .store
        .finish_sticker_pack(&job.peer, &job.pack_id, &job.token)
        .unwrap();
    assert!(!pair
        .session
        .complete_pack_download(pair.group.id(), &job, &pack.encode(), &SEAL)
        .unwrap());
    assert!(pair
        .session
        .store
        .saved_sticker_pack(&SEAL, &job.pack_id)
        .unwrap()
        .is_none());

    let (mut pair, _, pack, job) = admitted();
    pair.session.store.set_blocked(&job.peer, true).unwrap();
    assert!(!pair
        .session
        .complete_pack_download(pair.group.id(), &job, &pack.encode(), &SEAL)
        .unwrap());
    assert!(pair
        .session
        .store
        .saved_sticker_pack(&SEAL, &job.pack_id)
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn stop_after_transport_and_unreadable_local_file_preserve_retryable_intent() {
    let (mut pair, _, pack, job) = admitted();
    let checks = std::cell::Cell::new(0);
    assert_eq!(
        pair.session
            .download_sticker_packs(&mut [], &SEAL, || {
                let checked = checks.get();
                checks.set(checked + 1);
                checked > 0
            })
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
    assert!(state.failure.is_none() && state.attempts == 0);
    pair.session.store.save_sticker_pack(&SEAL, &pack).unwrap();
    assert_eq!(
        pair.session
            .download_sticker_packs(&mut [], &[99; 32], || false)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        pair.session
            .store
            .sticker_pack_download_state(&job.peer, &job.pack_id)
            .unwrap()
            .unwrap()
            .failure,
        Some(Failure::Storage)
    );
    assert!(pair
        .session
        .store
        .saved_sticker_pack(&SEAL, &job.pack_id)
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn five_due_downloads_leave_one_for_the_next_pass() {
    let (mut pair, _) = pair();
    for index in 20..25 {
        let pack = sticker_packs::tests::pack(&format!("Cats {index}"));
        let mut item = sticker(&pack);
        item.id = [index; 16];
        item.hlc = u64::from(index);
        deliver(&mut pair, &Body::Sticker(Box::new(item)), index);
        let key = MessageKey {
            group: pair.group.id().to_vec(),
            author: pair.alice_id.peer_id(),
            id: [index; 16],
        };
        pair.session
            .stage_sticker_pack_request(&key.author, &key, &SEAL)
            .unwrap();
        let (mut attachment, _) = crate::attachment::seal(&pack.encode()).unwrap();
        attachment.release_secret = [0; 32];
        deliver(
            &mut pair,
            &Body::PackAnswer(Box::new(PackAnswer {
                pack_id: *pack.id(),
                attachment,
            })),
            index + 50,
        );
        pair.session.store.save_sticker_pack(&SEAL, &pack).unwrap();
    }
    assert_eq!(
        pair.session
            .download_sticker_packs(&mut [], &SEAL, || false)
            .await
            .unwrap(),
        4
    );
    assert_eq!(
        pair.session
            .store
            .due_sticker_pack_downloads(0)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        pair.session
            .download_sticker_packs(&mut [], &SEAL, || false)
            .await
            .unwrap(),
        1
    );
    assert!(pair
        .session
        .store
        .due_sticker_pack_downloads(0)
        .unwrap()
        .is_empty());
}

#[test]
fn full_clear_cancels_old_fetch_but_preserves_saved_packs_recents_and_sharing() {
    let (mut pair, _, pack, job) = admitted();
    let kept = sticker_packs::tests::pack("Keep this pack");
    pair.session.store.save_sticker_pack(&SEAL, &kept).unwrap();
    pair.session
        .stage_sticker(pair.group.id(), &sticker(&kept), &SEAL)
        .unwrap();
    let recent = pair.session.store.recent_stickers().unwrap();
    pair.session.store.clear_chat_pair(&job.peer).unwrap();
    assert!(pair
        .session
        .store
        .pending_sticker_pack(&job.peer, &job.pack_id)
        .unwrap()
        .is_none());
    assert!(!pair
        .session
        .complete_pack_download(pair.group.id(), &job, &pack.encode(), &SEAL)
        .unwrap());
    assert!(pair
        .session
        .store
        .saved_sticker_pack(&SEAL, pack.id())
        .unwrap()
        .is_none());
    assert!(pair
        .session
        .store
        .saved_sticker_pack(&SEAL, kept.id())
        .unwrap()
        .is_some());
    assert!(pair
        .session
        .store
        .shared_sticker_pack(&job.peer, kept.id())
        .unwrap());
    assert!(pair.session.store.recent_stickers().unwrap() == recent);
    assert!(pair
        .session
        .stage_fresh_sticker_pack_request(&job, &SEAL)
        .is_err());
    assert!(pair.session.device.is_some());

    let mut incoming = sticker(&pack);
    incoming.id = [7; 16];
    incoming.hlc = 2000;
    deliver(&mut pair, &Body::Sticker(Box::new(incoming)), 7);
    let selected = MessageKey {
        group: pair.group.id().to_vec(),
        author: job.peer.clone(),
        id: [7; 16],
    };
    let (next, _) = pair
        .session
        .stage_sticker_pack_request(&job.peer, &selected, &SEAL)
        .unwrap();
    assert_ne!(next.token, job.token);
    assert!(!pair
        .session
        .complete_pack_download(pair.group.id(), &job, &pack.encode(), &SEAL)
        .unwrap());
    assert_eq!(
        pair.session
            .store
            .pending_sticker_pack(&job.peer, &job.pack_id)
            .unwrap()
            .unwrap()
            .token,
        next.token
    );
    assert!(pair
        .session
        .store
        .saved_sticker_pack(&SEAL, pack.id())
        .unwrap()
        .is_none());
}
