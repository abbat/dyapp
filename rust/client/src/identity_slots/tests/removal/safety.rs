use super::*;
use crate::identity_slots::tests::SECOND;
use crate::identity_slots::{active, Active, State};

fn seed_owned_data(dir: &std::path::Path) {
    for name in [
        "device.key",
        "seal.key",
        "chat-state.key",
        "client.db",
        "client.db-wal",
        "network",
    ] {
        fs::write(dir.join(name), b"owned data").unwrap();
    }
    for name in ["originals", "sticker-packs"] {
        fs::create_dir(dir.join(name)).unwrap();
        fs::write(dir.join(name).join("a".repeat(64)), b"sealed media").unwrap();
    }
    fs::write(
        dir.join("sticker-packs")
            .join(format!(".{}.pending", "b".repeat(32))),
        b"pending",
    )
    .unwrap();
}

#[test]
fn all_layouts_remove_only_confirmed_identity_data_and_preserve_installation_node() {
    for (layout, storage) in [
        (Layout::Desktop, Storage::System),
        (Layout::Desktop, Storage::File),
        (Layout::Android, Storage::Wrapped),
        (Layout::Ios, Storage::Wrapped),
    ] {
        let (root, registry, backend) = fixture(layout, storage);
        let dir = root.join("profiles").join(FIRST);
        let other = slot(&root, SECOND, b"ready\n");
        fs::write(other.join("identity.key"), b"other identity").unwrap();
        fs::create_dir(root.join("node")).unwrap();
        fs::write(root.join("node").join("key"), b"installation").unwrap();
        seed_owned_data(&dir);
        let outcome = registry
            .remove(FIRST, Some(&backend.identity.peer_id()), &backend)
            .unwrap();
        #[cfg(unix)]
        assert_eq!(outcome.durability_detail, None);
        assert!(!dir.exists());
        assert!(!root
            .join("profiles")
            .join(format!(".{FIRST}.removed"))
            .exists());
        assert_eq!(
            fs::read(other.join("identity.key")).unwrap(),
            b"other identity"
        );
        assert_eq!(
            fs::read(root.join("node").join("key")).unwrap(),
            b"installation"
        );
        assert_eq!(
            active(&root, layout),
            if layout == Layout::Ios {
                Active::Unselected
            } else {
                Active::Absent
            }
        );
    }
}

#[test]
fn unknown_files_and_nested_media_leave_every_key_and_pointer_intact() {
    for name in [
        "notes.txt",
        "node.json",
        "originals/notes.txt",
        "sticker-packs/unexpected",
    ] {
        let (root, registry, backend) = fixture(Layout::Android, Storage::Wrapped);
        let dir = root.join("profiles").join(FIRST);
        let unknown = dir.join(name);
        fs::create_dir_all(unknown.parent().unwrap()).unwrap();
        fs::write(&unknown, b"keep").unwrap();
        let pointer = fs::read(root.join("active-profile")).unwrap();
        assert!(registry
            .remove(FIRST, Some(&backend.identity.peer_id()), &backend)
            .is_err());
        assert_eq!(*backend.present.borrow(), [false, false, true]);
        assert_eq!(*backend.erased.borrow(), [] as [Credential; 0]);
        assert_eq!(registry.list().unwrap()[0].state, State::Ready);
        assert_eq!(fs::read(dir.join("identity.key")).unwrap(), b"wrapped");
        assert_eq!(fs::read(root.join("active-profile")).unwrap(), pointer);
        assert_eq!(fs::read(unknown).unwrap(), b"keep");
    }
}

#[test]
fn wrong_or_missing_confirmation_and_denied_access_never_erase_credentials() {
    let (_, registry, backend) = fixture(Layout::Android, Storage::Wrapped);
    for confirmed in [None, Some("another identity")] {
        assert!(registry.remove(FIRST, confirmed, &backend).is_err());
        assert_eq!(registry.list().unwrap()[0].state, State::Ready);
    }
    backend.denied.set(true);
    assert!(registry
        .remove(FIRST, Some(&backend.identity.peer_id()), &backend)
        .is_err());
    assert_eq!(*backend.erased.borrow(), [] as [Credential; 0]);
}

#[test]
fn legacy_and_damaged_slots_are_never_removed() {
    let (root, registry, backend) = fixture(Layout::Android, Storage::Wrapped);
    fs::write(root.join("identity.key"), b"legacy").unwrap();
    assert!(registry.remove("legacy-primary", None, &backend).is_err());
    fs::write(root.join("profiles").join(FIRST).join("state"), b"damaged").unwrap();
    assert!(registry
        .remove(FIRST, Some(&backend.identity.peer_id()), &backend)
        .is_err());
    assert_eq!(fs::read(root.join("identity.key")).unwrap(), b"legacy");
    assert_eq!(*backend.erased.borrow(), [] as [Credential; 0]);
}

#[test]
fn removing_an_inactive_identity_preserves_the_other_selection() {
    let (root, registry, backend) = fixture(Layout::Android, Storage::Wrapped);
    let other = slot(&root, SECOND, b"ready\n");
    fs::write(other.join("identity.key"), b"other key").unwrap();
    let pointer = format!("{SECOND}\n").into_bytes();
    fs::write(root.join("active-profile"), &pointer).unwrap();
    registry
        .remove(FIRST, Some(&backend.identity.peer_id()), &backend)
        .unwrap();
    assert_eq!(fs::read(root.join("active-profile")).unwrap(), pointer);
    assert_eq!(fs::read(other.join("identity.key")).unwrap(), b"other key");
}

#[test]
fn absent_wrapping_credentials_allow_cleanup_of_an_orphan_oversize_blob() {
    let (root, registry, backend) = fixture(Layout::Android, Storage::Wrapped);
    backend.present.borrow_mut()[2] = false;
    fs::write(
        root.join("profiles").join(FIRST).join("identity.key"),
        [0; 4097],
    )
    .unwrap();
    registry.remove(FIRST, None, &backend).unwrap();
    assert_eq!(registry.list().unwrap(), [] as [Slot; 0]);
    assert_eq!(*backend.erased.borrow(), [] as [Credential; 0]);
}

#[test]
fn a_nested_media_directory_is_not_an_owned_hash_file() {
    let (root, registry, backend) = fixture(Layout::Android, Storage::Wrapped);
    let nested = root
        .join("profiles")
        .join(FIRST)
        .join("originals")
        .join("a".repeat(64));
    fs::create_dir_all(&nested).unwrap();
    fs::write(nested.join("valuable"), b"preserve").unwrap();
    assert!(registry
        .remove(FIRST, Some(&backend.identity.peer_id()), &backend)
        .is_err());
    assert_eq!(*backend.erased.borrow(), [] as [Credential; 0]);
    assert_eq!(fs::read(nested.join("valuable")).unwrap(), b"preserve");
}

#[cfg(unix)]
#[test]
fn symlinked_owned_files_or_media_are_refused_before_erasure() {
    for name in ["network", "originals", "sticker-packs"] {
        let (root, registry, backend) = fixture(Layout::Android, Storage::Wrapped);
        let outside = root.join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("valuable"), b"untouched").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("profiles").join(FIRST).join(name)).unwrap();
        assert!(registry
            .remove(FIRST, Some(&backend.identity.peer_id()), &backend)
            .is_err());
        assert_eq!(*backend.present.borrow(), [false, false, true]);
        assert_eq!(*backend.erased.borrow(), [] as [Credential; 0]);
        assert_eq!(fs::read(outside.join("valuable")).unwrap(), b"untouched");
    }
}
