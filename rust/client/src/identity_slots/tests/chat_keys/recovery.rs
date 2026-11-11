use super::*;
use crate::{session::Session, store::Store};

fn initialize(registry: &Registry, wrapping: &Wrapping) {
    let operation = registry
        .chat_operation(FIRST, &wrapping.owner.peer_id(), wrapping, wrapping)
        .unwrap();
    let store = Store::open(
        &operation.slot().dir.join("client.db"),
        operation.identity(),
    )
    .unwrap();
    let session = Session::open_or_create(
        store,
        operation.identity(),
        operation.device(),
        operation.state_key(),
    )
    .unwrap();
    assert!(session.store().sealed_device().unwrap().is_some());
}

#[test]
fn initialized_chats_never_regenerate_a_missing_device_or_state_key() {
    for name in ["device.key", "chat-state.key"] {
        let (dir, registry, wrapping) = fixture(Layout::Android);
        initialize(&registry, &wrapping);
        let other = if name == "device.key" {
            "chat-state.key"
        } else {
            "device.key"
        };
        let existing = fs::read(dir.join(other)).unwrap();
        let generated = wrapping.generated.borrow().clone();
        fs::remove_file(dir.join(name)).unwrap();
        assert!(registry
            .chat_operation(FIRST, &wrapping.owner.peer_id(), &wrapping, &wrapping)
            .is_err());
        assert!(!dir.join(name).exists());
        assert_eq!(fs::read(dir.join(other)).unwrap(), existing);
        assert_eq!(*wrapping.generated.borrow(), generated);
    }
}

#[test]
fn initialized_chats_reject_a_different_device_or_state_key_without_replacing_it() {
    for name in ["device.key", "chat-state.key"] {
        let (dir, registry, wrapping) = fixture(Layout::Android);
        initialize(&registry, &wrapping);
        let old = fs::read(dir.join(name)).unwrap();
        let changed: Vec<_> = old.iter().map(|byte| byte ^ 1).collect();
        fs::write(dir.join(name), &changed).unwrap();
        let generated = wrapping.generated.borrow().clone();
        assert!(registry
            .chat_operation(FIRST, &wrapping.owner.peer_id(), &wrapping, &wrapping)
            .is_err());
        assert_eq!(fs::read(dir.join(name)).unwrap(), changed);
        assert_eq!(*wrapping.generated.borrow(), generated);
    }
}

#[test]
fn invalid_existing_wrapped_files_fail_before_generating_any_other_key() {
    for name in ["device.key", "chat-state.key"] {
        let (dir, registry, wrapping) = fixture(Layout::Android);
        fs::write(dir.join(name), [0; 4097]).unwrap();
        assert!(registry
            .chat_operation(FIRST, &wrapping.owner.peer_id(), &wrapping, &wrapping)
            .is_err());
        assert_eq!(*wrapping.generated.borrow(), [] as [ChatKeyKind; 0]);
        assert_eq!(fs::read(dir.join(name)).unwrap(), [0; 4097]);
    }
}

#[test]
fn a_corrupt_existing_database_never_triggers_key_generation() {
    let (dir, registry, wrapping) = fixture(Layout::Android);
    fs::write(dir.join("client.db"), b"not a database").unwrap();
    assert!(registry
        .chat_operation(FIRST, &wrapping.owner.peer_id(), &wrapping, &wrapping)
        .is_err());
    assert_eq!(*wrapping.generated.borrow(), [] as [ChatKeyKind; 0]);
    assert_eq!(fs::read(dir.join("client.db")).unwrap(), b"not a database");
}

#[cfg(unix)]
#[test]
fn ancillary_symlinks_are_not_followed_or_replaced() {
    for name in ["device.key", "chat-state.key"] {
        let (dir, registry, wrapping) = fixture(Layout::Android);
        let outside = dir.parent().unwrap().join("outside");
        fs::write(&outside, b"keep").unwrap();
        std::os::unix::fs::symlink(&outside, dir.join(name)).unwrap();
        assert!(registry
            .chat_operation(FIRST, &wrapping.owner.peer_id(), &wrapping, &wrapping)
            .is_err());
        assert_eq!(*wrapping.generated.borrow(), [] as [ChatKeyKind; 0]);
        assert_eq!(fs::read(outside).unwrap(), b"keep");
    }
}

#[test]
fn wrong_identity_or_lifecycle_never_generates_chat_keys() {
    let (dir, registry, wrapping) = fixture(Layout::Android);
    assert!(registry
        .chat_operation(FIRST, "wrong peer", &wrapping, &wrapping)
        .is_err());
    fs::write(dir.join("state"), b"pending\n").unwrap();
    assert!(registry
        .chat_operation(FIRST, &wrapping.owner.peer_id(), &wrapping, &wrapping)
        .is_err());
    assert_eq!(*wrapping.generated.borrow(), [] as [ChatKeyKind; 0]);
}
