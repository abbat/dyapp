use super::*;
use crate::store::messages::{CurrentMessage, MessageKey};

fn message() -> ChatMessage {
    ChatMessage {
        key: MessageKey {
            group: vec![1],
            author: "sender".into(),
            id: [2; 16],
        },
        current: CurrentMessage {
            text: Some(b"current words".to_vec()),
            edited: true,
            deleted: false,
        },
        original_hlc: None,
        unread: false,
        forwarded: false,
        delivery: None,
    }
}

#[test]
fn source_selection_requires_the_full_live_key_and_uses_current_text() {
    let source = Source {
        peer: "unused",
        group: &[1],
        author: "sender",
        id: &[2; 16],
    };
    let original = message();
    for (group, author, id) in [
        (&[3][..], "sender", [2; 16]),
        (&[1][..], "other", [2; 16]),
        (&[1][..], "sender", [3; 16]),
    ] {
        assert_eq!(
            text(
                std::slice::from_ref(&original),
                Source {
                    group,
                    author,
                    ..source
                },
                id
            ),
            None
        );
    }
    assert_eq!(
        text(std::slice::from_ref(&original), source, [2; 16]),
        Some(b"current words".to_vec())
    );
    for current in [
        CurrentMessage {
            text: None,
            edited: false,
            deleted: true,
        },
        CurrentMessage {
            text: Some(vec![]),
            edited: false,
            deleted: false,
        },
        CurrentMessage {
            text: Some(vec![1; MAX_TEXT_BYTES + 1]),
            edited: false,
            deleted: false,
        },
    ] {
        let mut invalid = message();
        invalid.current = current;
        assert_eq!(text(&[invalid], source, [2; 16]), None);
    }
}

pub(crate) fn open_chat(store: &mut Store, peer: &str) {
    let owner = store.owner_peer_id().to_owned();
    let like = store
        .interact(peer, crate::interactions::Kind::Like, &[1], 1_000)
        .unwrap()
        .unwrap();
    store.apply_interaction(peer, &owner, &[2], like).unwrap();
}

struct Fixture {
    store: Store,
    source: String,
    target: String,
    key: MessageKey,
}
impl Fixture {
    fn new() -> Self {
        let owner = dyapp_identity::Identity::generate();
        let mut store = Store::open(&crate::store::tests::temp_db_path(), &owner).unwrap();
        let source = "ab".repeat(32);
        let target = "cd".repeat(32);
        crate::store::tests::catalog_group(&mut store, &[1], &source);
        let key = MessageKey {
            group: vec![1],
            author: source.clone(),
            id: [9; 16],
        };
        store.record_chat_text(&key, b"current").unwrap();
        open_chat(&mut store, &source);
        open_chat(&mut store, &target);
        Self {
            store,
            source,
            target,
            key,
        }
    }

    fn request(&self) -> Source<'_> {
        Source {
            peer: &self.source,
            group: &self.key.group,
            author: &self.key.author,
            id: &self.key.id,
        }
    }
}

#[test]
fn stored_preflight_rejects_same_own_and_unknown_targets_without_writes() {
    let f = Fixture::new();
    assert_eq!(
        prepare(&f.store, f.request(), &f.target).unwrap(),
        b"current"
    );
    assert!(matches!(
        prepare(&f.store, f.request(), &f.source),
        Err(Error::InvalidTarget)
    ));
    assert!(matches!(
        prepare(&f.store, f.request(), f.store.owner_peer_id()),
        Err(Error::InvalidTarget)
    ));
    assert!(matches!(
        prepare(&f.store, f.request(), &"ef".repeat(32)),
        Err(Error::InvalidTarget)
    ));
    assert!(f.store.due_outbound(0).unwrap().is_empty());
}

#[test]
fn stored_preflight_rejects_invalid_source_id_without_writes() {
    let f = Fixture::new();
    assert!(matches!(
        prepare(
            &f.store,
            Source {
                id: &[1],
                ..f.request()
            },
            &f.target
        ),
        Err(Error::UnknownMessage)
    ));
    assert!(f.store.due_outbound(0).unwrap().is_empty());
}

#[test]
fn stored_preflight_rejects_blocked_source_or_target_without_writes() {
    let f = Fixture::new();
    for peer in [&f.source, &f.target] {
        f.store.set_blocked(peer, true).unwrap();
        assert!(matches!(
            prepare(&f.store, f.request(), &f.target),
            Err(Error::Blocked)
        ));
        f.store.set_blocked(peer, false).unwrap();
    }
    assert!(f.store.due_outbound(0).unwrap().is_empty());
}

#[test]
fn stored_preflight_rejects_closed_target_without_writes() {
    let mut f = Fixture::new();
    f.store
        .interact(&f.target, crate::interactions::Kind::Unlike, &[3], 2_000)
        .unwrap();
    assert!(matches!(
        prepare(&f.store, f.request(), &f.target),
        Err(Error::InvalidTarget)
    ));
    assert!(f.store.due_outbound(0).unwrap().is_empty());
}
