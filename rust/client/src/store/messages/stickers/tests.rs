use super::*;
mod sharing;
use crate::chat::{AttachmentMessage, Body};
use crate::store::tests::{catalog_group, temp_db_path};
use dyapp_identity::Identity;

fn sender(peer: &str) -> VerifiedChatSender<'_> {
    VerifiedChatSender {
        group: &[9],
        author: peer,
        device: &[8; 32],
    }
}

fn sticker(id: u8, attached: bool) -> Sticker {
    Sticker {
        id: [id; 16],
        hlc: u64::from(id),
        pack_id: [2; 32],
        sha256: [3; 32],
        emoji: "😺".into(),
        pack_name: "Cats".into(),
        attachment: attached.then_some(crate::attachment::Attachment {
            key: [4; 32],
            plaintext_sha256: [3; 32],
            blob_sha256: [5; 32],
            release_secret: [0; 32],
            size: 20,
            preview: vec![],
            mime: "image/webp".into(),
            width: 1,
            height: 1,
        }),
    }
}

#[test]
fn fallback_survives_restart_read_and_clear_without_resurrection() {
    let path = temp_db_path();
    let owner = Identity::generate();
    let peer = Identity::generate().peer_id();
    let mut store = Store::open(&path, &owner).unwrap();
    catalog_group(&mut store, &[9], &peer);
    let mut message = sticker(1, false);
    message.hlc = (1_234 << 16) | 17;
    assert!(store
        .record_chat_sticker(sender(&peer), &message, true)
        .unwrap());
    assert!(!store
        .record_chat_sticker(sender(&peer), &message, false)
        .unwrap());
    assert!(store.current_chat_stickers(&peer).unwrap()[0].unread);
    assert_eq!(store.mark_chat_read(&peer).unwrap(), 1);
    assert!(!store
        .record_chat_sticker(sender(&peer), &message, true)
        .unwrap());
    drop(store);
    let mut store = Store::open(&path, &owner).unwrap();
    let items = store.current_chat_stickers(&peer).unwrap();
    assert_eq!(items.len(), 1);
    assert!(items[0].sticker == message);
    assert_eq!(items[0].sent_at_ms(), 1_234);
    assert!(!items[0].unread);
    assert!(store.current_chat_attachments(&peer).unwrap().is_empty());
    assert!(store.attachment_blobs().unwrap().is_empty());
    assert_eq!(store.clear_chat_pair(&peer).unwrap(), 1);
    assert!(!store
        .record_chat_sticker(sender(&peer), &message, true)
        .unwrap());
    assert!(store.current_chat_stickers(&peer).unwrap().is_empty());
}

#[test]
fn media_ids_and_verified_context_cannot_be_reused() {
    let owner = Identity::generate();
    let peer = Identity::generate().peer_id();
    let mut store = Store::open(&temp_db_path(), &owner).unwrap();
    catalog_group(&mut store, &[9], &peer);
    let message = sticker(1, true);
    assert!(store
        .record_chat_sticker(sender(&peer), &message, true)
        .unwrap());
    let photo = Body::Attachment(AttachmentMessage {
        id: message.id,
        hlc: message.hlc,
        attachment: message.attachment.clone().unwrap(),
    });
    assert!(store
        .record_chat_attachment(sender(&peer), &photo, true)
        .is_err());
    assert!(store
        .record_chat_text(
            &MessageKey {
                group: vec![9],
                author: peer.clone(),
                id: message.id,
            },
            b"text"
        )
        .is_err());
    for context in [
        VerifiedChatSender {
            group: &[9],
            author: "stranger",
            device: &[8; 32],
        },
        VerifiedChatSender {
            group: &[10],
            author: &peer,
            device: &[8; 32],
        },
        VerifiedChatSender {
            group: &[9],
            author: &peer,
            device: &[8; 31],
        },
        VerifiedChatSender {
            group: &[],
            author: &peer,
            device: &[8; 32],
        },
    ] {
        assert!(store
            .record_chat_sticker(context, &sticker(2, false), true)
            .is_err());
    }
    assert!(store
        .record_chat_sticker(sender(&owner.peer_id()), &sticker(2, false), true)
        .is_err());
    let mut changed = message.clone();
    changed.pack_name = "Other".into();
    assert!(store
        .record_chat_sticker(sender(&peer), &changed, true)
        .is_err());
    let mut photo_message = photo.attachment_message().unwrap().clone();
    photo_message.id = [2; 16];
    store
        .record_chat_attachment(sender(&peer), &Body::Attachment(photo_message), true)
        .unwrap();
    assert!(store
        .record_chat_sticker(sender(&peer), &sticker(2, false), true)
        .is_err());
}

