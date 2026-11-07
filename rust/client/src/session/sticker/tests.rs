use super::*;
use crate::chat;
use crate::session::tests::{bob_invited, Invite};
use crate::store::messages::{DeliveryState, MessageKey};
use crate::store::{Conversation, InboxKey, Store};

fn inbox(id: u8) -> InboxKey {
    InboxKey {
        mailbox: [9; 32],
        id: [id; 16],
    }
}

#[test]
fn sticker_sends_have_fresh_ids_and_durable_media_delivery() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../build/sticker-session-tests");
    std::fs::create_dir_all(&root).unwrap();
    let mut random = [0; 16];
    getrandom::fill(&mut random).unwrap();
    let path = root.join(format!("{:032x}.db", u128::from_be_bytes(random)));
    let seal = [5; 32];
    let Invite {
        mut session,
        bob_id,
        alice_id,
        alice,
        mut group,
        welcome,
        profiles,
    } = bob_invited(&path, &seal);
    session
        .stage_envelope(&inbox(1), &welcome, &profiles, &seal)
        .unwrap();
    let peer = alice_id.peer_id();
    let template = Sticker {
        id: [99; 16],
        hlc: 0,
        pack_id: [2; 32],
        sha256: [3; 32],
        emoji: "😺".into(),
        pack_name: "Cats".into(),
        attachment: None,
    };
    let before = session.store.sealed_device().unwrap();
    assert!(matches!(
        session.stage_sticker(group.id(), &template, &seal),
        Err(Error::ConversationNotOpen)
    ));
    assert!(session.store.sealed_device().unwrap() == before);
    let request = chat::message(
        &mut group,
        &alice,
        &Body::text_with_id([1; 16], b"hello".to_vec()),
    )
    .unwrap();
    session
        .stage_envelope(&inbox(2), &request[0].mls, &profiles, &seal)
        .unwrap();
    assert!(matches!(
        session.stage_sticker(group.id(), &template, &seal),
        Err(Error::ConversationNotOpen)
    ));
    session.stage_text(group.id(), b"reply", &seal).unwrap();
    let count = session.store.due_outbound(i64::MAX).unwrap().len();
    let (first, first_writes) = session.stage_sticker(group.id(), &template, &seal).unwrap();
    let (second, second_writes) = session.stage_sticker(group.id(), &template, &seal).unwrap();
    assert!(first != second && first != template.id && second != template.id);
    let rows = session.store.current_chat_stickers(&peer).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!((rows[0].key.id, rows[1].key.id), (first, second));
    assert!(rows[0].sticker.hlc < rows[1].sticker.hlc);
    assert!(rows
        .iter()
        .all(|row| !row.unread && row.delivery == Some(DeliveryState::Pending)));
    let due = session.store.due_outbound(i64::MAX).unwrap();
    assert_eq!(due.len(), count + first_writes.len() + second_writes.len());
    for write in first_writes.iter().chain(&second_writes) {
        assert!(due.iter().any(|saved| saved.id() == write.id()
            && saved.mailbox() == write.mailbox()
            && saved.as_bytes() == write.as_bytes()));
    }
    let mut invalid = template.clone();
    invalid.emoji = "two labels".into();
    let sealed = session.store.sealed_device().unwrap();
    assert!(session.stage_sticker(group.id(), &invalid, &seal).is_err());
    assert!(session.store.sealed_device().unwrap() == sealed && session.device.is_some());
    assert_eq!(
        session.store.due_outbound(i64::MAX).unwrap().len(),
        due.len()
    );
    assert!(matches!(
        session.stage_edit(group.id(), first, b"no", &seal),
        Err(Error::UnknownMessage)
    ));
    session.stage_delete(group.id(), first, &seal).unwrap();
    let target = MessageKey {
        group: group.id().to_vec(),
        author: bob_id.peer_id(),
        id: first,
    };
    assert!(session.chat_message(&target).unwrap().unwrap().deleted);
    assert_eq!(session.store.current_chat_stickers(&peer).unwrap().len(), 1);
    assert!(matches!(
        session.stage_delete(group.id(), first, &seal),
        Err(Error::UnknownMessage)
    ));
    drop(session);
    let mut session = Session::open(Store::open(&path, &bob_id).unwrap(), &seal).unwrap();
    let retained = session.store.current_chat_stickers(&peer).unwrap();
    assert_eq!(retained[0].key.id, second);
    assert_eq!(retained[0].delivery, Some(DeliveryState::Pending));
    assert!(session.chat_message(&target).unwrap().unwrap().deleted);
    let due = session.store.due_outbound(i64::MAX).unwrap();
    for write in &second_writes {
        assert!(due.iter().any(|saved| saved.as_bytes() == write.as_bytes()));
    }
    session.store.set_blocked(&peer, true).unwrap();
    assert!(matches!(
        session.stage_sticker(group.id(), &template, &seal),
        Err(Error::Store(crate::store::Error::BlockedPeer))
    ));
    session.store.set_blocked(&peer, false).unwrap();
    let device = session.device.as_ref().unwrap().device_key().unwrap();
    let now = crate::now() * 1000;
    // A text reply opens a conversation without creating a mutual Like.
    session
        .store
        .interact(&peer, crate::interactions::Kind::Like, &device, now)
        .unwrap();
    session
        .store
        .receive_interaction(
            &peer,
            &alice.device_key().unwrap(),
            crate::interactions::Interaction {
                kind: crate::interactions::Kind::Like,
                event_id: [12; 16],
                hlc: dyapp_messaging::hlc::Timestamp::new(now, 0).into(),
            },
            now,
        )
        .unwrap();
    session
        .store
        .interact(
            &peer,
            crate::interactions::Kind::Unlike,
            &device,
            crate::now() * 1000,
        )
        .unwrap();
    assert_eq!(
        session.store.conversation(&peer).unwrap(),
        Conversation::Closed
    );
    assert!(matches!(
        session.stage_sticker(group.id(), &template, &seal),
        Err(Error::ConversationNotOpen)
    ));
}
