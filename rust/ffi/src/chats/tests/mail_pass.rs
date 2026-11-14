//! Real stored notice projections; metadata enters below verified receive, not a live mail transfer.
use super::offline::snapshot;
use super::sticker_exchange::Received;
use crate::{ChatMailNotice, ChatNotice};
use dyapp_client::{
    mail_notices,
    store::{messages::VerifiedChatSender, InboxKey, Store},
};
use std::path::Path;

fn store(f: &Received) -> Store {
    Store::open(
        &Path::new(&f.native.dir).join("client.db"),
        &f.native.key.identity,
    )
    .unwrap()
}
fn before(f: &Received) -> mail_notices::Waiting {
    let mut store = store(f);
    let sealed = store.sealed_device().unwrap().unwrap();
    let hlc = store.draft_chat_text(0).unwrap();
    store
        .stage_chat_inbound(
            &sealed,
            &InboxKey {
                mailbox: [1; 32],
                id: [40; 16],
            },
            VerifiedChatSender {
                group: &f.key.group,
                author: &f.peer,
                device: &[8; 32],
            },
            &dyapp_client::chat::Body::Text {
                id: [40; 16],
                hlc: Some(hlc),
                text: b"older private words".to_vec(),
            },
            dyapp_messaging::Timestamp::from(hlc).wall_ms(),
        )
        .unwrap();
    mail_notices::capture(&store).unwrap()
}
fn arrival(f: &Received) -> Store {
    let mut store = store(f);
    let mut sticker = store
        .current_chat_stickers(&f.peer)
        .unwrap()
        .remove(0)
        .sticker;
    sticker.id = [43; 16];
    sticker.hlc = store.draft_chat_text(0).unwrap();
    sticker.pack_name = "private pack metadata".into();
    store
        .record_chat_sticker(
            VerifiedChatSender {
                group: &f.key.group,
                author: &f.peer,
                device: &[8; 32],
            },
            &sticker,
            true,
        )
        .unwrap();
    store
}

#[test]
fn native_notice_uses_current_media_and_preferences_without_old_text_or_pack_metadata() {
    let f = Received::new();
    let before = before(&f);
    let store = arrival(&f);
    let retained = store.chat_rows().unwrap().remove(0).last_message.unwrap();
    assert_eq!(
        retained.current.text.as_deref(),
        Some(b"older private words".as_slice())
    );
    assert!(retained.unread);
    for (mode, expected) in [
        (
            ChatNotice::Text,
            Some(ChatMailNotice {
                peer_id: Some(f.peer.clone()),
                text: Some("😺 Стикер".into()),
            }),
        ),
        (
            ChatNotice::Name,
            Some(ChatMailNotice {
                peer_id: Some(f.peer.clone()),
                text: None,
            }),
        ),
        (
            ChatNotice::Fixed,
            Some(ChatMailNotice {
                peer_id: None,
                text: None,
            }),
        ),
        (ChatNotice::Off, None),
    ] {
        f.native.chats.set_notice(mode).unwrap();
        assert_eq!(
            super::super::mail_pass::notice_after(Some(before.clone()), &store, "Стикер"),
            expected
        );
    }
}

#[test]
fn missing_snapshot_muted_or_blocked_new_mail_stays_quiet() {
    let f = Received::new();
    f.native.chats.set_notice(ChatNotice::Text).unwrap();
    let before = before(&f);
    let store = arrival(&f);
    assert_eq!(
        super::super::mail_pass::notice_after(None, &store, "Sticker"),
        None
    );
    store.set_muted(&f.peer, true).unwrap();
    assert_eq!(
        super::super::mail_pass::notice_after(Some(before.clone()), &store, "Sticker"),
        None
    );
    store.set_muted(&f.peer, false).unwrap();
    store.set_blocked(&f.peer, true).unwrap();
    assert_eq!(
        super::super::mail_pass::notice_after(Some(before), &store, "Sticker"),
        None
    );
}

#[test]
fn foreign_identity_snapshot_never_reveals_current_sender_or_body() {
    let f = Received::new();
    let other = Received::new();
    f.native.chats.set_notice(ChatNotice::Text).unwrap();
    let before = before(&other);
    let store = arrival(&f);
    assert_eq!(
        super::super::mail_pass::notice_after(Some(before), &store, "Sticker"),
        None
    );
}

#[test]
fn receive_with_notice_keeps_existing_offline_error_and_does_not_reset_mls() {
    let f = Received::new();
    let before = snapshot(&f.native);
    assert_eq!(
        f.native
            .chats
            .receive_with_notice("Sticker".into())
            .unwrap_err()
            .to_string(),
        "notConnected"
    );
    assert_eq!(
        f.native.chats.receive().unwrap_err().to_string(),
        "notConnected"
    );
    assert_eq!(snapshot(&f.native), before);
}
