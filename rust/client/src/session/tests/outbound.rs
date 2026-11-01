use super::*;

#[test]
fn message_is_staged_before_network_and_reopens_exact_bytes() {
    let (alice_id, alice, alice_profile) = person();
    let (_, bob, bob_profile) = person();
    let (group, _) = alice
        .create_group(
            &[bob.key_package(false).unwrap()],
            &[alice_profile, bob_profile.clone()],
        )
        .unwrap();
    let group_id = group.id().to_vec();
    let seal_key = [3; 32];
    let mut random = [0; 16];
    getrandom::fill(&mut random).unwrap();
    let path = std::env::temp_dir().join(format!(
        "dyapp-send-session-{:032x}.db",
        u128::from_be_bytes(random)
    ));
    let mut store = Store::open(&path, &alice_id).unwrap();
    store
        .stage_group(&group, &bob_profile, &alice.seal(&seal_key).unwrap(), &[])
        .unwrap();
    // A match opens the chat, so the second text is not a second pending request.
    let alice_peer = alice_id.peer_id();
    let like = store
        .interact(
            &bob_profile.peer_id,
            crate::interactions::Kind::Like,
            &[1],
            1_000,
        )
        .unwrap()
        .unwrap();
    store
        .apply_interaction(&bob_profile.peer_id, &alice_peer, &[2], like)
        .unwrap();
    let mut session = Session::open(store, &seal_key).unwrap();
    let writes = session
        .stage_message(
            &group_id,
            &Body::text_with_id([1; 16], b"hello".to_vec()),
            &seal_key,
        )
        .unwrap();
    assert_eq!(writes.len(), 1);
    let (offline_id, offline_writes) = session
        .stage_text(&group_id, b"offline", &seal_key)
        .unwrap();
    assert_ne!(offline_id, [1; 16]);
    assert_eq!(offline_writes.len(), 1);
    let (forward_id, forward_writes) = session
        .stage_forwarded_text(&group_id, b"forwarded words", &seal_key)
        .unwrap();
    assert_ne!(forward_id, offline_id);
    assert_eq!(forward_writes.len(), 1);
    let sent = session
        .store
        .current_chat_messages(&bob_profile.peer_id)
        .unwrap();
    let forwarded = sent
        .iter()
        .find(|message| message.key.id == forward_id)
        .unwrap();
    assert!(forwarded.forwarded);
    assert!(forwarded.original_hlc.is_some());
    assert_eq!(
        forwarded.current.text.as_deref(),
        Some(&b"forwarded words"[..])
    );
    let original_hlc = sent
        .iter()
        .find(|message| message.key.id == offline_id)
        .unwrap()
        .original_hlc
        .unwrap();
    assert!(u64::from(original_hlc) > 0);
    assert!(session.store.draft_chat_text(crate::now() * 1000).unwrap() > u64::from(original_hlc));
    let own = MessageKey {
        group: group_id.clone(),
        author: alice_id.peer_id(),
        id: offline_id,
    };
    assert_eq!(
        session
            .store
            .chat_message(&own)
            .unwrap()
            .unwrap()
            .text
            .as_deref(),
        Some(&b"offline"[..])
    );
    let due = session.due_outbound(0).unwrap();
    assert!(due
        .iter()
        .any(|write| write.as_bytes() == writes[0].as_bytes()));
    assert!(due
        .iter()
        .any(|write| write.as_bytes() == offline_writes[0].as_bytes()));
    assert!(due
        .iter()
        .any(|write| write.as_bytes() == forward_writes[0].as_bytes()));
    drop(session);
    let mut session = Session::open(Store::open(&path, &alice_id).unwrap(), &seal_key).unwrap();
    assert!(session
        .store
        .current_chat_messages(&bob_profile.peer_id)
        .unwrap()
        .iter()
        .any(|message| message.key.id == forward_id && message.forwarded));
    assert_eq!(
        session.chat_message(&own).unwrap().unwrap().text.as_deref(),
        Some(&b"offline"[..])
    );
    let before_invalid = session.store.sealed_device().unwrap();
    assert!(matches!(
        session.stage_edit(&[9], offline_id, b"wrong group", &seal_key),
        Err(Error::UnknownMessage)
    ));
    assert!(matches!(
        session.stage_delete(&group_id, [9; 16], &seal_key),
        Err(Error::UnknownMessage)
    ));
    assert_eq!(session.store.sealed_device().unwrap(), before_invalid);
    let (edit_id, edit_writes) = session
        .stage_edit(&group_id, offline_id, b"revised", &seal_key)
        .unwrap();
    assert_ne!(edit_id, [0; 16]);
    assert_eq!(
        session.chat_message(&own).unwrap().unwrap().text.as_deref(),
        Some(&b"revised"[..])
    );
    let (delete_id, delete_writes) = session
        .stage_delete(&group_id, offline_id, &seal_key)
        .unwrap();
    assert_ne!(delete_id, edit_id);
    assert!(matches!(
        session.stage_edit(&group_id, offline_id, b"restore", &seal_key),
        Err(Error::UnknownMessage)
    ));
    let current = session.chat_message(&own).unwrap().unwrap();
    assert!(current.edited && current.deleted && current.text.is_none());
    drop(session);
    let mut session = Session::open(Store::open(&path, &alice_id).unwrap(), &seal_key).unwrap();
    assert!(session.chat_message(&own).unwrap().unwrap().deleted);
    let due = session.due_outbound(0).unwrap();
    for exact in [
        &writes[0],
        &offline_writes[0],
        &edit_writes[0],
        &delete_writes[0],
    ] {
        assert!(due.iter().any(|write| write.as_bytes() == exact.as_bytes()));
    }
    assert_eq!(session.clear_chat(&group_id).unwrap(), 2);
    assert!(session.chat_message(&own).unwrap().is_none());
    assert!(session.due_outbound(0).unwrap().is_empty());
}
