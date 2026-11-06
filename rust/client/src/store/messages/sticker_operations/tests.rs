use super::*;
use crate::store::messages::VerifiedChatSender;
use crate::store::tests::{catalog_group, temp_db_path};
use crate::store::Store;
use dyapp_identity::Identity;

struct Fixture {
    store: Store,
    owner: Identity,
    peer: String,
    sticker: Sticker,
    key: MessageKey,
}

fn fixture() -> Fixture {
    let owner = Identity::generate();
    let peer = Identity::generate().peer_id();
    let mut store = Store::open(&temp_db_path(), &owner).unwrap();
    catalog_group(&mut store, &[9], &peer);
    let (mut attachment, _) = crate::attachment::seal(b"sticker").unwrap();
    attachment.release_secret = [0; 32];
    let sticker = Sticker {
        id: [1; 16],
        hlc: 1 << 16,
        pack_id: [2; 32],
        sha256: attachment.plaintext_sha256,
        emoji: "😺".into(),
        pack_name: "Cats".into(),
        attachment: Some(attachment),
    };
    let key = MessageKey {
        group: vec![9],
        author: peer.clone(),
        id: sticker.id,
    };
    Fixture {
        store,
        owner,
        peer,
        sticker,
        key,
    }
}

fn action(id: u8, edit: bool) -> Body {
    let (target, operation, hlc) = ([1; 16], [id; 16], u64::from(id) << 16);
    if edit {
        Body::Edit {
            target,
            operation,
            hlc,
            text: b"replacement".to_vec(),
        }
    } else {
        Body::Delete {
            target,
            operation,
            hlc,
        }
    }
}

impl Fixture {
    fn record(&mut self) -> Result<bool, Error> {
        self.store.record_chat_sticker(
            VerifiedChatSender {
                group: &[9],
                author: &self.peer,
                device: &[8; 32],
            },
            &self.sticker,
            true,
        )
    }
    fn apply(&mut self, id: u8, edit: bool) -> Result<bool, Error> {
        self.store
            .apply_chat_operation(&self.key, &[8; 32], &action(id, edit), 10)
    }
    fn pending(&self) -> i64 {
        self.store
            .db
            .query_row(
                "SELECT COUNT(*) FROM `chat_operations` WHERE `pending` = 1",
                [],
                |row| row.get(0),
            )
            .unwrap()
    }
}

#[test]
fn deletion_is_terminal_replay_safe_and_releases_media_reference() {
    let mut f = fixture();
    f.record().unwrap();
    assert_eq!(f.store.attachment_blobs().unwrap().len(), 1);
    assert!(f.apply(2, false).unwrap());
    assert!(f.store.current_chat_stickers(&f.peer).unwrap().is_empty());
    assert!(f.store.attachment_blobs().unwrap().is_empty());
    assert!(f.store.chat_message(&f.key).unwrap().unwrap().deleted);
    assert!(!f.apply(2, false).unwrap());
    let mut conflicting = action(2, false);
    if let Body::Delete { hlc, .. } = &mut conflicting {
        *hlc += 1;
    }
    assert!(f
        .store
        .apply_chat_operation(&f.key, &[8; 32], &conflicting, 10)
        .is_err());
    assert!(f
        .store
        .apply_chat_operation(&f.key, &[7; 32], &action(2, false), 10)
        .is_err());
    let path = std::path::PathBuf::from(f.store.db.path().unwrap());
    drop(f.store);
    f.store = Store::open(&path, &f.owner).unwrap();
    assert!(!f.record().unwrap());
    assert!(!f.apply(3, true).unwrap());
    assert_eq!(f.pending(), 0);
    f.sticker.pack_name = "Changed original".into();
    assert!(f.record().is_err());
}

#[test]
fn early_delete_binds_original_and_releases_pending_slots() {
    for early_edit in [false, true] {
        let mut f = fixture();
        if early_edit {
            assert!(f.apply(2, true).unwrap());
        }
        assert!(f.apply(3, false).unwrap());
        assert!(f.pending() > 0);
        assert!(!f.record().unwrap());
        assert_eq!(f.pending(), 0);
        assert!(!f.record().unwrap());
        assert!(f.store.current_chat_stickers(&f.peer).unwrap().is_empty());
        assert!(f.store.chat_message(&f.key).unwrap().unwrap().deleted);
        assert!(!f.apply(4, true).unwrap());
    }
}

#[test]
fn early_edit_cannot_turn_a_sticker_into_text() {
    let mut f = fixture();
    assert!(f.apply(2, true).unwrap());
    assert_eq!(f.pending(), 1);
    assert!(f.record().unwrap());
    assert_eq!(f.pending(), 0);
    assert!(f.store.current_chat_messages(&f.peer).unwrap().is_empty());
    assert!(f.apply(3, true).is_err());
    assert!(f.apply(2, false).is_err()); // Reused early-edit ID must roll back deletion.
    assert_eq!(f.store.current_chat_stickers(&f.peer).unwrap().len(), 1);
}

#[test]
fn wrong_scope_and_skew_do_not_delete_the_sticker() {
    let mut f = fixture();
    f.record().unwrap();
    for key in [
        MessageKey {
            author: f.owner.peer_id(),
            ..f.key.clone()
        },
        MessageKey {
            group: vec![10],
            ..f.key.clone()
        },
    ] {
        f.store
            .apply_chat_operation(&key, &[8; 32], &action(2, false), 10)
            .unwrap();
    }
    assert_eq!(f.store.current_chat_stickers(&f.peer).unwrap().len(), 1);
    let skewed = Body::Delete {
        target: f.key.id,
        operation: [5; 16],
        hlc: (dyapp_messaging::hlc::MAX_CLOCK_SKEW_MS + 100) << 16,
    };
    assert!(f
        .store
        .apply_chat_operation(&f.key, &[8; 32], &skewed, 10)
        .is_err());
    assert_eq!(f.store.current_chat_stickers(&f.peer).unwrap().len(), 1);
    assert_eq!(f.store.attachment_blobs().unwrap().len(), 1);
    assert!(f.apply(6, false).unwrap());
}
