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

fn template() -> Sticker {
    Sticker {
        id: [99; 16],
        hlc: 0,
        pack_id: [2; 32],
        sha256: [3; 32],
        emoji: "😺".into(),
        pack_name: "Cats".into(),
        attachment: None,
    }
}

fn before_open(pair: &mut Invite, template: &Sticker, seal: &[u8; 32]) {
    let before = pair.session.store.sealed_device().unwrap();
    assert!(matches!(
        pair.session.stage_sticker(pair.group.id(), template, seal),
        Err(Error::ConversationNotOpen)
    ));
    assert_eq!(pair.session.store.sealed_device().unwrap(), before);
    let request = chat::message(
        &mut pair.group,
        &pair.alice,
        &Body::text_with_id([1; 16], b"hello".to_vec()),
    )
    .unwrap();
    pair.session
        .stage_envelope(&inbox(2), &request[0].mls, &pair.profiles, seal)
        .unwrap();
    assert!(matches!(
        pair.session.stage_sticker(pair.group.id(), template, seal),
        Err(Error::ConversationNotOpen)
    ));
    pair.session
        .stage_text(pair.group.id(), b"reply", seal)
        .unwrap();
}

fn fresh_ids(
    pair: &mut Invite,
    template: &Sticker,
    seal: &[u8; 32],
) -> ([u8; 16], [u8; 16], Vec<crate::PreparedEnvelope>) {
    let peer = pair.alice_id.peer_id();
    let count = pair.session.store.due_outbound(i64::MAX).unwrap().len();
    let (first, first_writes) = pair
        .session
        .stage_sticker(pair.group.id(), template, seal)
        .unwrap();
    let (second, second_writes) = pair
        .session
        .stage_sticker(pair.group.id(), template, seal)
        .unwrap();
    assert!(first != second && first != template.id && second != template.id);
    let rows = pair.session.store.current_chat_stickers(&peer).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!((rows[0].key.id, rows[1].key.id), (first, second));
    assert!(rows[0].sticker.hlc < rows[1].sticker.hlc);
    assert!(rows
        .iter()
        .all(|row| !row.unread && row.delivery == Some(DeliveryState::Pending)));
    let due = pair.session.store.due_outbound(i64::MAX).unwrap();
    assert_eq!(due.len(), count + first_writes.len() + second_writes.len());
    for write in first_writes.iter().chain(&second_writes) {
        assert!(due.iter().any(|saved| saved.id() == write.id()
            && saved.mailbox() == write.mailbox()
            && saved.as_bytes() == write.as_bytes()));
    }
    let mut invalid = template.clone();
    invalid.emoji = "two labels".into();
    let sealed = pair.session.store.sealed_device().unwrap();
    assert!(pair
        .session
        .stage_sticker(pair.group.id(), &invalid, seal)
        .is_err());
    assert!(pair.session.store.sealed_device().unwrap() == sealed && pair.session.device.is_some());
    assert_eq!(
        pair.session.store.due_outbound(i64::MAX).unwrap().len(),
        due.len()
    );
    (first, second, second_writes)
}

fn delete(pair: &mut Invite, first: [u8; 16], seal: &[u8; 32]) -> MessageKey {
    let peer = pair.alice_id.peer_id();
    assert!(matches!(
        pair.session.stage_edit(pair.group.id(), first, b"no", seal),
        Err(Error::UnknownMessage)
    ));
    pair.session
        .stage_delete(pair.group.id(), first, seal)
        .unwrap();
    let target = MessageKey {
        group: pair.group.id().to_vec(),
        author: pair.bob_id.peer_id(),
        id: first,
    };
    assert!(pair.session.chat_message(&target).unwrap().unwrap().deleted);
    assert_eq!(
        pair.session
            .store
            .current_chat_stickers(&peer)
            .unwrap()
            .len(),
        1
    );
    assert!(matches!(
        pair.session.stage_delete(pair.group.id(), first, seal),
        Err(Error::UnknownMessage)
    ));
    target
}

fn after_reopen(
    session: &mut Session,
    peer: &str,
    second: [u8; 16],
    second_writes: &[crate::PreparedEnvelope],
    target: &MessageKey,
) {
    let retained = session.store.current_chat_stickers(peer).unwrap();
    assert_eq!(retained[0].key.id, second);
    assert_eq!(retained[0].delivery, Some(DeliveryState::Pending));
    assert!(session.chat_message(target).unwrap().unwrap().deleted);
    let due = session.store.due_outbound(i64::MAX).unwrap();
    for write in second_writes {
        assert!(due.iter().any(|saved| saved.as_bytes() == write.as_bytes()));
    }
}

fn closed(
    session: &mut Session,
    group_id: &[u8],
    peer: &str,
    alice_device: &[u8],
    template: &Sticker,
    seal: &[u8; 32],
) {
    session.store.set_blocked(peer, true).unwrap();
    assert!(matches!(
        session.stage_sticker(group_id, template, seal),
        Err(Error::Store(crate::store::Error::BlockedPeer))
    ));
    session.store.set_blocked(peer, false).unwrap();
    let device = session.device.as_ref().unwrap().device_key().unwrap();
    let now = crate::now() * 1000;
    // A text reply opens a conversation without creating a mutual Like.
    session
        .store
        .interact(peer, crate::interactions::Kind::Like, &device, now)
        .unwrap();
    session
        .store
        .receive_interaction(
            peer,
            alice_device,
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
            peer,
            crate::interactions::Kind::Unlike,
            &device,
            crate::now() * 1000,
        )
        .unwrap();
    assert_eq!(
        session.store.conversation(peer).unwrap(),
        Conversation::Closed
    );
    assert!(matches!(
        session.stage_sticker(group_id, template, seal),
        Err(Error::ConversationNotOpen)
    ));
}

#[test]
fn sticker_sends_have_fresh_ids_and_durable_media_delivery() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let root = test_dir.path();
    let mut random = [0; 16];
    getrandom::fill(&mut random).unwrap();
    let path = root.join(format!("{:032x}.db", u128::from_be_bytes(random)));
    let seal = [5; 32];
    let mut pair = bob_invited(&path, &seal);
    pair.session
        .stage_envelope(&inbox(1), &pair.welcome, &pair.profiles, &seal)
        .unwrap();
    let peer = pair.alice_id.peer_id();
    let template = template();
    before_open(&mut pair, &template, &seal);
    let (first, second, second_writes) = fresh_ids(&mut pair, &template, &seal);
    let target = delete(&mut pair, first, &seal);
    let group_id = pair.group.id().to_vec();
    let alice_device = pair.alice.device_key().unwrap();
    let Invite { bob_id, .. } = pair;
    let mut session = Session::open(Store::open(&path, &bob_id).unwrap(), &seal).unwrap();
    after_reopen(&mut session, &peer, second, &second_writes, &target);
    closed(
        &mut session,
        &group_id,
        &peer,
        &alice_device,
        &template,
        &seal,
    );
}
