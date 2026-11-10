use super::root;
use crate::identity_slots::{KeyReader, Layout, Registry, Resolution, Slot};
use dyapp_identity::Identity;
use std::fs::{self, OpenOptions, TryLockError};
use std::path::PathBuf;

struct Probe(PathBuf);

impl KeyReader for Probe {
    fn contains(&self, _: &Slot) -> Result<bool, String> {
        let file = OpenOptions::new().write(true).open(&self.0).unwrap();
        assert!(matches!(file.try_lock(), Err(TryLockError::WouldBlock)));
        Ok(false)
    }

    fn open(&self, _: &Slot, _: Option<&[u8]>) -> Result<Option<Identity>, String> {
        panic!("an empty inventory must not open a key")
    }
}

#[test]
fn resolution_holds_the_compatible_lock_through_backend_completion_then_releases_it() {
    for (layout, name) in [
        (Layout::Desktop, "profiles.lock"),
        (Layout::Android, "profiles.lock"),
        (Layout::Ios, ".identity-slots.lock"),
    ] {
        let root = root();
        let path = root.join(name);
        fs::write(&path, b"existing lock bytes").unwrap();
        let registry = Registry::new(root.clone(), layout);
        assert_eq!(registry.list().unwrap(), [] as [Slot; 0]);
        assert!(matches!(
            registry.resolve(&Probe(path.clone())).unwrap(),
            Resolution::Setup
        ));
        let file = OpenOptions::new().write(true).open(&path).unwrap();
        file.try_lock().expect("operation released its lock");
        assert_eq!(fs::read(&path).unwrap(), b"existing lock bytes");
        assert!(!root.join("identity.key").exists());
    }
}

#[test]
fn an_inventory_failure_releases_the_lock_and_is_not_fresh_setup() {
    let root = root();
    fs::write(root.join("profiles"), b"not a directory").unwrap();
    let registry = Registry::new(root.clone(), Layout::Android);
    assert!(registry.list().is_err());
    let file = OpenOptions::new()
        .write(true)
        .open(root.join("profiles.lock"))
        .unwrap();
    file.try_lock().expect("failed inventory released its lock");
}

#[cfg(unix)]
#[test]
fn a_symlinked_lock_does_not_modify_the_target() {
    let root = root();
    let target = root.join("unrelated");
    fs::write(&target, b"keep").unwrap();
    std::os::unix::fs::symlink(&target, root.join("profiles.lock")).unwrap();
    assert!(Registry::new(root, Layout::Android).list().is_err());
    assert_eq!(fs::read(target).unwrap(), b"keep");
}
