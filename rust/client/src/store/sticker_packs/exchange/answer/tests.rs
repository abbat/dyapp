use super::*;
use crate::attachment::Attachment;
use crate::store::sticker_packs::exchange::tests::{database, selected};

fn inbox(id: u8) -> InboxKey {
    InboxKey {
        mailbox: [9; 32],
        id: [id; 16],
    }
}
fn sender(peer: &str) -> VerifiedChatSender<'_> {
    VerifiedChatSender {
        group: &[9],
        author: peer,
        device: &[8; 32],
    }
}
fn answer() -> PackAnswer {
    PackAnswer {
        pack_id: [1; 32],
        attachment: Attachment {
            key: [1; 32],
            plaintext_sha256: [2; 32],
            blob_sha256: [3; 32],
            release_secret: [0; 32],
            size: 100,
            preview: vec![],
            mime: "application/octet-stream".into(),
            width: 0,
            height: 0,
        },
    }
}

#[test]
fn unsolicited_cross_peer_blocked_and_closed_answers_only_commit_ack() {
    let (mut store, _, _, peer) = database();
    let answer = answer();
    assert!(!store
        .stage_sticker_pack_answer(b"drop", &inbox(1), sender(&peer), &answer)
        .unwrap());
    assert!(store
        .pending_sticker_pack(&peer, &answer.pack_id)
        .unwrap()
        .is_none());
    assert!(store.pending_acks().unwrap().contains(&inbox(1)));
    let selected = selected(&mut store, &peer, 1);
    let request = store.sticker_pack_request(&[9], &selected).unwrap();
    let pending = store
        .stage_sticker_pack_request(b"request", &[], &[9], &selected, &request)
        .unwrap();
    let stranger = dyapp_identity::Identity::generate().peer_id();
    crate::store::tests::catalog_group(&mut store, &[10], &stranger);
    store
        .db
        .execute(
            "INSERT INTO `conversations` (`peer_id`, `state`, `request_id`) VALUES (?, 3, NULL)",
            [&stranger],
        )
        .unwrap();
    assert!(!store
        .stage_sticker_pack_answer(
            b"cross",
            &inbox(2),
            VerifiedChatSender {
                group: &[10],
                ..sender(&stranger)
            },
            &answer
        )
        .unwrap());
    store.set_blocked(&peer, true).unwrap();
    assert!(!store
        .stage_sticker_pack_answer(b"blocked", &inbox(3), sender(&peer), &answer)
        .unwrap());
    assert!(store
        .sticker_pack_answer(&[9], &peer, &answer.pack_id, &pending.token)
        .is_err());
    store.set_blocked(&peer, false).unwrap();
    store
        .db
        .execute(
            "UPDATE `conversations` SET `state` = 4 WHERE `peer_id` = ?",
            [&peer],
        )
        .unwrap();
    assert!(!store
        .stage_sticker_pack_answer(b"closed", &inbox(4), sender(&peer), &answer)
        .unwrap());
    assert_eq!(store.pending_acks().unwrap().len(), 4);
    let stored: Option<Vec<u8>> = store
        .db
        .query_row("SELECT `answer` FROM `sticker_pack_requests`", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert!(stored.is_none());
    assert_eq!(store.sealed_device().unwrap().unwrap(), b"closed");
}

#[test]
fn first_answer_survives_replay_conflict_restart_and_token_checks() {
    let (mut store, owner, path, peer) = database();
    let selected = selected(&mut store, &peer, 1);
    let request = store.sticker_pack_request(&[9], &selected).unwrap();
    let pending = store
        .stage_sticker_pack_request(b"request", &[], &[9], &selected, &request)
        .unwrap();
    let answer = answer();
    assert!(store
        .stage_sticker_pack_answer(b"first", &inbox(1), sender(&peer), &answer)
        .unwrap());
    assert!(!store
        .stage_sticker_pack_answer(b"replay", &inbox(1), sender(&peer), &answer)
        .unwrap());
    assert_eq!(store.sealed_device().unwrap().unwrap(), b"first");
    let mut conflict = answer.clone();
    conflict.attachment.blob_sha256 = [4; 32];
    assert!(store
        .stage_sticker_pack_answer(b"bad", &inbox(1), sender(&peer), &conflict)
        .is_err());
    assert!(!store
        .stage_sticker_pack_answer(b"ignored", &inbox(2), sender(&peer), &conflict)
        .unwrap());
    assert!(store
        .sticker_pack_answer(&[9], &peer, &request.pack_id, &[0; 16])
        .unwrap()
        .is_none());
    drop(store);
    let mut store = Store::open(&path, &owner).unwrap();
    assert!(
        store
            .sticker_pack_answer(&[9], &peer, &request.pack_id, &pending.token)
            .unwrap()
            == Some(answer.clone())
    );
    assert_eq!(
        store
            .pending_sticker_pack(&peer, &request.pack_id)
            .unwrap()
            .unwrap()
            .sticker_hash,
        [7; 32]
    );
    store
        .db
        .execute(
            "UPDATE `sticker_pack_requests` SET `answer` = ?",
            [vec![1_u8]],
        )
        .unwrap();
    assert!(store
        .sticker_pack_answer(&[9], &peer, &request.pack_id, &pending.token)
        .is_err());
    assert!(store
        .stage_sticker_pack_answer(b"corrupt", &inbox(3), sender(&peer), &answer)
        .is_err());
    assert!(!store.has_inbox(&inbox(3)).unwrap());
    assert_eq!(store.sealed_device().unwrap().unwrap(), b"ignored");
}

#[test]
fn descriptor_write_failure_rolls_back_mls_inbox_and_ack() {
    let (mut store, _, _, peer) = database();
    let selected = selected(&mut store, &peer, 1);
    let request = store.sticker_pack_request(&[9], &selected).unwrap();
    let pending = store
        .stage_sticker_pack_request(b"before", &[], &[9], &selected, &request)
        .unwrap();
    store
        .db
        .execute_batch(
            "CREATE TRIGGER `reject_answer` BEFORE UPDATE OF `answer` ON `sticker_pack_requests`
        BEGIN SELECT RAISE(ABORT, 'fixture answer failure'); END;",
        )
        .unwrap();
    assert!(store
        .stage_sticker_pack_answer(b"after", &inbox(1), sender(&peer), &answer())
        .is_err());
    assert!(!store.has_inbox(&inbox(1)).unwrap());
    assert_eq!(
        store.pending_acks().unwrap(),
        [] as [crate::store::InboxKey; 0]
    );
    assert_eq!(store.sealed_device().unwrap().unwrap(), b"before");
    assert!(store
        .sticker_pack_answer(&[9], &peer, &request.pack_id, &pending.token)
        .unwrap()
        .is_none());
}
