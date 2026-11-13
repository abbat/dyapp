mod concurrency;
mod existing;
mod recovery;

use super::{root, slot, FIRST};
use crate::identity_slots::{ChatKeyKind, ChatKeyStore, KeyReader, Layout, Registry, Slot};
use dyapp_identity::Identity;
use std::cell::{Cell, RefCell};
use std::fs::{self, File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

struct Wrapping {
    owner: Identity,
    lock: PathBuf,
    generated: RefCell<Vec<ChatKeyKind>>,
    fail: Cell<Option<ChatKeyKind>>,
    mismatch: Cell<bool>,
}

impl KeyReader for Wrapping {
    fn contains(&self, _: &Slot) -> Result<bool, String> {
        Ok(true)
    }

    fn open(&self, _: &Slot, wrapped: Option<&[u8]>) -> Result<Option<Identity>, String> {
        let expected: Vec<_> = self.owner.secret().iter().map(|byte| byte ^ 0x5a).collect();
        if wrapped != Some(expected.as_slice()) {
            return Err("wrong owner blob".into());
        }
        Ok(Some(Identity::from_secret(&self.owner.secret())))
    }
}

fn handle(path: &Path) -> File {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .unwrap()
}

impl ChatKeyStore for Wrapping {
    fn wrap(&self, _: &Slot, kind: ChatKeyKind, secret: &[u8; 32]) -> Result<Vec<u8>, String> {
        assert!(matches!(
            handle(&self.lock).try_lock_shared(),
            Err(TryLockError::WouldBlock)
        ));
        self.generated.borrow_mut().push(kind);
        if self.fail.get() == Some(kind) {
            return Err("wrapping denied".into());
        }
        Ok(secret.iter().map(|byte| byte ^ 0x5a).collect())
    }

    fn unwrap(
        &self,
        _: &Slot,
        kind: ChatKeyKind,
        wrapped: &[u8],
    ) -> Result<Zeroizing<[u8; 32]>, String> {
        let bytes: Vec<_> = wrapped.iter().map(|byte| byte ^ 0x5a).collect();
        let mut secret =
            Zeroizing::new(<[u8; 32]>::try_from(bytes).map_err(|_| "invalid wrapped length")?);
        if self.mismatch.get() && kind != ChatKeyKind::Identity {
            secret[0] ^= 1;
        }
        Ok(secret)
    }
}

fn fixture(layout: Layout) -> (PathBuf, Registry, Wrapping) {
    let root = root();
    let directory = slot(&root, FIRST, b"ready\n");
    let owner = Identity::generate();
    let wrapped: Vec<_> = owner.secret().iter().map(|byte| byte ^ 0x5a).collect();
    fs::write(directory.join("identity.key"), wrapped).unwrap();
    let name = if layout == Layout::Ios {
        ".identity-slots.lock"
    } else {
        "profiles.lock"
    };
    let wrapping = Wrapping {
        owner,
        lock: root.join(name),
        generated: RefCell::new(Vec::new()),
        fail: Cell::new(None),
        mismatch: Cell::new(false),
    };
    (directory, Registry::new(root, layout), wrapping)
}

#[test]
fn wrapped_mobile_bootstrap_reuses_both_keys_and_holds_operation_locks() {
    for layout in [Layout::Android, Layout::Ios] {
        let (dir, registry, wrapping) = fixture(layout);
        let peer = wrapping.owner.peer_id();
        let operation = registry
            .chat_operation(FIRST, &peer, &wrapping, &wrapping)
            .unwrap();
        let device = operation.device().peer_id();
        let state = *operation.state_key();
        assert_eq!(operation.slot().dir, dir);
        assert_eq!(operation.identity().peer_id(), peer);
        assert_eq!(registry.list().unwrap().len(), 1);
        assert!(matches!(
            handle(&wrapping.lock).try_lock(),
            Err(TryLockError::WouldBlock)
        ));
        assert!(matches!(
            handle(&dir.join("chat.lock")).try_lock(),
            Err(TryLockError::WouldBlock)
        ));
        drop(operation);
        let reopened = registry
            .chat_operation(FIRST, &peer, &wrapping, &wrapping)
            .unwrap();
        assert_eq!(reopened.device().peer_id(), device);
        assert_eq!(reopened.state_key(), &state);
        assert_eq!(
            *wrapping.generated.borrow(),
            [ChatKeyKind::Device, ChatKeyKind::State]
        );
    }
}

#[test]
fn a_state_wrap_failure_keeps_the_verified_device_key_for_retry() {
    let (dir, registry, wrapping) = fixture(Layout::Android);
    wrapping.fail.set(Some(ChatKeyKind::State));
    let peer = wrapping.owner.peer_id();
    assert!(registry
        .chat_operation(FIRST, &peer, &wrapping, &wrapping)
        .is_err());
    let existing = fs::read(dir.join("device.key")).unwrap();
    assert!(!dir.join("chat-state.key").exists());
    wrapping.fail.set(None);
    let operation = registry
        .chat_operation(FIRST, &peer, &wrapping, &wrapping)
        .unwrap();
    assert_eq!(fs::read(dir.join("device.key")).unwrap(), existing);
    assert_eq!(
        *wrapping.generated.borrow(),
        [ChatKeyKind::Device, ChatKeyKind::State, ChatKeyKind::State]
    );
    drop(operation);
}

#[test]
fn a_wrapped_key_that_does_not_reopen_is_not_persisted() {
    let (dir, registry, wrapping) = fixture(Layout::Android);
    wrapping.mismatch.set(true);
    assert!(registry
        .chat_operation(FIRST, &wrapping.owner.peer_id(), &wrapping, &wrapping)
        .is_err());
    assert!(!dir.join("device.key").exists());
    assert!(!dir.join("chat-state.key").exists());
}
