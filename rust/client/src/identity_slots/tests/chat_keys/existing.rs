use super::*;

#[test]
fn existing_admission_refuses_uninitialized_storage_without_generating_credentials() {
    for layout in [Layout::Android, Layout::Ios] {
        let (dir, registry, wrapping) = fixture(layout);
        assert!(registry
            .existing_chat_operation(FIRST, &wrapping.owner.peer_id(), &wrapping, &wrapping)
            .is_err());
        assert!(wrapping.generated.borrow().is_empty());
        for name in ["client.db", "device.key", "chat-state.key"] {
            assert!(!dir.join(name).exists());
        }
    }
}

#[test]
fn existing_admission_reopens_the_same_device_and_releases_all_locks_when_idle() {
    for layout in [Layout::Android, Layout::Ios] {
        let (dir, registry, wrapping) = fixture(layout);
        recovery::initialize(&registry, &wrapping);
        let generated = wrapping.generated.borrow().clone();
        let operation = registry
            .existing_chat_operation(FIRST, &wrapping.owner.peer_id(), &wrapping, &wrapping)
            .unwrap();
        let device = operation.device().peer_id();
        let state = *operation.state_key();
        assert!(matches!(
            handle(&wrapping.lock).try_lock(),
            Err(TryLockError::WouldBlock)
        ));
        assert!(matches!(
            handle(&dir.join("chat.lock")).try_lock(),
            Err(TryLockError::WouldBlock)
        ));
        drop(operation);
        handle(&wrapping.lock).try_lock().unwrap();
        handle(&dir.join("chat.lock")).try_lock().unwrap();
        let resumed = registry
            .existing_chat_operation(FIRST, &wrapping.owner.peer_id(), &wrapping, &wrapping)
            .unwrap();
        assert_eq!(resumed.device().peer_id(), device);
        assert_eq!(resumed.state_key(), &state);
        assert_eq!(*wrapping.generated.borrow(), generated);
    }
}

#[test]
fn missing_existing_credentials_or_database_are_not_recreated() {
    for name in ["device.key", "chat-state.key", "client.db"] {
        let (dir, registry, wrapping) = fixture(Layout::Android);
        recovery::initialize(&registry, &wrapping);
        let generated = wrapping.generated.borrow().clone();
        fs::rename(dir.join(name), dir.join(format!("{name}.saved"))).unwrap();
        assert!(registry
            .existing_chat_operation(FIRST, &wrapping.owner.peer_id(), &wrapping, &wrapping)
            .is_err());
        assert!(!dir.join(name).exists());
        assert_eq!(*wrapping.generated.borrow(), generated);
    }
}

#[test]
fn empty_databases_are_not_initialized_by_existing_admission() {
    let (dir, registry, wrapping) = fixture(Layout::Android);
    let path = dir.join("client.db");
    fs::write(&path, []).unwrap();
    assert!(registry
        .existing_chat_operation(FIRST, &wrapping.owner.peer_id(), &wrapping, &wrapping)
        .is_err());
    assert_eq!(fs::read(path).unwrap(), [] as [u8; 0]);
    assert!(wrapping.generated.borrow().is_empty());
}

#[test]
fn changed_existing_credentials_or_lifecycle_fail_without_regeneration() {
    for name in ["device.key", "chat-state.key", "identity.key", "state"] {
        let (dir, registry, wrapping) = fixture(Layout::Android);
        recovery::initialize(&registry, &wrapping);
        let generated = wrapping.generated.borrow().clone();
        let changed = if name == "state" {
            b"pending\n".to_vec()
        } else {
            vec![9; 32]
        };
        fs::write(dir.join(name), &changed).unwrap();
        assert!(registry
            .existing_chat_operation(FIRST, &wrapping.owner.peer_id(), &wrapping, &wrapping)
            .is_err());
        assert_eq!(fs::read(dir.join(name)).unwrap(), changed);
        assert_eq!(*wrapping.generated.borrow(), generated);
    }
}
