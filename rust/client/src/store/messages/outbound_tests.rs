use super::{Body, DeliveryState, Error, MessageKey, Store, VerifiedChatSender};
use crate::attachment::Attachment;
use crate::chat::AttachmentMessage;
use crate::{prepare_put, PreparedEnvelope};
use dyapp_identity::Identity;
use rusqlite::params;

fn delivery_state(store: &Store, key: &MessageKey) -> i64 {
    store
        .db
        .query_row(
            "SELECT `delivery_state` FROM `chat_messages`
         WHERE `group_id` = ?1 AND `author` = ?2 AND `message_id` = ?3",
            params![&key.group, &key.author, key.id.as_slice()],
            |row| row.get(0),
        )
        .unwrap()
}

fn assert_staged(store: &Store, key: &MessageKey, write: &PreparedEnvelope, generation: [u8; 16]) {
    let link: (Vec<u8>, Vec<u8>, Vec<u8>) = store
        .db
        .query_row(
            "SELECT `chat_group`, `chat_message_id`, `chat_generation_id` FROM `outbox`
         WHERE `mailbox` = ?1 AND `envelope_id` = ?2",
            params![write.mailbox().as_slice(), write.id().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        link,
        (key.group.clone(), key.id.to_vec(), generation.to_vec())
    );
    let projection: (Vec<u8>, i64) = store
        .db
        .query_row(
            "SELECT `delivery_generation`, `delivery_state` FROM `chat_messages`
         WHERE `group_id` = ?1 AND `author` = ?2 AND `message_id` = ?3",
            params![&key.group, &key.author, key.id.as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(projection, (generation.to_vec(), 0));
}

#[test]
fn own_chat_projection_mls_and_exact_writes_share_one_commit() {
    let mut random = [0; 16];
    getrandom::fill(&mut random).unwrap();
    let path = std::env::temp_dir().join(format!(
        "dyapp-chat-outbound-{:032x}.db",
        u128::from_be_bytes(random)
    ));
    let owner = Identity::try_generate().unwrap();
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
    let write1 = prepare_put(&[7; 32], vec![1]).unwrap();
    let write2 = prepare_put(&[7; 32], vec![2]).unwrap();
    let write3 = prepare_put(&[7; 32], vec![3]).unwrap();
    let mut store = Store::open(&path, &owner).unwrap();
    let peer = Identity::try_generate().unwrap().peer_id();
    let (edit, text_hlc, delete_id, delete_hlc) =
        stage_outbound_history(&mut store, sender, &key, &peer, (&write1, &write2, &write3));
    drop(store);

    let mut store = Store::open(&path, &owner).unwrap();
    assert_restart_projection(
        &store,
        &key,
        &peer,
        (&write1, &write2, &write3),
        delete_id,
        text_hlc,
        delete_hlc,
    );
    assert_generation_delivery(&mut store, &key, &peer, &write1, &write3);
    drop(store);
    assert_final_restart(&path, &owner, sender, &key, &edit);
}

fn stage_outbound_history(
    store: &mut Store,
    sender: VerifiedChatSender,
    key: &MessageKey,
    peer: &str,
    writes: (&PreparedEnvelope, &PreparedEnvelope, &PreparedEnvelope),
) -> (Body, u64, [u8; 16], u64) {
    let (original_write, edit_write, delete_write) = writes;
    crate::store::tests::catalog_group(store, sender.group, peer);
    let text_hlc = store.draft_chat_text(1_000).unwrap();
    let text = Body::Text {
        id: key.id,
        hlc: Some(text_hlc),
        text: b"private original".to_vec(),
    };
    stage_original_text(
        store,
        sender,
        key,
        peer,
        (original_write, edit_write),
        &text,
        text_hlc,
    );
    assert_invalid_forward(store, sender, key, edit_write, text_hlc);
    assert_later_conflict(store, sender, key, edit_write, text_hlc);
    let (edit, delete_id, delete_hlc) =
        stage_edit_and_delete(store, sender, key, edit_write, delete_write);
    (edit, text_hlc, delete_id, delete_hlc)
}

fn stage_original_text(
    store: &mut Store,
    sender: VerifiedChatSender,
    key: &MessageKey,
    peer: &str,
    writes: (&PreparedEnvelope, &PreparedEnvelope),
    text: &Body,
    text_hlc: u64,
) {
    let (original_write, conflicting_write) = writes;
    store
        .stage_chat_outbound(
            b"mls-1",
            std::slice::from_ref(original_write),
            sender,
            text,
            1_000,
        )
        .unwrap();
    assert_staged(store, key, original_write, key.id);
    let original = store.current_chat_messages(peer).unwrap();
    assert_eq!(
        original[0].current.text.as_deref(),
        Some(&b"private original"[..])
    );
    assert_eq!(original[0].original_hlc.map(u64::from), Some(text_hlc));
    assert!(store.draft_chat_text(1_000).unwrap() > text_hlc);
    let conflicting = Body::Text {
        id: key.id,
        hlc: Some(text_hlc),
        text: b"other".to_vec(),
    };
    assert!(matches!(
        store.stage_chat_outbound(
            b"mls-bad",
            std::slice::from_ref(conflicting_write),
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
}

fn assert_invalid_forward(
    store: &mut Store,
    sender: VerifiedChatSender,
    key: &MessageKey,
    write: &PreparedEnvelope,
    text_hlc: u64,
) {
    let invalid_forward = Body::Forwarded(Box::new(Body::Delete {
        target: key.id,
        operation: [8; 16],
        hlc: text_hlc,
    }));
    assert!(matches!(
        store.stage_chat_outbound(
            b"mls-invalid-forward",
            std::slice::from_ref(write),
            sender,
            &invalid_forward,
            1_000
        ),
        Err(Error::InboxConflict)
    ));
    assert_eq!(store.due_outbound(0).unwrap().len(), 1);
}

fn assert_later_conflict(
    store: &mut Store,
    sender: VerifiedChatSender,
    key: &MessageKey,
    write: &PreparedEnvelope,
    text_hlc: u64,
) {
    let next_hlc = store.draft_chat_text(1_000).unwrap();
    assert!(next_hlc > text_hlc);
    let conflicting_later = Body::Text {
        id: key.id,
        hlc: Some(next_hlc),
        text: b"other".to_vec(),
    };
    assert!(matches!(
        store.stage_chat_outbound(
            b"mls-bad",
            std::slice::from_ref(write),
            sender,
            &conflicting_later,
            1_000,
        ),
        Err(Error::InboxConflict)
    ));
    assert_eq!(store.draft_chat_text(1_000).unwrap(), next_hlc);
    assert_eq!(store.due_outbound(0).unwrap().len(), 1);
}

fn stage_edit_and_delete(
    store: &mut Store,
    sender: VerifiedChatSender,
    key: &MessageKey,
    write2: &PreparedEnvelope,
    write3: &PreparedEnvelope,
) -> (Body, [u8; 16], u64) {
    let (edit_id, edit_hlc) = store.draft_chat_operation(1_000).unwrap();
    let edit = Body::Edit {
        target: key.id,
        operation: edit_id,
        hlc: edit_hlc,
        text: b"private replacement".to_vec(),
    };
    store
        .stage_chat_outbound(b"mls-2", std::slice::from_ref(write2), sender, &edit, 1_000)
        .unwrap();
    assert_staged(store, key, write2, edit_id);
    assert_eq!(
        store.chat_message(key).unwrap().unwrap().text.as_deref(),
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
            std::slice::from_ref(write3),
            sender,
            &delete,
            1_000,
        )
        .unwrap();
    assert_staged(store, key, write3, delete_id);
    (edit, delete_id, delete_hlc)
}

fn assert_restart_projection(
    store: &Store,
    key: &MessageKey,
    peer: &str,
    writes: (&PreparedEnvelope, &PreparedEnvelope, &PreparedEnvelope),
    delete_id: [u8; 16],
    text_hlc: u64,
    delete_hlc: u64,
) {
    let (original_write, edit_write, delete_write) = writes;
    assert_staged(store, key, delete_write, delete_id);
    let current = store.chat_message(key).unwrap().unwrap();
    assert!(current.edited && current.deleted && current.text.is_none());
    let ordered = store.current_chat_messages(peer).unwrap();
    assert_eq!(ordered[0].original_hlc.map(u64::from), Some(text_hlc));
    assert!(store.draft_chat_text(1_000).unwrap() > delete_hlc);
    assert_eq!(
        store.sealed_device().unwrap().as_deref(),
        Some(&b"mls-3"[..])
    );
    let due = store.due_outbound(0).unwrap();
    assert_eq!(due.len(), 3);
    for write in [original_write, edit_write, delete_write] {
        assert!(due.iter().any(|due| due.as_bytes() == write.as_bytes()));
    }
}

fn assert_generation_delivery(
    store: &mut Store,
    key: &MessageKey,
    peer: &str,
    write1: &PreparedEnvelope,
    write3: &PreparedEnvelope,
) {
    // A node taking the superseded original cannot mark the deleted generation accepted.
    store.accepted(write1, 2_000, "node").unwrap();
    assert_eq!(delivery_state(store, key), 0);
    store.accepted(write3, 2_000, "node").unwrap();
    assert_eq!(delivery_state(store, key), 1);
    assert_eq!(
        store.current_chat_messages(peer).unwrap()[0].delivery,
        Some(DeliveryState::NodeAccepted)
    );
    store.receipted(write1.mailbox(), &[*write1.id()]).unwrap();
    assert_eq!(delivery_state(store, key), 1);
    store.receipted(write3.mailbox(), &[*write3.id()]).unwrap();
    assert_eq!(delivery_state(store, key), 2);
}

fn assert_final_restart(
    path: &std::path::Path,
    owner: &Identity,
    sender: VerifiedChatSender,
    key: &MessageKey,
    edit: &Body,
) {
    let mut store = Store::open(path, owner).unwrap();
    assert_eq!(delivery_state(&store, key), 2);
    assert_eq!(
        store.chat_rows().unwrap()[0]
            .last_message
            .as_ref()
            .unwrap()
            .delivery,
        Some(DeliveryState::DeviceReceived)
    );
    assert_failed_edit_rollback(&mut store, sender, edit);
}

fn assert_failed_edit_rollback(store: &mut Store, sender: VerifiedChatSender, edit: &Body) {
    let before_failure = store.draft_chat_operation(1_000).unwrap().1;
    assert!(matches!(
        store.stage_chat_outbound(b"mls-bad", &[], sender, edit, 1_000),
        Err(Error::InboxConflict)
    ));
    assert_eq!(store.draft_chat_operation(1_000).unwrap().1, before_failure);
    assert_eq!(
        store.sealed_device().unwrap().as_deref(),
        Some(&b"mls-3"[..])
    );
}

#[test]
fn attachment_descriptor_clock_and_delivery_survive_restart() {
    let path = crate::store::tests::temp_db_path();
    let owner = Identity::try_generate().unwrap();
    let author = owner.peer_id();
    let peer = Identity::try_generate().unwrap().peer_id();
    let group = [3];
    let device = [4; 32];
    let sender = VerifiedChatSender {
        group: &group,
        author: &author,
        device: &device,
    };
    let mut store = Store::open(&path, &owner).unwrap();
    let write = prepare_put(&[7; 32], vec![1]).unwrap();
    let hlc = store.draft_chat_text(1_000).unwrap();
    let body = attachment_body(hlc);
    stage_attachment(&mut store, sender, &peer, &write, &body);
    assert!(store.draft_chat_text(1_000).unwrap() > hlc);
    assert_attachment_outbox(&store, &peer, &write);
    let next_hlc = store.draft_chat_text(1_000).unwrap();
    let conflict = Body::Attachment(AttachmentMessage {
        id: [5; 16],
        hlc: next_hlc,
        attachment: body.attachment_message().unwrap().attachment.clone(),
    });
    assert!(matches!(
        store.stage_chat_outbound(b"mls-wrong", &[], sender, &conflict, 1_000),
        Err(Error::InboxConflict)
    ));
    assert_eq!(store.draft_chat_text(1_000).unwrap(), next_hlc);
    store.accepted(&write, 2_000, "node").unwrap();
    assert_eq!(
        store.current_chat_attachments(&peer).unwrap()[0].delivery,
        Some(DeliveryState::NodeAccepted)
    );
    store.receipted(write.mailbox(), &[*write.id()]).unwrap();
    assert_eq!(
        store.current_chat_attachments(&peer).unwrap()[0].delivery,
        Some(DeliveryState::DeviceReceived)
    );
    drop(store);
    let store = Store::open(&path, &owner).unwrap();
    assert!(store.draft_chat_text(1_000).unwrap() > hlc);
    assert_eq!(
        store.current_chat_attachments(&peer).unwrap()[0].delivery,
        Some(DeliveryState::DeviceReceived)
    );
    assert_eq!(
        store.sealed_device().unwrap().as_deref(),
        Some(&b"mls-attachment"[..])
    );
}

fn attachment_body(hlc: u64) -> Body {
    Body::Attachment(AttachmentMessage {
        id: [5; 16],
        hlc,
        attachment: Attachment {
            key: [1; 32],
            plaintext_sha256: [2; 32],
            blob_sha256: [3; 32],
            release_secret: [4; 32],
            size: 12,
            preview: vec![5],
            mime: "image/jpeg".into(),
            width: 20,
            height: 10,
        },
    })
}

fn stage_attachment(
    store: &mut Store,
    sender: VerifiedChatSender,
    peer: &str,
    write: &PreparedEnvelope,
    body: &Body,
) {
    crate::store::tests::catalog_group(store, sender.group, peer);
    assert!(matches!(
        store.stage_chat_outbound(
            b"no-request",
            std::slice::from_ref(write),
            sender,
            body,
            1_000
        ),
        Err(Error::RequestPending)
    ));
    assert!(store.due_outbound(0).unwrap().is_empty());
    store
        .db
        .execute(
            "INSERT INTO `conversations` (`peer_id`, `state`, `request_id`) VALUES (?1, 3, NULL)",
            [peer],
        )
        .unwrap();
    store
        .stage_chat_outbound(
            b"mls-attachment",
            std::slice::from_ref(write),
            sender,
            body,
            1_000,
        )
        .unwrap();
}

fn assert_attachment_outbox(store: &Store, peer: &str, write: &PreparedEnvelope) {
    let history = store.current_chat_attachments(peer).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].key.id, [5; 16]);
    assert_eq!(history[0].delivery, Some(DeliveryState::Pending));
    let link: (Vec<u8>, Vec<u8>) = store
        .db
        .query_row(
            "SELECT `chat_message_id`, `chat_generation_id` FROM `outbox` WHERE `envelope_id` = ?1",
            [write.id().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(link, (vec![5; 16], vec![5; 16]));
    assert_eq!(
        store.due_outbound(0).unwrap()[0].as_bytes(),
        write.as_bytes()
    );
}

#[test]
fn received_operations_and_interactions_advance_chat_drafts_but_failure_does_not() {
    let path = super::super::tests::temp_db_path();
    let owner = Identity::try_generate().unwrap();
    let peer = Identity::try_generate().unwrap();
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
