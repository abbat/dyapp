use super::offline::snapshot;
use super::sticker_exchange::Received;
use super::*;
use dyapp_client::store::messages::{MessageKey, VerifiedChatSender};
use std::path::Path;

fn store(f: &Received) -> Store {
    Store::open(
        &Path::new(&f.native.dir).join("client.db"),
        &f.native.key.identity,
    )
    .unwrap()
}

fn own_selection(f: &Received) -> (MessageKey, MessageKey) {
    let mut store = store(f);
    let own = f.native.key.peer_id();
    let sticker = store
        .current_chat_stickers(&f.peer)
        .unwrap()
        .remove(0)
        .sticker;
    // Keep the foreign logical ID to prove that an explicit foreign author cannot select this copy.
    store
        .record_chat_sticker(
            VerifiedChatSender {
                group: &f.key.group,
                author: &own,
                device: &[8; 32],
            },
            &sticker,
            false,
        )
        .unwrap();
    let text = store
        .current_chat_messages(&f.peer)
        .unwrap()
        .into_iter()
        .find(|message| message.key.author == own && !message.current.deleted)
        .unwrap()
        .key;
    (
        MessageKey {
            author: own,
            ..f.key.clone()
        },
        text,
    )
}

fn delete(f: &Received, key: &MessageKey) -> Result<ChatSubmission, CoreError> {
    f.native.chats.submit_delete_sticker(
        f.peer.clone(),
        key.group.clone(),
        key.author.clone(),
        key.id.to_vec(),
    )
}

#[test]
fn sticker_delete_is_full_key_and_kind_specific_without_writes_before_transport() {
    let f = Received::new();
    let (own, text) = own_selection(&f);
    let before = snapshot(&f.native);
    let rows = f.native.chats.chat_items(f.peer.clone()).unwrap().len();
    // A valid selection reaches transport admission; this isolated fixture has no active network.
    assert_eq!(delete(&f, &own).unwrap_err().to_string(), "notConnected");
    assert_eq!(
        f.native
            .chats
            .submit_delete_text(f.peer.clone(), own.group.clone(), own.id.to_vec())
            .unwrap_err()
            .to_string(),
        "unknownMessage"
    );
    for key in [
        f.key.clone(),
        MessageKey {
            group: vec![99],
            ..own.clone()
        },
        MessageKey {
            id: [99; 16],
            ..own.clone()
        },
        text,
    ] {
        assert_eq!(delete(&f, &key).unwrap_err().to_string(), "unknownMessage");
    }
    assert_eq!(snapshot(&f.native), before);
    assert_eq!(
        f.native.chats.chat_items(f.peer.clone()).unwrap().len(),
        rows
    );
}

#[test]
fn native_sticker_delete_preserves_own_blocked_malformed_and_clear_guards() {
    let f = Received::new();
    let (own, _) = own_selection(&f);
    assert_eq!(
        f.native
            .chats
            .submit_delete_sticker(
                f.native.key.peer_id(),
                own.group.clone(),
                own.author.clone(),
                vec![1]
            )
            .unwrap_err()
            .to_string(),
        "ownPeer"
    );
    assert_eq!(
        f.native
            .chats
            .submit_delete_sticker(
                f.peer.clone(),
                own.group.clone(),
                own.author.clone(),
                vec![1]
            )
            .unwrap_err()
            .to_string(),
        "unknownMessage"
    );
    store(&f).set_blocked(&f.peer, true).unwrap();
    assert_eq!(delete(&f, &own).unwrap_err().to_string(), "blocked");
    store(&f).set_blocked(&f.peer, false).unwrap();
    store(&f).clear_chat(&f.peer).unwrap();
    let cleared = snapshot(&f.native);
    assert_eq!(delete(&f, &own).unwrap_err().to_string(), "unknownMessage");
    assert_eq!(snapshot(&f.native), cleared);
}
