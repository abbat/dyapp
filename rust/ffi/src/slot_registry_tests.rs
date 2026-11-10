//! Fixtures cross the exported registry and opaque-key callback boundary.

use crate::{
    IdentityKey, KeyStore, MobileIdentityRegistry, MobileSlotKeys, MobileSlotLayout,
    MobileSlotRecovery, MobileSlotResolution, WrapError,
};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Credentials {
    aliases: Mutex<BTreeSet<String>>,
}

struct Wrapping;

impl KeyStore for Wrapping {
    fn wrap(&self, secret: Vec<u8>) -> Result<Vec<u8>, WrapError> {
        Ok(secret.into_iter().map(|byte| byte ^ 0x5a).collect())
    }

    fn unwrap(&self, wrapped: Vec<u8>) -> Result<Vec<u8>, WrapError> {
        self.wrap(wrapped)
    }
}

impl MobileSlotKeys for Credentials {
    fn contains(&self, id: String) -> Result<bool, WrapError> {
        Ok(self.aliases.lock().unwrap().contains(&id))
    }

    fn open(&self, id: String, wrapped: Vec<u8>) -> Result<Option<Arc<IdentityKey>>, WrapError> {
        if !self.contains(id)? {
            return Ok(None);
        }
        IdentityKey::unwrap(Arc::new(Wrapping), wrapped)
            .map(Some)
            .map_err(|error| wrap_error(&error))
    }

    fn wrap(&self, id: String, key: Arc<IdentityKey>) -> Result<Vec<u8>, WrapError> {
        if !self.aliases.lock().unwrap().insert(id) {
            return Err(WrapError::Failed {
                reason: "existing credential".to_owned(),
            });
        }
        key.wrap(Arc::new(Wrapping))
            .map_err(|error| wrap_error(&error))
    }

    fn erase(&self, id: String) -> Result<(), WrapError> {
        self.aliases.lock().unwrap().remove(&id);
        Ok(())
    }
}

fn wrap_error(error: &crate::CoreError) -> WrapError {
    WrapError::Failed {
        reason: error.to_string(),
    }
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../build/native-slot-fixtures")
        .join(IdentityKey::generate().peer_id())
}

#[test]
fn exported_registry_creates_reopens_and_selects_the_same_identity() {
    for layout in [MobileSlotLayout::Android, MobileSlotLayout::Ios] {
        let root = root().to_str().unwrap().to_owned();
        let keys = Arc::new(Credentials::default());
        let registry = MobileIdentityRegistry::new(root.clone(), layout, keys.clone());
        assert!(matches!(
            registry.resolve().unwrap(),
            MobileSlotResolution::Setup
        ));
        let created = registry.create().unwrap();
        let peer = created.key.peer_id();
        let reopened = MobileIdentityRegistry::new(root, layout, keys);
        let MobileSlotResolution::Ready { slot, key } = reopened.resolve().unwrap() else {
            panic!("created identity must reopen");
        };
        assert_eq!(slot.id, created.slot.id);
        assert_eq!(key.peer_id(), peer);
        assert_eq!(reopened.list().unwrap().len(), 1);
        let selected = reopened.select(slot.id).unwrap();
        assert_eq!(selected.key.peer_id(), peer);
        #[cfg(unix)]
        assert_eq!(selected.durability_detail, None);
    }
}

#[test]
fn literal_legacy_and_uuid_layouts_reopen_without_rewriting_the_key() {
    const UUID: &str = "12345678-1234-4234-8234-123456789abc";
    for layout in [MobileSlotLayout::Android, MobileSlotLayout::Ios] {
        for id in ["legacy-primary", UUID] {
            let root = root();
            let directory = if id == "legacy-primary" {
                root.clone()
            } else {
                root.join("profiles").join(id)
            };
            std::fs::create_dir_all(&directory).unwrap();
            let keys = Arc::new(Credentials::default());
            let identity = IdentityKey::generate();
            let wrapped = keys.wrap(id.to_owned(), identity.clone()).unwrap();
            std::fs::write(directory.join("identity.key"), &wrapped).unwrap();
            if id == UUID {
                std::fs::write(directory.join("state"), b"ready\n").unwrap();
            }
            std::fs::write(root.join("active-profile"), format!("{id}\n")).unwrap();
            let registry =
                MobileIdentityRegistry::new(root.to_str().unwrap().to_owned(), layout, keys);
            let MobileSlotResolution::Ready { slot, key } = registry.resolve().unwrap() else {
                panic!("existing layout must reopen");
            };
            assert_eq!(slot.id, id);
            assert_eq!(key.peer_id(), identity.peer_id());
            assert_eq!(
                std::fs::read(directory.join("identity.key")).unwrap(),
                wrapped
            );
        }
    }
}

#[test]
fn committed_selection_retains_the_key_and_unconfirmed_durability() {
    use dyapp_client::identity_slots::{SelectionDurability, Slot, State, Storage};
    let identity = dyapp_identity::Identity::generate();
    let peer = identity.peer_id();
    let selected = crate::slot_registry::selected(
        Slot {
            id: "legacy-primary".to_owned(),
            dir: root(),
            state: State::Ready,
            storage: Some(Storage::Wrapped),
        },
        identity,
        SelectionDurability::Unconfirmed {
            detail: "directory flush failed".to_owned(),
        },
    );
    assert_eq!(selected.key.peer_id(), peer);
    assert_eq!(
        selected.durability_detail.as_deref(),
        Some("directory flush failed")
    );
}

