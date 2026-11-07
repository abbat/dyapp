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
    assert!(
        pair.session
            .store
            .pending_sticker_pack(&job.peer, &job.pack_id)
            .unwrap()
            .unwrap()
            .token
            == next.token
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
