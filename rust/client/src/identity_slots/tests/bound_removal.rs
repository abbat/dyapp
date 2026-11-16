mod legacy;

use super::{link::WrappedKeys, root};
use crate::identity_slots::{
    active, Active, ChatBinding, ChatKeyKind, ChatKeyStore, Credential, KeyReader, KeyRemover,
    Layout, Registry, Slot, State, Storage,
};
use crate::{session::Session, store::Store};
use dyapp_identity::Identity;
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::fs::{self, OpenOptions, TryLockError};
use std::path::PathBuf;
use zeroize::Zeroizing;

struct Keys {
    base: WrappedKeys,
    erased: RefCell<BTreeSet<String>>,
    lock: PathBuf,
}
impl KeyReader for Keys {
    fn contains(&self, slot: &Slot) -> Result<bool, String> {
        Ok(!self.erased.borrow().contains(&slot.id) && self.base.contains(slot)?)
    }
    fn open(&self, slot: &Slot, wrapped: Option<&[u8]>) -> Result<Option<Identity>, String> {
        if self.contains(slot)? {
            if wrapped != Some(&[1][..]) {
                return Err("wrong fixture owner blob".into());
            }
            self.base.open(slot, wrapped)
        } else {
            Ok(None)
        }
    }
}
impl KeyRemover for Keys {
    fn credential_present(&self, slot: &Slot, role: Credential) -> Result<bool, String> {
        Ok(role == Credential::Identity && self.contains(slot)?)
    }
    fn erase_credential(&self, slot: &Slot, role: Credential) -> Result<(), String> {
        assert_eq!(role, Credential::Identity);
        self.erased.borrow_mut().insert(slot.id.clone());
        Ok(())
    }
}
impl ChatKeyStore for Keys {
    fn wrap(&self, slot: &Slot, kind: ChatKeyKind, secret: &[u8; 32]) -> Result<Vec<u8>, String> {
        self.base.wrap(slot, kind, secret)
    }
    fn unwrap(
        &self,
        slot: &Slot,
        kind: ChatKeyKind,
        wrapped: &[u8],
    ) -> Result<Zeroizing<[u8; 32]>, String> {
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&self.lock)
            .unwrap();
        assert!(matches!(
            lock.try_lock_shared(),
            Err(TryLockError::WouldBlock)
        ));
        if kind == ChatKeyKind::Identity {
            return self
                .open(slot, Some(wrapped))?
                .map(|owner| owner.secret())
                .ok_or_else(|| "missing owner".into());
        }
        self.base.unwrap(slot, kind, wrapped)
    }
}

struct Fixture {
    registry: Registry,
    keys: Keys,
    slot: Slot,
    owner: Identity,
    binding: ChatBinding,
}
impl Fixture {
    fn new() -> Self {
        let root = root();
        let registry = Registry::new(root.clone(), Layout::Android);
        let keys = Keys {
            base: WrappedKeys::default(),
            erased: RefCell::default(),
            lock: root.join("profiles.lock"),
        };
        let (slot, owner, _) = registry.create(Storage::Wrapped, &keys.base).unwrap();
        let operation = registry
            .chat_operation(&slot.id, &owner.peer_id(), &keys, &keys.base)
            .unwrap();
        let store = Store::open(&slot.dir.join("client.db"), operation.identity()).unwrap();
        drop(
            Session::open_or_create(
                store,
                operation.identity(),
                operation.device(),
                operation.state_key(),
            )
            .unwrap(),
        );
        let binding = operation.binding();
        drop(operation);
        Self {
            registry,
            keys,
            slot,
            owner,
            binding,
        }
    }

    fn unchanged(&self) {
        assert_eq!(self.registry.list().unwrap()[0].state, State::Ready);
        assert_eq!(
            active(
                self.slot.dir.parent().unwrap().parent().unwrap(),
                Layout::Android
            ),
            Active::Selected(self.slot.id.clone())
        );
        assert_eq!(self.keys.erased.borrow().len(), 0);
    }
}

#[test]
fn bound_removal_keeps_another_selected_identity_and_checks_under_the_exclusive_guard() {
    let f = Fixture::new();
    let (other, owner, _) = f.registry.create(Storage::Wrapped, &f.keys.base).unwrap();
    let result = f
        .registry
        .remove_bound_chat(&f.binding, &f.keys, &f.keys)
        .unwrap();
    assert_eq!(result.durability_detail, None);
    assert!(!f.slot.dir.exists());
    assert_eq!(f.registry.list().unwrap(), vec![other.clone()]);
    assert_eq!(
        active(
            other.dir.parent().unwrap().parent().unwrap(),
            Layout::Android
        ),
        Active::Selected(other.id.clone())
    );
    let crate::identity_slots::Resolution::Ready(_, reopened) =
        f.registry.inspect(&other.id, &f.keys).unwrap()
    else {
        panic!("removal damaged the other selected profile");
    };
    assert_eq!(reopened.peer_id(), owner.peer_id());
    assert_eq!(*f.keys.erased.borrow(), BTreeSet::from([f.slot.id]));
}

