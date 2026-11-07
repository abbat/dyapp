use super::*;
use crate::chat::sticker::Sticker;

fn save(store: &mut Store, group: &[u8], peer: &str, id: u8, time: u64) {
    let sticker = Sticker {
        id: [id; 16],
        hlc: Timestamp::new(time, 0).into(),
        pack_id: [1; 32],
        sha256: [2; 32],
        emoji: "😺".into(),
        pack_name: "Cats".into(),
        attachment: None,
    };
    store
        .record_chat_sticker(
            VerifiedChatSender {
                group,
                author: peer,
                device: &[6; 32],
            },
            &sticker,
            true,
        )
        .unwrap();
}

fn open(store: &Store, peer: &str) {
    store
        .db
        .execute(
            "INSERT INTO `conversations` (`peer_id`, `state`) VALUES (?1, 3)",
            [peer],
        )
        .unwrap();
}

#[test]
fn mixed_kinds_select_one_preview_and_count_every_unread_once() {
    let owner = Identity::generate();
    let peer = Identity::generate().peer_id();
    let other = Identity::generate().peer_id();
    let mut store = Store::open(&super::super::tests::temp_db_path(), &owner).unwrap();
    for (group, name) in [(&[3][..], &peer), (&[4], &peer), (&[5], &other)] {
        super::super::tests::catalog_group(&mut store, group, name);
    }
    open(&store, &peer);
    open(&store, &other);
    deliver(&mut store, 1, &[3], &peer, text(1, Some(300)));
    for (group, name, id, time) in [(&[3][..], &peer, 2, 400), (&[5], &other, 5, 450)] {
        store
            .record_chat_attachment(
                VerifiedChatSender {
                    group,
                    author: name,
                    device: &[6; 32],
                },
                &photo(id, time),
                true,
            )
            .unwrap();
    }
    save(&mut store, &[4], &peer, 3, 500);
    save(&mut store, &[3], &peer, 4, 500);
    let rows = store.chat_rows().unwrap();
    assert_eq!(rows[0].peer_id, peer);
    assert_eq!(rows[0].last_sticker.as_ref().unwrap().key.id, [4; 16]);
    assert!(rows[0].last_attachment.is_none());
    assert_eq!(rows[0].unread_count, 4);
    assert_eq!(rows[1].peer_id, other);
    assert_eq!(store.clear_chat(&[3]).unwrap(), 3);
    let rows = store.chat_rows().unwrap();
    assert_eq!(rows[0].last_sticker.as_ref().unwrap().key.id, [3; 16]);
    assert_eq!(rows[0].unread_count, 1);
    assert_eq!(store.mark_chat_read(&peer).unwrap(), 1);
    deliver(&mut store, 7, &[4], &peer, text(7, Some(600)));
    let rows = store.chat_rows().unwrap();
    assert!(rows[0].last_sticker.is_none() && rows[0].last_attachment.is_none());
    assert_eq!(rows[0].last_message.as_ref().unwrap().key.id, [7; 16]);
    assert_eq!(store.clear_chat_pair(&peer).unwrap(), 2);
    assert_eq!(store.chat_rows().unwrap()[0].peer_id, other);
}

#[test]
fn closed_sticker_only_history_survives_restart_delete_then_clear() {
    let path = super::super::tests::temp_db_path();
    let owner = Identity::generate();
    let peer = Identity::generate().peer_id();
    let mut store = Store::open(&path, &owner).unwrap();
    super::super::tests::catalog_group(&mut store, &[3], &peer);
    open(&store, &peer);
    save(&mut store, &[3], &peer, 1, 300);
    store
        .db
        .execute(
            "UPDATE `conversations` SET `state` = 4 WHERE `peer_id` = ?1",
            [&peer],
        )
        .unwrap();
    drop(store);
    let mut store = Store::open(&path, &owner).unwrap();
    let rows = store.chat_rows().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].state, Conversation::Closed);
    assert_eq!(rows[0].unread_count, 1);
    assert!(rows[0].last_message.is_none());
    assert_eq!(rows[0].last_sticker.as_ref().unwrap().key.id, [1; 16]);
    let key = rows[0].last_sticker.as_ref().unwrap().key.clone();
    store
        .apply_chat_operation(
            &key,
            &[6; 32],
            &Body::Delete {
                target: key.id,
                operation: [9; 16],
                hlc: Timestamp::new(400, 0).into(),
            },
            1000,
        )
        .unwrap();
    let rows = store.chat_rows().unwrap();
    assert!(rows[0].last_sticker.is_none());
    assert!(rows[0].last_message.as_ref().unwrap().current.deleted);
    assert_eq!(rows[0].unread_count, 0);
    assert_eq!(store.clear_chat_pair(&peer).unwrap(), 1);
    assert!(store.chat_rows().unwrap().is_empty());
}