#[test]
fn shared_media_references_follow_history_and_hash_binding() {
    let owner = Identity::generate();
    let peer = Identity::generate().peer_id();
    let mut store = Store::open(&temp_db_path(), &owner).unwrap();
    catalog_group(&mut store, &[9], &peer);
    catalog_group(&mut store, &[10], &peer);
    let message = sticker(1, true);
    store
        .record_chat_sticker(sender(&peer), &message, true)
        .unwrap();
    let mut other = sticker(2, true);
    other.sha256 = [7; 32]; // The descriptor does not authorize this sticker's bytes.
    other.attachment.as_mut().unwrap().blob_sha256 = [6; 32];
    store
        .record_chat_sticker(sender(&peer), &other, true)
        .unwrap();
    assert_eq!(store.attachment_blobs().unwrap().len(), 1);
    let context = VerifiedChatSender {
        group: &[10],
        ..sender(&peer)
    };
    store.record_chat_sticker(context, &message, true).unwrap();
    assert_eq!(store.clear_chat(&[9]).unwrap(), 2);
    assert_eq!(store.attachment_blobs().unwrap().len(), 1);
    assert_eq!(store.clear_chat(&[10]).unwrap(), 1);
    assert!(store.attachment_blobs().unwrap().is_empty());
}

#[test]
fn merged_history_keeps_fallback_between_legacy_text_and_later_photo() {
    use super::super::ChatItem;
    let owner = Identity::generate();
    let peer = Identity::generate().peer_id();
    let mut store = Store::open(&temp_db_path(), &owner).unwrap();
    catalog_group(&mut store, &[9], &peer);
    store
        .record_chat_text(
            &MessageKey {
                group: vec![9],
                author: peer.clone(),
                id: [9; 16],
            },
            b"legacy",
        )
        .unwrap();
    let message = sticker(2, false);
    store
        .record_chat_sticker(sender(&peer), &message, true)
        .unwrap();
    let photo = sticker(3, true);
    store
        .record_chat_attachment(
            sender(&peer),
            &Body::Attachment(AttachmentMessage {
                id: photo.id,
                hlc: photo.hlc,
                attachment: photo.attachment.unwrap(),
            }),
            true,
        )
        .unwrap();
    let items = store.current_chat_items(&peer).unwrap();
    assert_eq!(items.len(), 3);
    assert!(matches!(&items[0], ChatItem::Text(text) if text.key.id == [9; 16]));
    assert!(matches!(&items[1], ChatItem::Sticker(stored) if stored.sticker == message));
    assert!(matches!(&items[2], ChatItem::Attachment(photo) if photo.key.id == [3; 16]));
}

#[test]
fn corrupt_persisted_context_and_payload_fail_closed() {
    for sql in [
        "UPDATE `chat_attachments` SET `original_hlc` = 99",
        "UPDATE `chat_attachments` SET `descriptor` = x'ff'",
        "UPDATE `chat_attachments` SET `forwarded` = 1",
        "UPDATE `chat_attachments` SET `delivery_state` = 0, `delivery_generation` = `message_id`",
        "UPDATE `chat_attachments` SET `author` = 'stranger'",
    ] {
        let owner = Identity::generate();
        let peer = Identity::generate().peer_id();
        let mut store = Store::open(&temp_db_path(), &owner).unwrap();
        catalog_group(&mut store, &[9], &peer);
        store
            .record_chat_sticker(sender(&peer), &sticker(1, false), true)
            .unwrap();
        store.db.execute(sql, []).unwrap();
        assert!(store.current_chat_stickers(&peer).is_err(), "{sql}");
    }
}
