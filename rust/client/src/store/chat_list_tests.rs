mod stickers;

use super::messages::{ChatItem, VerifiedChatSender};
use super::{Conversation, Error, InboxKey, Store};
use crate::attachment::Attachment;
use crate::chat::{AttachmentMessage, Body};
use dyapp_identity::Identity;
use dyapp_messaging::hlc::Timestamp;

fn deliver(store: &mut Store, envelope: u8, group: &[u8], author: &str, body: &Body) {
    let inbox = InboxKey {
        mailbox: [1; 32],
        id: [envelope; 16],
    };
    assert!(store
        .stage_chat_inbound(
            b"mls",
            &inbox,
            VerifiedChatSender {
                group,
                author,
                device: &[6; 32],
            },
            body,
            1_000,
        )
        .unwrap());
}

fn text(id: u8, time: Option<u64>) -> Body {
    Body::Text {
        id: [id; 16],
        hlc: time.map(|ms| Timestamp::new(ms, 0).into()),
        text: vec![id],
    }
}

fn photo(id: u8, time: u64) -> Body {
    Body::Attachment(AttachmentMessage {
        id: [id; 16],
        hlc: Timestamp::new(time, 0).into(),
        attachment: Attachment {
            key: [3; 32],
            plaintext_sha256: [4; 32],
            blob_sha256: [5; 32],
            release_secret: [6; 32],
            size: 12,
            preview: vec![7, 8],
            mime: "image/jpeg".into(),
            width: 20,
            height: 10,
        },
    })
}

#[test]
fn mixed_history_and_rows_follow_original_time_and_count_each_item_once() {
    let path = super::tests::temp_db_path();
    let owner = Identity::generate();
    let (a, b, c) = (
        Identity::generate().peer_id(),
        Identity::generate().peer_id(),
        Identity::generate().peer_id(),
    );
    let (group_a, group_b, group_c) = ([3], [4], [5]);
    let mut store = Store::open(&path, &owner).unwrap();
    super::tests::catalog_group(&mut store, &group_a, &a);
    super::tests::catalog_group(&mut store, &group_b, &b);
    super::tests::catalog_group(&mut store, &group_c, &c);
    deliver(&mut store, 1, &group_a, &a, &text(1, None));
    // The reply opens the chat; a second text of a pending request would be dropped.
    let own = owner.peer_id();
    deliver(&mut store, 9, &group_a, &own, &text(8, Some(150)));
    deliver(&mut store, 2, &group_a, &a, &text(2, Some(300)));
    deliver(&mut store, 3, &group_b, &b, &text(3, Some(400)));
    deliver(&mut store, 4, &group_c, &c, &text(4, Some(100)));
    for (group, author, id, time) in [
        (&group_a, &a, 5, 200),
        (&group_a, &a, 6, 500),
        (&group_c, &c, 7, 50),
    ] {
        let sender = VerifiedChatSender {
            group,
            author,
            device: &[6; 32],
        };
        assert!(store
            .record_chat_attachment(sender, &photo(id, time), true)
            .unwrap());
    }
    drop(store);

    let store = Store::open(&path, &owner).unwrap();
    let order: Vec<u8> = store
        .current_chat_items(&a)
        .unwrap()
        .iter()
        .map(|item| match item {
            ChatItem::Text(message) => message.key.id[0],
            ChatItem::Attachment(attachment) => attachment.key.id[0],
            ChatItem::Sticker(sticker) => sticker.key.id[0],
        })
        .collect();
    assert_eq!(order, [1, 8, 5, 2, 6]);

    let rows = store.chat_rows().unwrap();
    let peers: Vec<&str> = rows.iter().map(|row| row.peer_id.as_str()).collect();
    assert_eq!(peers, [a.as_str(), b.as_str(), c.as_str()]);
    assert_eq!(rows[0].last_attachment.as_ref().unwrap().key.id, [6; 16]);
    assert_eq!(rows[0].unread_count, 4);
    assert!(rows[1].last_attachment.is_none());
    assert!(
        rows[2].last_attachment.is_none(),
        "an older photo is no preview"
    );
    assert_eq!(rows[2].last_message.as_ref().unwrap().key.id, [4; 16]);
    assert_eq!(rows[2].unread_count, 2);
}

