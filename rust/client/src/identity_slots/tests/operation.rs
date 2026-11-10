use super::{root, slot, FIRST};
use crate::identity_slots::{KeyReader, Layout, Registry, Slot};
use dyapp_identity::Identity;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

struct Keys {
    identity: Identity,
    lock: PathBuf,
}

fn handle(path: &Path) -> File {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .unwrap()
}

impl KeyReader for Keys {
    fn contains(&self, _: &Slot) -> Result<bool, String> {
        Ok(true)
    }

    fn open(&self, _: &Slot, wrapped: Option<&[u8]>) -> Result<Option<Identity>, String> {
        assert_eq!(wrapped, Some(b"wrapped".as_slice()));
        let file = handle(&self.lock);
        assert!(matches!(file.try_lock(), Err(TryLockError::WouldBlock)));
        file.try_lock_shared()
            .expect("verification holds a shared lock");
        Ok(Some(Identity::from_secret(&self.identity.secret())))
    }
}

fn fixture() -> (Registry, Keys) {
    let root = root();
    let dir = slot(&root, FIRST, b"ready\n");
    fs::write(dir.join("identity.key"), b"wrapped").unwrap();
    let keys = Keys {
        identity: Identity::generate(),
        lock: root.join("profiles.lock"),
    };
    (Registry::new(root, Layout::Android), keys)
}

#[test]
fn multiple_operations_and_inventory_share_read_access_until_the_last_guard_drops() {
    let (registry, keys) = fixture();
    let peer = keys.identity.peer_id();
    let first = registry.operation(FIRST, &peer, &keys).unwrap();
    let second = registry.operation(FIRST, &peer, &keys).unwrap();
    assert_eq!(first.identity().peer_id(), peer);
    assert_eq!(first.slot().id, FIRST);
    assert_eq!(registry.list().unwrap().len(), 1);
    assert!(registry.resolve(&keys).is_ok());
    let file = handle(&keys.lock);
    assert!(matches!(file.try_lock(), Err(TryLockError::WouldBlock)));
    drop(first);
    assert!(matches!(file.try_lock(), Err(TryLockError::WouldBlock)));
    drop(second);
    file.try_lock()
        .expect("last operation released mutation protection");
}

#[test]
fn refused_operations_release_the_lock_and_never_admit_a_changed_identity() {
    let (registry, keys) = fixture();
    assert!(registry.operation(FIRST, "different peer", &keys).is_err());
    assert!(registry
        .operation("../outside", &keys.identity.peer_id(), &keys)
        .is_err());
    fs::write(registry.list().unwrap()[0].dir.join("state"), b"pending\n").unwrap();
    assert!(registry
        .operation(FIRST, &keys.identity.peer_id(), &keys)
        .is_err());
    handle(&keys.lock)
        .try_lock()
        .expect("every failed admission releases its lock");
}

#[test]
fn worker_retains_operation_protection_until_completion_signal() {
    let (registry, keys) = fixture();
    let operation = registry
        .operation(FIRST, &keys.identity.peer_id(), &keys)
        .unwrap();
    let (started, observe_start) = mpsc::channel();
    let (finish, observe_finish) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        started.send(()).unwrap();
        observe_finish
            .recv_timeout(Duration::from_secs(5))
            .expect("completion signal");
        drop(operation);
    });
    observe_start
        .recv_timeout(Duration::from_secs(5))
        .expect("worker admitted");
    let file = handle(&keys.lock);
    assert!(matches!(file.try_lock(), Err(TryLockError::WouldBlock)));
    finish.send(()).unwrap();
    worker.join().unwrap();
    file.try_lock()
        .expect("completed worker released its guard");
}
