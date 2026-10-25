use super::{Body, Error, MessageKey, Store, VerifiedChatSender};
use crate::prepare_put;
use dyapp_identity::Identity;

#[test]
fn own_chat_projection_mls_and_exact_writes_share_one_commit() {
    let mut random = [0; 16];
    getrandom::fill(&mut random).unwrap();
    let path = std::env::temp_dir().join(format!(
        "dyapp-chat-outbound-{:032x}.db",
        u128::from_be_bytes(random)
    ));
    let owner = Identity::generate();
    let author = owner.peer_id();
    let group = [3];
    let device = [4; 32];
    let sender = VerifiedChatSender {
        group: &group,
        author: &author,
        device: &device,
    };
    let key = MessageKey {
        group: group.to_vec(),
        author: author.clone(),
        id: [5; 16],
    };
    let text = Body::Text {
        id: key.id,
        text: b"private original".to_vec(),
    };
    let write1 = prepare_put(&[7; 32], vec![1]).unwrap();
    let write2 = prepare_put(&[7; 32], vec![2]).unwrap();
    let write3 = prepare_put(&[7; 32], vec![3]).unwrap();
    let mut store = Store::open(&path, &owner).unwrap();

    store
        .stage_chat_outbound(
            b"mls-1",
            std::slice::from_ref(&write1),
            sender,
            &text,
            1_000,
        )
        .unwrap();
    assert_eq!(
        store.chat_message(&key).unwrap().unwrap().text.as_deref(),
        Some(&b"private original"[..])
    );
    let conflicting = Body::Text {
        id: key.id,
        text: b"other".to_vec(),
    };
    assert!(matches!(
        store.stage_chat_outbound(
            b"mls-bad",
            std::slice::from_ref(&write2),
            sender,
            &conflicting,
            1_000
        ),
        Err(Error::InboxConflict)
    ));
    assert_eq!(
        store.sealed_device().unwrap().as_deref(),
        Some(&b"mls-1"[..])
    );
    assert_eq!(store.due_outbound(0).unwrap().len(), 1);

    let (edit_id, edit_hlc) = store.draft_chat_operation(1_000).unwrap();
    let edit = Body::Edit {
        target: key.id,
        operation: edit_id,
        hlc: edit_hlc,
        text: b"private replacement".to_vec(),
    };
    store
        .stage_chat_outbound(
            b"mls-2",
            std::slice::from_ref(&write2),
            sender,
            &edit,
            1_000,
        )
        .unwrap();
    assert_eq!(
        store.chat_message(&key).unwrap().unwrap().text.as_deref(),
        Some(&b"private replacement"[..])
    );
    let (delete_id, delete_hlc) = store.draft_chat_operation(1_000).unwrap();
    assert!(delete_hlc > edit_hlc);
    let delete = Body::Delete {
        target: key.id,
        operation: delete_id,
        hlc: delete_hlc,
    };
    store
        .stage_chat_outbound(
            b"mls-3",
            std::slice::from_ref(&write3),
            sender,
            &delete,
            1_000,
        )
        .unwrap();
    drop(store);

    let mut store = Store::open(&path, &owner).unwrap();
    let current = store.chat_message(&key).unwrap().unwrap();
    assert!(current.edited && current.deleted && current.text.is_none());
    assert_eq!(
        store.sealed_device().unwrap().as_deref(),
        Some(&b"mls-3"[..])
    );
    let due = store.due_outbound(0).unwrap();
    assert_eq!(due.len(), 3);
    assert!(due
        .iter()
        .any(|write| write.as_bytes() == write1.as_bytes()));
    assert!(due
        .iter()
        .any(|write| write.as_bytes() == write2.as_bytes()));
    assert!(due
        .iter()
        .any(|write| write.as_bytes() == write3.as_bytes()));
    let before_failure = store.draft_chat_operation(1_000).unwrap().1;
    assert!(matches!(
        store.stage_chat_outbound(b"mls-bad", &[], sender, &edit, 1_000),
        Err(Error::InboxConflict)
    ));
    assert_eq!(store.draft_chat_operation(1_000).unwrap().1, before_failure);
    assert_eq!(
        store.sealed_device().unwrap().as_deref(),
        Some(&b"mls-3"[..])
    );
}

#[test]
fn received_operations_and_interactions_advance_chat_drafts_but_failure_does_not() {
    let mut random = [0; 16];
    getrandom::fill(&mut random).unwrap();
    let path = std::env::temp_dir().join(format!(
        "dyapp-chat-clock-{:032x}.db",
        u128::from_be_bytes(random)
    ));
    let owner = Identity::generate();
    let peer = Identity::generate();
    let mut store = Store::open(&path, &owner).unwrap();
    let key = MessageKey {
        group: vec![1],
        author: peer.peer_id(),
        id: [2; 16],
    };
    let received_hlc = dyapp_messaging::hlc::Timestamp::new(2_000, 0).into();
    let received = Body::Edit {
        target: key.id,
        operation: [3; 16],
        hlc: received_hlc,
        text: b"early".to_vec(),
    };
    store
        .apply_chat_operation(&key, &[4; 32], &received, 2_000)
        .unwrap();
    let (_, after_chat) = store.draft_chat_operation(2_000).unwrap();
    assert!(after_chat > received_hlc);

    let own = store
        .interact(
            &peer.peer_id(),
            crate::interactions::Kind::Like,
            &[5; 32],
            2_000,
        )
        .unwrap()
        .unwrap();
    let (_, after_interaction) = store.draft_chat_operation(2_000).unwrap();
    assert!(after_interaction > own.hlc && after_interaction > after_chat);

    let future = Body::Edit {
        target: key.id,
        operation: [6; 16],
        hlc: dyapp_messaging::hlc::Timestamp::new(100_000, 0).into(),
        text: b"future".to_vec(),
    };
    assert!(matches!(
        store.apply_chat_operation(&key, &[4; 32], &future, 2_000),
        Err(Error::ClockSkew(_))
    ));
    assert_eq!(
        store.draft_chat_operation(2_000).unwrap().1,
        after_interaction
    );
}
