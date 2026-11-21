use super::fixture::Fixture;
use super::*;
use crate::store::messages::{ChatMessage, ChatSticker, CurrentMessage};

fn key() -> MessageKey {
    MessageKey {
        group: vec![4],
        author: "own".into(),
        id: [2; 16],
    }
}

fn sticker() -> ChatItem {
    ChatItem::Sticker(ChatSticker {
        key: key(),
        sticker: crate::chat::sticker::Sticker {
            id: [2; 16],
            hlc: 1,
            pack_id: [3; 32],
            sha256: [4; 32],
            emoji: "😺".into(),
            pack_name: "Cats".into(),
            attachment: None,
        },
        unread: false,
        delivery: None,
    })
}

#[test]
fn stickers_are_delete_only_and_require_the_exact_own_key() {
    let item = sticker();
    assert!(matches(&item, &key(), Kind::TextOrSticker));
    assert!(matches(&item, &key(), Kind::Sticker));
    assert!(!matches(&item, &key(), Kind::Text));
    for target in [
        MessageKey {
            group: vec![5],
            ..key()
        },
        MessageKey {
            id: [9; 16],
            ..key()
        },
        MessageKey {
            author: "another author".into(),
            ..key()
        },
    ] {
        assert!(!matches(&item, &target, Kind::TextOrSticker));
    }
}

#[test]
fn edit_target_is_a_live_own_text_in_the_selected_group() {
    let mut message = ChatMessage {
        key: key(),
        current: CurrentMessage {
            text: Some(b"text".to_vec()),
            edited: false,
            deleted: false,
        },
        original_hlc: None,
        unread: false,
        forwarded: false,
        delivery: None,
    };
    let item = ChatItem::Text(message.clone());
    assert!(matches(&item, &key(), Kind::Text));
    assert!(matches(&item, &key(), Kind::TextOrSticker));
    assert!(!matches(&item, &key(), Kind::Sticker));
    for target in [
        MessageKey {
            group: vec![5],
            ..key()
        },
        MessageKey {
            id: [3; 16],
            ..key()
        },
    ] {
        assert!(!matches(&item, &target, Kind::Text));
    }
    message.key.author = "peer".into();
    assert!(!matches(
        &ChatItem::Text(message.clone()),
        &key(),
        Kind::Text
    ));
    message.key = key();
    message.current.deleted = true;
    assert!(!matches(&ChatItem::Text(message), &key(), Kind::Text));
}

#[test]
fn stored_full_key_and_required_kind_do_not_rebind_foreign_colliding_ids() {
    let f = Fixture::new();
    for (key, kind) in [(&f.text, Kind::Text), (&f.sticker, Kind::Sticker)] {
        assert_eq!(f.select(key, kind).unwrap(), key.id);
        assert_eq!(f.select(key, Kind::TextOrSticker).unwrap(), key.id);
    }
    for (key, kind) in [(&f.text, Kind::Sticker), (&f.sticker, Kind::Text)] {
        assert!(matches!(f.select(key, kind), Err(Error::UnknownMessage)));
    }
    for target in [
        MessageKey {
            group: vec![10],
            ..f.sticker.clone()
        },
        MessageKey {
            id: [99; 16],
            ..f.sticker.clone()
        },
        MessageKey {
            author: f.media.peer.clone(),
            ..f.sticker.clone()
        },
    ] {
        assert!(matches!(
            f.select(&target, Kind::TextOrSticker),
            Err(Error::UnknownMessage)
        ));
    }
}

#[test]
fn blocking_own_peer_and_clear_refuse_before_any_operation_can_be_staged() {
    let mut f = Fixture::new();
    let own = f.media.store.owner_peer_id().to_owned();
    let source = Source {
        peer: &own,
        group: &f.sticker.group,
        author: &own,
        id: &[1],
    };
    assert!(matches!(
        prepare(&f.media.store, source, Kind::Sticker),
        Err(Error::OwnPeer)
    ));
    f.media.store.set_blocked(&f.media.peer, true).unwrap();
    assert!(matches!(
        f.select(&f.sticker, Kind::Sticker),
        Err(Error::Blocked)
    ));
    f.media.store.set_blocked(&f.media.peer, false).unwrap();
    let malformed = Source {
        peer: &f.media.peer,
        id: &[1],
        ..source
    };
    assert!(matches!(
        prepare(&f.media.store, malformed, Kind::Text),
        Err(Error::UnknownMessage)
    ));
    f.media.store.clear_chat(&f.media.peer).unwrap();
    for kind in [Kind::Text, Kind::Sticker, Kind::TextOrSticker] {
        assert!(matches!(
            f.select(&f.text, kind),
            Err(Error::UnknownMessage)
        ));
        assert!(matches!(
            f.select(&f.sticker, kind),
            Err(Error::UnknownMessage)
        ));
    }
}