#[test]
fn a_consistent_replacement_device_or_sealing_key_cannot_authorize_bound_removal() {
    for replace_device in [true, false] {
        let f = Fixture::new();
        let operation = f
            .registry
            .existing_chat_operation(&f.slot.id, &f.owner.peer_id(), &f.keys, &f.keys.base)
            .unwrap();
        let mut store =
            Store::open_existing(&f.slot.dir.join("client.db"), operation.identity()).unwrap();
        let sealed = if replace_device {
            let device = Identity::generate();
            fs::write(
                f.slot.dir.join("device.key"),
                device
                    .secret()
                    .iter()
                    .map(|byte| byte ^ 0x5a)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
            dyapp_messaging::mls::Device::new(operation.identity(), device.public_key(), 1)
                .unwrap()
                .seal(operation.state_key())
                .unwrap()
        } else {
            let mut state = *operation.state_key();
            state[0] ^= 1;
            fs::write(
                f.slot.dir.join("chat-state.key"),
                state.map(|byte| byte ^ 0x5a),
            )
            .unwrap();
            dyapp_messaging::mls::Device::open(
                &store.sealed_device().unwrap().unwrap(),
                operation.state_key(),
            )
            .unwrap()
            .seal(&state)
            .unwrap()
        };
        store.stage_outbound(&sealed, &[]).unwrap();
        drop(store);
        drop(operation);
        assert_eq!(
            f.registry
                .remove_bound_chat(&f.binding, &f.keys, &f.keys)
                .unwrap_err(),
            "Chat context changed"
        );
        f.unchanged();
    }
}

#[test]
fn changed_lifecycle_identity_or_missing_existing_storage_refuses_before_mutation() {
    for name in [
        "state",
        "identity.key",
        "device.key",
        "chat-state.key",
        "client.db",
    ] {
        let f = Fixture::new();
        let path = f.slot.dir.join(name);
        let held = fs::read(&path).unwrap();
        let changed = if name == "state" {
            b"pending\n".to_vec()
        } else {
            vec![]
        };
        fs::write(&path, &changed).unwrap();
        assert!(f
            .registry
            .remove_bound_chat(&f.binding, &f.keys, &f.keys)
            .is_err());
        assert_eq!(fs::read(&path).unwrap(), changed);
        assert_eq!(f.keys.erased.borrow().len(), 0);
        fs::write(path, held).unwrap();
        f.unchanged();
    }
}

#[test]
fn a_binding_cannot_remove_a_copied_slot_using_the_same_owner_and_platform_alias() {
    let f = Fixture::new();
    let other_root = root();
    let copied = other_root.join("profiles").join(&f.slot.id);
    fs::create_dir_all(&copied).unwrap();
    for entry in fs::read_dir(&f.slot.dir).unwrap() {
        let entry = entry.unwrap();
        assert!(entry.file_type().unwrap().is_file());
        fs::copy(entry.path(), copied.join(entry.file_name())).unwrap();
    }
    let other = Registry::new(other_root, Layout::Android);
    assert_eq!(
        other
            .remove_bound_chat(&f.binding, &f.keys, &f.keys)
            .unwrap_err(),
        "Chat context changed"
    );
    assert_eq!(other.list().unwrap()[0].state, State::Ready);
    assert!(copied.join("identity.key").exists());
    f.unchanged();
}

#[test]
fn changing_the_layout_cannot_move_cleanup_to_a_different_registry_lock() {
    let f = Fixture::new();
    let root = f.keys.lock.parent().unwrap().to_path_buf();
    let other = Registry::new(root, Layout::Ios);
    assert_eq!(
        other
            .remove_bound_chat(&f.binding, &f.keys, &f.keys)
            .unwrap_err(),
        "Chat context changed"
    );
    f.unchanged();
}

#[test]
fn missing_existing_files_are_not_recreated_or_treated_as_authorized_cleanup() {
    for name in ["identity.key", "device.key", "chat-state.key", "client.db"] {
        let f = Fixture::new();
        let original = f.slot.dir.join(name);
        let retained = f
            .keys
            .lock
            .parent()
            .unwrap()
            .join(format!("retained-{name}"));
        fs::rename(&original, &retained).unwrap();
        assert!(f
            .registry
            .remove_bound_chat(&f.binding, &f.keys, &f.keys)
            .is_err());
        assert!(!original.exists());
        assert_eq!(f.keys.erased.borrow().len(), 0);
        fs::rename(retained, original).unwrap();
        f.unchanged();
    }
}
