use super::*;
use crate::MobileSlotSelection;
use std::fs::{self, OpenOptions, TryLockError};

fn fixture(
    layout: MobileSlotLayout,
) -> (Arc<MobileIdentityRegistry>, MobileSlotSelection, PathBuf) {
    let root = root();
    let registry = MobileIdentityRegistry::new(
        root.to_str().unwrap().to_owned(),
        layout,
        Arc::new(Credentials::default()),
    );
    let created = registry.create().unwrap();
    (registry, created, root)
}

#[test]
fn guarded_native_chat_sessions_retain_locks_through_last_owner_then_allow_removal() {
    for layout in [MobileSlotLayout::Android, MobileSlotLayout::Ios] {
        let (registry, created, root) = fixture(layout);
        let chats = registry
            .open_chats(
                created.slot.id.clone(),
                created.key.peer_id(),
                Arc::new(Wrapping),
            )
            .unwrap();
        assert_eq!(chats.chat_rows().unwrap().len(), 0);
        let worker = chats.clone();
        drop(chats);
        let name = match layout {
            MobileSlotLayout::Android => "profiles.lock",
            MobileSlotLayout::Ios => ".identity-slots.lock",
        };
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(root.join(name))
            .unwrap();
        assert!(matches!(file.try_lock(), Err(TryLockError::WouldBlock)));
        drop(worker);
        registry
            .remove(created.slot.id, Some(created.key.peer_id()))
            .unwrap();
        assert_eq!(registry.list().unwrap().len(), 0);
        assert!(!PathBuf::from(created.slot.directory).exists());
    }
}

#[test]
fn initialized_native_chats_refuse_a_missing_key_without_regeneration() {
    let (registry, created, _) = fixture(MobileSlotLayout::Android);
    let open = || {
        registry.open_chats(
            created.slot.id.clone(),
            created.key.peer_id(),
            Arc::new(Wrapping),
        )
    };
    drop(open().unwrap());
    let directory = PathBuf::from(&created.slot.directory);
    let state = fs::read(directory.join("chat-state.key")).unwrap();
    fs::remove_file(directory.join("device.key")).unwrap();
    assert!(open().is_err());
    assert!(!directory.join("device.key").exists());
    assert_eq!(fs::read(directory.join("chat-state.key")).unwrap(), state);
}

struct OtherStore;

impl KeyStore for OtherStore {
    fn wrap(&self, bytes: Vec<u8>) -> Result<Vec<u8>, WrapError> {
        Ok(bytes.into_iter().map(|byte| byte ^ 0x33).collect())
    }

    fn unwrap(&self, bytes: Vec<u8>) -> Result<Vec<u8>, WrapError> {
        self.wrap(bytes)
    }
}

#[test]
fn a_different_wrapping_store_is_rejected_before_bootstrap_writes() {
    let (registry, created, _) = fixture(MobileSlotLayout::Android);
    assert!(registry
        .open_chats(created.slot.id, created.key.peer_id(), Arc::new(OtherStore))
        .is_err());
    let directory = PathBuf::from(created.slot.directory);
    assert!(!directory.join("device.key").exists());
    assert!(!directory.join("chat-state.key").exists());
}
