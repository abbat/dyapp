use crate::chat::{self, sticker::Sticker, Body};
use crate::session::tests::{bob_invited, Invite};
use crate::session::Session;
use crate::store::messages::MessageKey;
use crate::store::{InboxKey, Store};
use dyapp_messaging::hlc::Timestamp;

fn key(id: u8) -> InboxKey {
    InboxKey {
        mailbox: [9; 32],
        id: [id; 16],
    }
}

fn sticker() -> Sticker {
    Sticker {
        id: [3; 16],
        hlc: Timestamp::new(1_000, 0).into(),
        pack_id: [2; 32],
        sha256: [4; 32],
        emoji: "😺".into(),
        pack_name: "Cats".into(),
        attachment: None,
    }
}

#[test]
fn sticker_receive_is_atomic_matched_replay_safe_and_deletable() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../build/sticker-session-tests");
    std::fs::create_dir_all(&root).unwrap();
    let mut random = [0; 16];
    getrandom::fill(&mut random).unwrap();
    let path = root.join(format!("{:032x}.db", u128::from_be_bytes(random)));
    let seal_key = [5; 32];
    let Invite {
        mut session,
        bob_id,
        alice_id,
        alice,
        mut group,
        welcome,
        profiles,
    } = bob_invited(&path, &seal_key);
    session
        .stage_envelope(&key(1), &welcome, &profiles, &seal_key)
        .unwrap();
    let peer = alice_id.peer_id();
    let body = Body::Sticker(Box::new(sticker()));
    let before = chat::message(&mut group, &alice, &body).unwrap();
    assert!(session
        .stage_envelope(&key(2), &before[0].mls, &profiles, &seal_key)
        .unwrap()
        .is_none());
    assert!(session
        .store
        .current_chat_stickers(&peer)
        .unwrap()
        .is_empty());
    assert!(session.store.has_inbox(&key(2)).unwrap());

    let request = chat::message(
        &mut group,
        &alice,
        &Body::text_with_id([1; 16], b"hello".to_vec()),
    )
    .unwrap();
    session
        .stage_envelope(&key(3), &request[0].mls, &profiles, &seal_key)
        .unwrap();
    session.stage_text(group.id(), b"reply", &seal_key).unwrap();
    let received = chat::message(&mut group, &alice, &body).unwrap();
    assert!(session
        .stage_envelope(&key(4), &received[0].mls, &profiles, &seal_key)
        .unwrap()
        .is_some());
    let rows = session.store.current_chat_stickers(&peer).unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].sticker == sticker() && rows[0].unread);
    assert!(session.store.has_inbox(&key(4)).unwrap());
    session.mark_chat_read(&peer).unwrap();
    let replay = chat::message(&mut group, &alice, &body).unwrap();
    assert!(session
        .stage_envelope(&key(5), &replay[0].mls, &profiles, &seal_key)
        .unwrap()
        .is_none());
    assert!(!session.store.current_chat_stickers(&peer).unwrap()[0].unread);

    // Projection conflict must roll back both inbox/ACK and the sealed MLS state.
    let sealed = session.store.sealed_device().unwrap();
    let mut conflict = sticker();
    conflict.sha256 = [8; 32];
    let bad = chat::message(&mut group, &alice, &Body::Sticker(Box::new(conflict))).unwrap();
    assert!(session
        .stage_envelope(&key(6), &bad[0].mls, &profiles, &seal_key)
        .is_err());
    assert!(!session.store.has_inbox(&key(6)).unwrap());
    assert_eq!(session.store.sealed_device().unwrap(), sealed);
    session.recover(&seal_key).unwrap();
    assert!(session.store.current_chat_stickers(&peer).unwrap()[0].sticker == sticker());

    let hlc = Timestamp::new(crate::now() * 1000, 1).into();
    let edit = Body::Edit {
        target: [3; 16],
        operation: [7; 16],
        hlc,
        text: b"replacement".to_vec(),
    };
    let edit = chat::message(&mut group, &alice, &edit).unwrap();
    assert!(session
        .stage_envelope(&key(7), &edit[0].mls, &profiles, &seal_key)
        .is_err());
    assert!(!session.store.has_inbox(&key(7)).unwrap());
    session.recover(&seal_key).unwrap();
    let delete = Body::Delete {
        target: [3; 16],
        operation: [8; 16],
        hlc,
    };
    let delete = chat::message(&mut group, &alice, &delete).unwrap();
    session
        .stage_envelope(&key(8), &delete[0].mls, &profiles, &seal_key)
        .unwrap();
    assert!(session
        .store
        .current_chat_stickers(&peer)
        .unwrap()
        .is_empty());
    let replay = chat::message(&mut group, &alice, &body).unwrap();
    session
        .stage_envelope(&key(9), &replay[0].mls, &profiles, &seal_key)
        .unwrap();
    assert!(session
        .store
        .current_chat_stickers(&peer)
        .unwrap()
        .is_empty());
    let target = MessageKey {
        group: group.id().to_vec(),
        author: peer.clone(),
        id: [3; 16],
    };
    assert!(session.chat_message(&target).unwrap().unwrap().deleted);
    drop(session);
    let reopened = Session::open(Store::open(&path, &bob_id).unwrap(), &seal_key).unwrap();
    assert!(reopened
        .store
        .current_chat_stickers(&peer)
        .unwrap()
        .is_empty());
    assert!(reopened.chat_message(&target).unwrap().unwrap().deleted);
    assert!(reopened.store.has_inbox(&key(9)).unwrap());
}