#[test]
fn chats_rows_keep_state_latest_time_and_unread_across_clear_and_restart() {
    let path = super::tests::temp_db_path();
    let owner = Identity::generate();
    let own = owner.peer_id();
    let a = Identity::generate().peer_id();
    let b = Identity::generate().peer_id();
    let (a1, a2, b1) = ([3], [4], [5]);
    let mut store = Store::open(&path, &owner).unwrap();
    for (group, peer) in [(&a1[..], &a), (&a2[..], &a), (&b1[..], &b)] {
        super::tests::catalog_group(&mut store, group, peer);
    }
    deliver(&mut store, 1, &a1, &a, &text(1, None));
    deliver(&mut store, 2, &a1, &own, &text(2, None));
    deliver(&mut store, 3, &b1, &b, &text(3, Some(300)));
    deliver(&mut store, 4, &a2, &a, &text(4, Some(200)));
    let rows = store.chat_rows().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].peer_id, b);
    assert_eq!(rows[0].state, Conversation::RequestReceived([3; 16]));
    assert_eq!(rows[0].unread_count, 1);
    assert_eq!(
        rows[0].last_message.as_ref().unwrap().original_hlc,
        Some(Timestamp::new(300, 0))
    );
    assert_eq!(rows[1].peer_id, a);
    assert_eq!(rows[1].state, Conversation::Open);
    assert_eq!(rows[1].unread_count, 2);
    assert_eq!(rows[1].last_message.as_ref().unwrap().key.id, [4; 16]);

    let edit = Body::Edit {
        target: [4; 16],
        operation: [7; 16],
        hlc: Timestamp::new(500, 0).into(),
        text: b"edited".to_vec(),
    };
    let delete = Body::Delete {
        target: [3; 16],
        operation: [8; 16],
        hlc: Timestamp::new(600, 0).into(),
    };
    deliver(&mut store, 5, &a2, &a, &edit);
    deliver(&mut store, 6, &b1, &b, &delete);
    let rows = store.chat_rows().unwrap();
    assert!(rows[0].last_message.as_ref().unwrap().current.deleted);
    assert_eq!(
        rows[0].last_message.as_ref().unwrap().original_hlc,
        Some(Timestamp::new(300, 0))
    );
    assert_eq!(
        rows[1]
            .last_message
            .as_ref()
            .unwrap()
            .current
            .text
            .as_deref(),
        Some(&b"edited"[..])
    );
    assert_eq!(
        rows[1].last_message.as_ref().unwrap().original_hlc,
        Some(Timestamp::new(200, 0))
    );

    assert_eq!(store.mark_chat_read(&a).unwrap(), 2);
    deliver(&mut store, 7, &a2, &a, &text(9, Some(100)));
    let rows = store.chat_rows().unwrap();
    assert_eq!(rows[1].unread_count, 1);
    assert_eq!(rows[1].last_message.as_ref().unwrap().key.id, [4; 16]);
    assert_eq!(store.clear_chat_pair(&b).unwrap(), 1);
    drop(store);

    let store = Store::open(&path, &owner).unwrap();
    let rows = store.chat_rows().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].peer_id, a);
    assert_eq!(rows[0].unread_count, 1);
    assert_eq!(
        rows[0].last_message.as_ref().unwrap().original_hlc,
        Some(Timestamp::new(200, 0))
    );
    assert_eq!(rows[1].peer_id, b);
    assert_eq!(rows[1].state, Conversation::RequestReceived([3; 16]));
    assert_eq!(rows[1].unread_count, 0);
    assert!(rows[1].last_message.is_none());
    assert!(matches!(
        Store::open(&path, &Identity::generate()),
        Err(Error::WrongIdentity)
    ));
    let separate = Store::open(&super::tests::temp_db_path(), &Identity::generate()).unwrap();
    assert!(separate.chat_rows().unwrap().is_empty());
}
