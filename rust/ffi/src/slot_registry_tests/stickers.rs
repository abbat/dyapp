//! Exact-pack removal uses identity admission without ever opening chat/media keys.
use super::*;
use crate::{MobileIdentityOperation, MobileSlotSelection};
use dyapp_client::store::{messages::MessageKey, Store};
use std::fs::{self, OpenOptions, TryLockError};

struct Fixture {
    root: PathBuf,
    registry: Arc<MobileIdentityRegistry>,
    selected: MobileSlotSelection,
}

impl Fixture {
    fn new(layout: MobileSlotLayout) -> Self {
        let root = root();
        let registry = MobileIdentityRegistry::new(
            root.to_str().unwrap().to_owned(),
            layout,
            Arc::new(Credentials::default()),
        );
        let selected = registry.create().unwrap();
        Self {
            root,
            registry,
            selected,
        }
    }

    fn operation(&self) -> Arc<MobileIdentityOperation> {
        self.registry
            .operation(self.selected.slot.id.clone(), self.selected.key.peer_id())
            .unwrap()
    }

    fn directory(&self) -> PathBuf {
        PathBuf::from(&self.selected.slot.directory)
    }

    fn store(&self) -> Store {
        Store::open(
            &self.directory().join("client.db"),
            &self.selected.key.identity,
        )
        .unwrap()
    }
}

fn check_removal(f: &Fixture) {
    let directory = f.directory();
    let packs = directory.join("sticker-packs");
    fs::create_dir_all(&packs).unwrap();
    let selected = packs.join("01".repeat(32));
    let other = packs.join("02".repeat(32));
    fs::write(&selected, b"corrupt selected pack").unwrap();
    fs::write(&other, b"keep other pack").unwrap();
    fs::write(directory.join("chat-state.key"), b"unavailable media key").unwrap();
    let mut store = f.store();
    let message = MessageKey {
        group: vec![1],
        author: f.selected.key.peer_id(),
        id: [7; 16],
    };
    store.record_chat_text(&message, b"keep history").unwrap();
    let operation = f.operation();
    assert_eq!(
        operation
            .remove_saved_sticker_pack(vec![1; 31])
            .unwrap_err()
            .to_string(),
        "invalidPackId"
    );
    assert_eq!(fs::read(&selected).unwrap(), b"corrupt selected pack");
    assert!(operation.remove_saved_sticker_pack(vec![1; 32]).unwrap());
    assert!(!operation.remove_saved_sticker_pack(vec![1; 32]).unwrap());
    assert!(!selected.exists());
    assert_eq!(fs::read(other).unwrap(), b"keep other pack");
    assert_eq!(
        fs::read(directory.join("chat-state.key")).unwrap(),
        b"unavailable media key"
    );
    assert!(store.sealed_device().unwrap().is_none());
    assert!(store.due_outbound(i64::MAX).unwrap().is_empty());
    let current = store.chat_message(&message).unwrap().unwrap();
    assert_eq!(current.text.as_deref(), Some(b"keep history".as_slice()));
    assert!(!current.edited && !current.deleted);
}

#[test]
fn removal_without_media_keys_preserves_other_packs_history_and_outbox() {
    for layout in [MobileSlotLayout::Android, MobileSlotLayout::Ios] {
        check_removal(&Fixture::new(layout));
    }
}

#[test]
fn mismatched_or_missing_identity_admission_cannot_remove_a_saved_file() {
    for layout in [MobileSlotLayout::Android, MobileSlotLayout::Ios] {
        let f = Fixture::new(layout);
        let packs = f.directory().join("sticker-packs");
        fs::create_dir_all(&packs).unwrap();
        let file = packs.join("01".repeat(32));
        fs::write(&file, b"keep selected pack").unwrap();
        assert!(f
            .registry
            .operation(
                f.selected.slot.id.clone(),
                IdentityKey::generate().peer_id()
            )
            .is_err());
        assert!(f
            .registry
            .operation("missing".into(), f.selected.key.peer_id())
            .is_err());
        assert_eq!(fs::read(file).unwrap(), b"keep selected pack");
        assert!(!f.directory().join("client.db").exists());
    }
}

#[test]
fn removal_operation_keeps_identity_mutations_excluded_through_handle_completion() {
    for layout in [MobileSlotLayout::Android, MobileSlotLayout::Ios] {
        let f = Fixture::new(layout);
        let operation = f.operation();
        let name = match layout {
            MobileSlotLayout::Android => "profiles.lock",
            MobileSlotLayout::Ios => ".identity-slots.lock",
        };
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .open(f.root.join(name))
            .unwrap();
        assert!(matches!(lock.try_lock(), Err(TryLockError::WouldBlock)));
        assert!(!operation.remove_saved_sticker_pack(vec![1; 32]).unwrap());
        assert!(matches!(lock.try_lock(), Err(TryLockError::WouldBlock)));
        drop(operation);
        lock.try_lock()
            .expect("dropping the completed operation releases identity mutation admission");
    }
}
