use super::*;

fn accept_concurrent_invite(invite: Invite, own: &[u8], seal_key: &[u8; 32]) {
    let Invite {
        mut session,
        bob_id,
        alice,
        mut group,
        welcome,
        profiles,
        ..
    } = invite;
    // Alice made a group at the same time and replies in it: both groups are kept.
    session
        .stage_envelope(&key(1), &welcome, &profiles, seal_key)
        .unwrap();
    let reply = chat::message(
        &mut group,
        &alice,
        &Body::text_with_id([7; 16], b"yo".to_vec()),
    )
    .unwrap()[0]
        .mls
        .clone();
    assert!(session
        .stage_envelope(&key(2), &reply, &profiles, seal_key)
        .unwrap()
        .is_some());
    let lowest = own.to_vec().min(group.id().to_vec());
    let (id, _) = session
        .stage_text_to(&profiles, &[], b"again", seal_key)
        .unwrap();
    let sent = MessageKey {
        group: lowest,
        author: bob_id.peer_id(),
        id,
    };
    assert!(session.chat_message(&sent).unwrap().is_some());
    assert_eq!(session.store.known_groups().unwrap().len(), 2);
}

#[test]
fn a_first_text_invites_and_the_pair_then_sends_to_its_lowest_group() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let seal_key = [5; 32];
    let path = test_dir.path().join("session-1.db");
    let mut invite = bob_invited(&path, &seal_key);
    let alice_peer = invite.alice_id.peer_id();
    let alice_key: [u8; 32] = invite.alice.device_key().unwrap().try_into().unwrap();
    let packages = [(
        crate::mailbox(&alice_key).to_vec(),
        invite.alice.key_package(false).unwrap(),
    )];
    let (first_id, writes) = invite
        .session
        .stage_text_to(&invite.profiles, &packages, b"hi", &seal_key)
        .unwrap();
    let sent = invite
        .session
        .store
        .current_chat_messages(&alice_peer)
        .unwrap();
    let first = sent
        .iter()
        .find(|message| message.key.id == first_id)
        .unwrap();
    assert!(first.original_hlc.is_some());
    // The Welcome and the text, both to Alice's device, in one commit.
    assert_eq!(writes.len(), 2);
    assert!(writes
        .iter()
        .all(|w| w.mailbox() == &crate::mailbox(&alice_key)));
    let own = invite.session.pair_group(&alice_peer).unwrap();

    // A restart retries the same staged bytes and reopens the new group.
    invite.session = Session::open(Store::open(&path, &invite.bob_id).unwrap(), &seal_key).unwrap();
    let due: Vec<_> = invite.session.due_outbound(i64::MAX).unwrap();
    let bytes = |writes: &[crate::PreparedEnvelope]| {
        let mut all: Vec<_> = writes.iter().map(|w| w.as_bytes().to_vec()).collect();
        all.sort();
        all
    };
    assert_eq!(bytes(&due), bytes(&writes));
    assert_eq!(invite.session.pair_group(&alice_peer), Some(own.clone()));
    accept_concurrent_invite(invite, &own, &seal_key);
}

#[test]
fn a_first_like_invites_and_stages_the_like_with_the_welcome() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let seal_key = [5; 32];
    let path = test_dir.path().join("session-1.db");
    let Invite {
        session,
        bob_id,
        alice_id,
        alice,
        profiles,
        ..
    } = bob_invited(&path, &seal_key);
    let mut session = session;
    let alice_peer = alice_id.peer_id();
    let alice_key: [u8; 32] = alice.device_key().unwrap().try_into().unwrap();
    let packages = [(
        crate::mailbox(&alice_key).to_vec(),
        alice.key_package(false).unwrap(),
    )];
    let before = crate::now() * 1000;
    let (like, writes) = session
        .stage_first_like(&profiles, &packages, &seal_key)
        .unwrap()
        .unwrap();
    assert!(Timestamp::from(like.hlc).wall_ms() >= before);
    // The Welcome and the like, both to Alice's device.
    assert_eq!(writes.len(), 2);
    assert!(session.likes(&alice_peer).unwrap().ours);
    let group = session.pair_group(&alice_peer).unwrap();

    // A restart keeps the group, the like and both staged writes.
    drop(session);
    let session = Session::open(Store::open(&path, &bob_id).unwrap(), &seal_key).unwrap();
    assert_eq!(session.pair_group(&alice_peer), Some(group));
    assert!(session.likes(&alice_peer).unwrap().ours);
    assert_eq!(session.due_outbound(i64::MAX).unwrap().len(), 2);
}
