use super::*;
use crate::{
    chat::Body,
    store::{messages::VerifiedChatSender, tests::catalog_group},
    PreparedEnvelope,
};
use dyapp_identity::Identity;

fn fixture() -> (Store, Identity, std::path::PathBuf, String) {
    let owner = Identity::generate();
    let peer = Identity::generate().peer_id();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../build/recent-sticker-tests")
        .join(owner.peer_id());
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("client.db");
    let mut store = Store::open(&path, &owner).unwrap();
    catalog_group(&mut store, &[9], &peer);
    store
        .db
        .execute(
            "INSERT INTO `conversations` (`peer_id`, `state`) VALUES (?, 3)",
            [&peer],
        )
        .unwrap();
    (store, owner, path, peer)
}
fn send(
    store: &mut Store,
    id: u8,
    pack: u8,
    sealed: &[u8],
) -> Result<(Sticker, PreparedEnvelope), Error> {
    let owner = store.owner_peer_id().to_owned();
    let sticker = Sticker {
        id: [id; 16],
        hlc: store.draft_chat_text(1000)?,
        pack_id: [pack; 32],
        sha256: [pack; 32],
        emoji: "😺".into(),
        pack_name: "Cats".into(),
        attachment: None,
    };
    let write = crate::prepare_put(&[id; 32], vec![id]).unwrap();
    store.stage_chat_outbound(
        sealed,
        std::slice::from_ref(&write),
        VerifiedChatSender {
            group: &[9],
            author: &owner,
            device: &[8; 32],
        },
        &Body::Sticker(Box::new(sticker.clone())),
        1000,
    )?;
    Ok((sticker, write))
}

#[test]
fn twenty_recent_local_sends_reorder_without_replay_or_received_copy_changes() {
    let (mut store, owner, path, peer) = fixture();
    for index in 1..=21 {
        send(&mut store, index, index, b"sent").unwrap();
    }
    let recent = store.recent_stickers().unwrap();
    assert_eq!(recent.len(), 20);
    assert_eq!(recent[0].pack_id, [21; 32]);
    assert_eq!(recent[19].pack_id, [2; 32]);
    let (sticker, write) = send(&mut store, 22, 10, b"repeat").unwrap();
    let before = store.recent_stickers().unwrap();
    assert_eq!(before[0].pack_id, [10; 32]);
    assert!(store
        .stage_chat_outbound(
            b"replay",
            &[write],
            VerifiedChatSender {
                group: &[9],
                author: &owner.peer_id(),
                device: &[8; 32]
            },
            &Body::Sticker(Box::new(sticker.clone())),
            1000
        )
        .is_err());
    for (author, id, unread) in [
        (peer.as_str(), 100, true),
        (owner.peer_id().as_str(), 101, false),
    ] {
        let mut copy = sticker.clone();
        copy.id = [id; 16];
        copy.pack_id = [99; 32];
        store
            .record_chat_sticker(
                VerifiedChatSender {
                    group: &[9],
                    author,
                    device: &[8; 32],
                },
                &copy,
                unread,
            )
            .unwrap();
    }
    assert!(store.recent_stickers().unwrap() == before);
    drop(store);
    let store = Store::open(&path, &owner).unwrap();
    assert!(store.recent_stickers().unwrap() == before);
}

#[test]
fn failed_recent_insert_or_trim_rolls_back_history_mls_hlc_and_outbox() {
    let (mut store, _, _, peer) = fixture();
    for index in 1..=20 {
        send(&mut store, index, index, b"before").unwrap();
    }
    let before = store.recent_stickers().unwrap();
    let outbox = store.due_outbound(i64::MAX).unwrap().len();
    let next_hlc = store.draft_chat_text(1000).unwrap();
    store
        .db
        .execute_batch(
            "CREATE TABLE `test_recent_failure` (`phase` INTEGER);
         INSERT INTO `test_recent_failure` VALUES (1);
         CREATE TRIGGER `test_recent_insert` BEFORE INSERT ON `sticker_recents`
         WHEN (SELECT `phase` FROM `test_recent_failure`) = 1
         BEGIN SELECT RAISE(ABORT, 'fixture insert failure'); END;
         CREATE TRIGGER `test_recent_trim` BEFORE DELETE ON `sticker_recents`
         WHEN (SELECT `phase` FROM `test_recent_failure`) = 2
         BEGIN SELECT RAISE(ABORT, 'fixture trim failure'); END;",
        )
        .unwrap();
    for phase in 1..=2 {
        store
            .db
            .execute("UPDATE `test_recent_failure` SET `phase` = ?", [phase])
            .unwrap();
        assert!(send(&mut store, 30 + phase as u8, 99, b"failed snapshot").is_err());
        assert!(store.recent_stickers().unwrap() == before);
        assert_eq!(store.sealed_device().unwrap().unwrap(), b"before");
        assert_eq!(store.current_chat_stickers(&peer).unwrap().len(), 20);
        assert_eq!(store.due_outbound(i64::MAX).unwrap().len(), outbox);
        assert_eq!(store.draft_chat_text(1000).unwrap(), next_hlc);
        assert!(!store.shared_sticker_pack(&peer, &[99; 32]).unwrap());
    }
}
