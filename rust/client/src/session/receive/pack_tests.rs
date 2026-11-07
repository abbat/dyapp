use crate::chat::{
    self,
    pack_control::{PackAnswer, PackRequest},
    sticker::Sticker,
    Body, Event,
};
use crate::session::tests::{bob_invited, Invite};
use crate::store::{messages::MessageKey, InboxKey};
const SEAL: [u8; 32] = [5; 32];

fn key(id: u8) -> InboxKey {
    InboxKey {
        mailbox: [9; 32],
        id: [id; 16],
    }
}
fn deliver(pair: &mut Invite, body: &Body, id: u8) -> Option<Event> {
    let writes = chat::message(&mut pair.group, &pair.alice, body).unwrap();
    pair.session
        .stage_envelope(&key(id), &writes[0].mls, &pair.profiles, &SEAL)
        .unwrap()
}
async fn consumed(pair: &mut Invite, body: &Body, id: u8) {
    let event = deliver(pair, body, id).unwrap();
    assert!(super::receipted_group(&event).is_some());
    assert!(pair
        .session
        .control(&mut [], &pair.bob_id, event, &SEAL)
        .await
        .unwrap()
        .is_none());
}
fn sticker(pack: u8) -> Sticker {
    Sticker {
        id: [3; 16],
        hlc: 1000,
        pack_id: [pack; 32],
        sha256: [4; 32],
        emoji: "😺".into(),
        pack_name: "Cats".into(),
        attachment: None,
    }
}
fn answer(pack: u8) -> Body {
    let (mut attachment, _) = crate::attachment::seal(b"metadata only").unwrap();
    attachment.release_secret = [0; 32];
    Body::PackAnswer(Box::new(PackAnswer {
        pack_id: [pack; 32],
        attachment,
    }))
}

#[tokio::test]
async fn authenticated_pack_controls_are_guarded_durable_receipted_and_hidden() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../build/pack-session-tests");
    std::fs::create_dir_all(&root).unwrap();
    let mut random = [0; 16];
    getrandom::fill(&mut random).unwrap();
    let directory = root.join(format!("{:032x}", u128::from_be_bytes(random)));
    std::fs::create_dir(&directory).unwrap();
    let mut pair = bob_invited(&directory.join("client.db"), &SEAL);
    pair.session
        .stage_envelope(&key(1), &pair.welcome, &pair.profiles, &SEAL)
        .unwrap();
    let peer = pair.alice_id.peer_id();
    let request = Body::PackRequest(PackRequest { pack_id: [2; 32] });
    assert!(deliver(&mut pair, &request, 2).is_none());
    assert!(deliver(&mut pair, &answer(3), 3).is_none());
    assert!(pair
        .session
        .store
        .due_sticker_pack_responses(crate::now() * 1000)
        .unwrap()
        .is_empty());
    deliver(
        &mut pair,
        &Body::text_with_id([1; 16], b"hello".to_vec()),
        4,
    )
    .unwrap();
    pair.session
        .stage_text(pair.group.id(), b"reply", &SEAL)
        .unwrap();
    let before = pair.session.store.sealed_device().unwrap();
    assert!(pair
        .session
        .send_body(&mut [], pair.group.id(), &request, &SEAL)
        .await
        .is_err());
    assert!(pair.session.device.is_some());
    assert!(pair.session.store.sealed_device().unwrap() == before);
    // A new envelope receives a receipt uniformly, even without pack/pending permission.
    consumed(&mut pair, &request, 5).await;
    assert!(pair
        .session
        .store
        .due_sticker_pack_responses(crate::now() * 1000)
        .unwrap()
        .is_empty());
    pair.session
        .stage_sticker(pair.group.id(), &sticker(2), &SEAL)
        .unwrap();
    let history = pair.session.store.current_chat_items(&peer).unwrap().len();
    consumed(&mut pair, &request, 6).await;
    let jobs = pair
        .session
        .store
        .due_sticker_pack_responses(crate::now() * 1000)
        .unwrap();
    assert_eq!(jobs.len(), 1);
    consumed(&mut pair, &request, 7).await;
    assert!(
        pair.session
            .store
            .due_sticker_pack_responses(crate::now() * 1000)
            .unwrap()
            == jobs
    );
    assert_eq!(
        pair.session.store.current_chat_items(&peer).unwrap().len(),
        history
    );
    deliver(&mut pair, &Body::Sticker(Box::new(sticker(3))), 8).unwrap();
    let selected = MessageKey {
        group: pair.group.id().to_vec(),
        author: peer.clone(),
        id: [3; 16],
    };
    let pending_request = pair
        .session
        .store
        .sticker_pack_request(pair.group.id(), &selected)
        .unwrap();
    let sealed = pair.session.store.sealed_device().unwrap().unwrap();
    let pending = pair
        .session
        .store
        .stage_sticker_pack_request(&sealed, &[], pair.group.id(), &selected, &pending_request)
        .unwrap();
    let history = pair.session.store.current_chat_items(&peer).unwrap().len();
    consumed(&mut pair, &answer(4), 9).await;
    assert!(pair
        .session
        .store
        .sticker_pack_answer(pair.group.id(), &peer, &[3; 32], &pending.token)
        .unwrap()
        .is_none());
    let first = answer(3);
    consumed(&mut pair, &first, 10).await;
    consumed(&mut pair, &answer(3), 11).await;
    let Body::PackAnswer(expected) = first else {
        panic!("fixture answer")
    };
    assert!(
        pair.session
            .store
            .sticker_pack_answer(pair.group.id(), &peer, &[3; 32], &pending.token)
            .unwrap()
            == Some(*expected)
    );
    assert_eq!(
        pair.session.store.current_chat_items(&peer).unwrap().len(),
        history
    );
    pair.session.store.set_blocked(&peer, true).unwrap();
    assert!(deliver(&mut pair, &request, 12).is_none());
    assert!(deliver(&mut pair, &answer(3), 13).is_none());
    assert!(pair.session.store.has_inbox(&key(12)).unwrap());
    assert!(pair.session.store.has_inbox(&key(13)).unwrap());
}
