use super::*;

fn corrupt_key(
    path: &std::path::Path,
    kind: i64,
    descriptor: &crate::attachment::Attachment,
) -> Vec<u8> {
    let mut invalid = descriptor.clone();
    invalid.key[0] ^= 1;
    let raw = Body::Attachment(crate::chat::AttachmentMessage {
        id: [0; 16],
        hlc: 0,
        attachment: invalid,
    })
    .encode();
    let db = rusqlite::Connection::open(path).unwrap();
    assert_eq!(
        db.execute(
            "UPDATE `sticker_uploads` SET `descriptor` = ?3 WHERE `kind` = ?1 AND `hash` = ?2",
            rusqlite::params![kind, descriptor.plaintext_sha256.as_slice(), &raw]
        )
        .unwrap(),
        1
    );
    raw
}

#[tokio::test]
async fn wrong_owner_stop_block_and_removed_pack_never_send_a_response() {
    let (mut pair, _, _, job) = pending();
    let count = pair.session.due_outbound(i64::MAX).unwrap().len();
    let other = dyapp_identity::Identity::generate();
    assert!(pair
        .session
        .respond_sticker_pack_requests(&mut [], &other, &SEAL, || false)
        .await
        .is_err());
    assert_eq!(
        pair.session
            .respond_sticker_pack_requests(&mut [], &pair.bob_id, &SEAL, || true)
            .await
            .unwrap(),
        0
    );
    assert!(
        pair.session
            .store
            .due_sticker_pack_responses(crate::now() * 1000)
            .unwrap()
            == vec![job.clone()]
    );
    pair.session.store.set_blocked(&job.peer, true).unwrap();
    assert_eq!(
        pair.session
            .respond_sticker_pack_requests(&mut [], &pair.bob_id, &SEAL, || false)
            .await
            .unwrap(),
        0
    );
    assert!(pair
        .session
        .store
        .due_sticker_pack_responses(crate::now() * 1000)
        .unwrap()
        .is_empty());
    assert_eq!(pair.session.due_outbound(i64::MAX).unwrap().len(), count);
    let (mut pair, _, _, job) = pending();
    let count = pair.session.due_outbound(i64::MAX).unwrap().len();
    pair.session
        .store
        .remove_saved_sticker_pack(&job.pack_id)
        .unwrap();
    assert_eq!(
        pair.session
            .respond_sticker_pack_requests(&mut [], &pair.bob_id, &SEAL, || false)
            .await
            .unwrap(),
        0
    );
    assert!(pair
        .session
        .store
        .due_sticker_pack_responses(crate::now() * 1000)
        .unwrap()
        .is_empty());
    assert_eq!(pair.session.due_outbound(i64::MAX).unwrap().len(), count);
}

#[tokio::test]
async fn corrupt_ready_pack_key_is_preserved_without_reply_or_cooldown() {
    let (mut pair, path, pack, job) = pending();
    let descriptor = confirmed(&mut pair, &pack);
    let raw = corrupt_key(&path, 2, &descriptor);
    let count = pair.session.due_outbound(i64::MAX).unwrap().len();
    assert!(pair
        .session
        .respond_sticker_pack_requests(&mut [], &pair.bob_id, &SEAL, || false)
        .await
        .is_err());
    assert_eq!(pair.session.due_outbound(i64::MAX).unwrap().len(), count);
    assert!(
        pair.session
            .store
            .due_sticker_pack_responses(crate::now() * 1000)
            .unwrap()
            == vec![job]
    );
    let db = rusqlite::Connection::open(path).unwrap();
    let stored: Vec<u8> = db
        .query_row(
            "SELECT `descriptor` FROM `sticker_uploads` WHERE `kind` = 2",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stored, raw);
}

#[tokio::test]
async fn corrupt_ready_single_key_cannot_stage_a_bad_sticker_before_delivery_fails() {
    let (mut pair, path, pack, _) = pending();
    let (mut descriptor, _) = crate::attachment::seal(pack.sticker_bytes(0).unwrap()).unwrap();
    descriptor.mime = "image/webp".into();
    descriptor.width = 1;
    descriptor.height = 1;
    let lease = match pair
        .session
        .store
        .plan_sticker_upload(&descriptor, crate::now())
        .unwrap()
    {
        UploadPlan::Upload(value) => value,
        _ => panic!("fresh single fixture"),
    };
    pair.session
        .store
        .finish_sticker_upload(&lease, crate::now())
        .unwrap();
    corrupt_key(&path, 1, &descriptor);
    let peer = pair.alice_id.peer_id();
    let count = pair
        .session
        .store
        .current_chat_stickers(&peer)
        .unwrap()
        .len();
    let sealed = pair.session.store.sealed_device().unwrap();
    assert!(pair
        .session
        .send_pack_sticker_to(&mut [], &pair.bob_id, &peer, &pack, 0, &SEAL)
        .await
        .is_err());
    assert_eq!(
        pair.session
            .store
            .current_chat_stickers(&peer)
            .unwrap()
            .len(),
        count
    );
    assert_eq!(pair.session.store.sealed_device().unwrap(), sealed);
}

#[tokio::test]
async fn five_due_confirmed_jobs_leave_one_for_the_next_batch() {
    let (mut pair, _, first, _) = pending();
    confirmed(&mut pair, &first);
    for index in 2..=5 {
        let pack = sticker_packs::tests::pack(&format!("Cats {index}"));
        pair.session.store.save_sticker_pack(&SEAL, &pack).unwrap();
        pair.session
            .stage_sticker(pair.group.id(), &sticker(&pack), &SEAL)
            .unwrap();
        deliver(
            &mut pair,
            &Body::PackRequest(PackRequest {
                pack_id: *pack.id(),
            }),
            index + 2,
        );
        confirmed(&mut pair, &pack);
    }
    assert_eq!(
        pair.session
            .store
            .due_sticker_pack_responses(crate::now() * 1000)
            .unwrap()
            .len(),
        5
    );
    assert_eq!(
        pair.session
            .respond_sticker_pack_requests(&mut [], &pair.bob_id, &SEAL, || false)
            .await
            .unwrap(),
        4
    );
    assert_eq!(
        pair.session
            .store
            .due_sticker_pack_responses(crate::now() * 1000)
            .unwrap()
            .len(),
        1
    );
}