#[test]
fn interrupted_lifecycle_never_opens_as_ready() {
    for (marker, expected) in [
        ("pending\n", MobileSlotRecovery::Incomplete),
        ("deleting\n", MobileSlotRecovery::Deleting),
        ("invalid\n", MobileSlotRecovery::Damaged),
    ] {
        let root = root();
        let registry = MobileIdentityRegistry::new(
            root.to_str().unwrap().to_owned(),
            MobileSlotLayout::Android,
            Arc::new(Credentials::default()),
        );
        let created = registry.create().unwrap();
        std::fs::write(PathBuf::from(&created.slot.directory).join("state"), marker).unwrap();
        let MobileSlotResolution::Recovery {
            reason, peer_id, ..
        } = registry.resolve().unwrap()
        else {
            panic!("interrupted slot must require recovery");
        };
        assert_eq!(reason, expected);
        if expected != MobileSlotRecovery::Damaged {
            assert_eq!(peer_id, Some(created.key.peer_id()));
        }
        assert!(registry.select(created.slot.id).is_err());
    }
}

#[test]
fn native_operation_retains_mutation_protection_with_its_verified_key() {
    use std::fs::{OpenOptions, TryLockError};
    let root = root();
    let registry = MobileIdentityRegistry::new(
        root.to_str().unwrap().to_owned(),
        MobileSlotLayout::Android,
        Arc::new(Credentials::default()),
    );
    let created = registry.create().unwrap();
    let operation = registry
        .operation(created.slot.id.clone(), created.key.peer_id())
        .unwrap();
    assert_eq!(operation.slot().id, created.slot.id);
    assert_eq!(operation.key().peer_id(), created.key.peer_id());
    assert_eq!(registry.list().unwrap().len(), 1);
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.join("profiles.lock"))
        .unwrap();
    assert!(matches!(file.try_lock(), Err(TryLockError::WouldBlock)));
    drop(operation);
    file.try_lock()
        .expect("native operation released the core guard");
}

#[test]
fn exported_removal_preserves_confirmation_and_only_erases_the_requested_alias() {
    for layout in [MobileSlotLayout::Android, MobileSlotLayout::Ios] {
        let root = root().to_str().unwrap().to_owned();
        let keys = Arc::new(Credentials::default());
        let registry = MobileIdentityRegistry::new(root, layout, keys.clone());
        let first = registry.create().unwrap();
        let second = registry.create().unwrap();
        let MobileSlotResolution::Ready { key, .. } =
            registry.inspect(first.slot.id.clone()).unwrap()
        else {
            panic!("picker must open the requested candidate");
        };
        assert_eq!(key.peer_id(), first.key.peer_id());
        assert!(registry
            .remove(second.slot.id.clone(), Some(first.key.peer_id()))
            .is_err());
        assert!(registry.remove(second.slot.id.clone(), None).is_err());
        let detail = registry
            .remove(second.slot.id.clone(), Some(second.key.peer_id()))
            .unwrap();
        #[cfg(unix)]
        assert_eq!(detail, None);
        assert_eq!(registry.list().unwrap().len(), 1);
        assert_eq!(registry.list().unwrap()[0].id, first.slot.id);
        assert!(keys.contains(first.slot.id).unwrap());
        assert!(!keys.contains(second.slot.id).unwrap());
    }
}

struct Denied;

impl MobileSlotKeys for Denied {
    fn erase(&self, id: String) -> Result<(), WrapError> {
        self.contains(id).map(|_| ())
    }

    fn contains(&self, _: String) -> Result<bool, WrapError> {
        Err(WrapError::Failed {
            reason: "denied".to_owned(),
        })
    }

    fn open(&self, _: String, _: Vec<u8>) -> Result<Option<Arc<IdentityKey>>, WrapError> {
        self.contains(String::new())?;
        unreachable!()
    }

    fn wrap(&self, _: String, _: Arc<IdentityKey>) -> Result<Vec<u8>, WrapError> {
        self.contains(String::new())?;
        unreachable!()
    }
}

#[test]
fn missing_and_denied_credentials_preserve_selection_and_remain_distinct() {
    let root = root();
    let path = root.to_str().unwrap().to_owned();
    let keys = Arc::new(Credentials::default());
    let registry =
        MobileIdentityRegistry::new(path.clone(), MobileSlotLayout::Android, keys.clone());
    let created = registry.create().unwrap();
    let pointer = std::fs::read(root.join("active-profile")).unwrap();
    keys.aliases.lock().unwrap().clear();
    let MobileSlotResolution::Recovery { id, reason, .. } = registry.resolve().unwrap() else {
        panic!("missing credential must require recovery");
    };
    assert_eq!(id, Some(created.slot.id));
    assert_eq!(reason, MobileSlotRecovery::Missing);
    let denied = MobileIdentityRegistry::new(path, MobileSlotLayout::Android, Arc::new(Denied));
    let MobileSlotResolution::Recovery { reason, .. } = denied.resolve().unwrap() else {
        panic!("denied credential must require recovery");
    };
    assert_eq!(reason, MobileSlotRecovery::Unavailable);
    assert_eq!(std::fs::read(root.join("active-profile")).unwrap(), pointer);
}
