use super::*;
mod adversarial;
use crate::chat::sticker::Sticker;
use crate::store::{messages::VerifiedChatSender, tests::catalog_group};
use dyapp_identity::Identity;

pub(super) fn database() -> (Store, Identity, std::path::PathBuf, String) {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../build/pack-exchange-tests");
    std::fs::create_dir_all(&root).unwrap();
    let mut random = [0; 16];
    getrandom::fill(&mut random).unwrap();
    // Store media belongs to the database's parent, so each identity needs its own folder.
    let directory = root.join(format!("{:032x}", u128::from_be_bytes(random)));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("client.db");
    let owner = Identity::generate();
    let peer = Identity::generate().peer_id();
    let mut store = Store::open(&path, &owner).unwrap();
    catalog_group(&mut store, &[9], &peer);
    store
        .db
        .execute(
            "INSERT INTO `conversations` (`peer_id`, `state`, `request_id`) VALUES (?, 3, NULL)",
            [&peer],
        )
        .unwrap();
    (store, owner, path, peer)
}
pub(super) fn selected(store: &mut Store, peer: &str, id: u8) -> MessageKey {
    store
        .record_chat_sticker(
            VerifiedChatSender {
                group: &[9],
                author: peer,
                device: &[8; 32],
            },
            &Sticker {
                id: [id; 16],
                hlc: id.into(),
                pack_id: [id; 32],
                sha256: [7; 32],
                emoji: "😺".into(),
                pack_name: "Cats".into(),
                attachment: None,
            },
            true,
        )
        .unwrap();
    MessageKey {
        group: vec![9],
        author: peer.into(),
        id: [id; 16],
    }
}

#[test]
fn request_restart_retry_and_generation_keep_clicked_hash_and_exact_outbox() {
    let (mut store, owner, path, peer) = database();
    let key = selected(&mut store, &peer, 1);
    let request = store.sticker_pack_request(&[9], &key).unwrap();
    let write = crate::prepare_put(&[1; 32], vec![9]).unwrap();
    let pending = store
        .stage_sticker_pack_request(b"ready", std::slice::from_ref(&write), &[9], &key, &request)
        .unwrap();
    assert_eq!(pending.sticker_hash, [7; 32]);
    let retried = store
        .stage_sticker_pack_request(b"retry", std::slice::from_ref(&write), &[9], &key, &request)
        .unwrap();
    assert!(pending == retried);
    drop(store);
    let mut store = Store::open(&path, &owner).unwrap();
    assert!(store.pending_sticker_pack(&peer, &request.pack_id).unwrap() == Some(pending.clone()));
    assert_eq!(store.sealed_device().unwrap().unwrap(), b"retry");
    let due = store.due_outbound(i64::MAX).unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].as_bytes(), write.as_bytes());
    let group: Vec<u8> = store
        .db
        .query_row("SELECT `chat_group` FROM `outbox`", [], |row| row.get(0))
        .unwrap();
    assert_eq!(group, [9]);
    assert!(!store
        .finish_sticker_pack(&peer, &request.pack_id, &[0; 16])
        .unwrap());
    assert!(store
        .finish_sticker_pack(&peer, &request.pack_id, &pending.token)
        .unwrap());
    let next = store
        .stage_sticker_pack_request(b"new", &[], &[9], &key, &request)
        .unwrap();
    assert_ne!(next.token, pending.token);
    assert!(!store
        .finish_sticker_pack(&peer, &request.pack_id, &pending.token)
        .unwrap());
}

#[test]
fn mls_and_conflicting_outbox_failures_roll_back_pending_intent() {
    let (mut store, _, _, peer) = database();
    let key = selected(&mut store, &peer, 1);
    let request = store.sticker_pack_request(&[9], &key).unwrap();
    assert!(store
        .stage_sticker_pack_request(b"", &[], &[9], &key, &request)
        .is_err());
    assert!(store
        .pending_sticker_pack(&peer, &request.pack_id)
        .unwrap()
        .is_none());
    let write = crate::prepare_put(&[1; 32], vec![9]).unwrap();
    store
        .stage_outbound(b"before", std::slice::from_ref(&write))
        .unwrap();
    store
        .db
        .execute(
            "UPDATE `outbox` SET `signed_envelope` = ?",
            [b"corrupt".as_slice()],
        )
        .unwrap();
    assert!(store
        .stage_sticker_pack_request(b"after", &[write], &[9], &key, &request)
        .is_err());
    assert!(store
        .pending_sticker_pack(&peer, &request.pack_id)
        .unwrap()
        .is_none());
    assert_eq!(store.sealed_device().unwrap().unwrap(), b"before");
}
