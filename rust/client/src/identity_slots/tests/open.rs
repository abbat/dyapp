#[cfg(unix)]
mod unix;

use super::{root, slot, FIRST};
use crate::identity_slots::*;
use dyapp_identity::Identity;
use std::cell::Cell;
use std::fs;
use std::path::Path;

#[derive(Default)]
struct Reader {
    present: bool,
    denied: bool,
    calls: Cell<usize>,
}

impl KeyReader for Reader {
    fn contains(&self, _: &Slot) -> Result<bool, String> {
        if self.denied {
            Err("denied".into())
        } else {
            Ok(self.present)
        }
    }

    fn open(&self, _: &Slot, wrapped: Option<&[u8]>) -> Result<Option<Identity>, String> {
        self.calls.set(self.calls.get() + 1);
        if self.denied {
            return Err("denied".into());
        }
        if let Some(bytes) = wrapped {
            assert_eq!(bytes, b"old wrapped fixture");
        }
        Ok(self.present.then(|| Identity::from_secret(&[7; 32])))
    }
}

fn key_file(dir: &Path, bytes: &[u8]) {
    let path = dir.join("identity.key");
    fs::write(&path, bytes).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }
}

fn ready(result: Resolution) -> (Slot, String) {
    match result {
        Resolution::Ready(slot, identity) => (slot, identity.peer_id()),
        _ => panic!("expected the existing identity to open"),
    }
}

fn recovery(result: Resolution, expected: RecoveryReason) {
    let Resolution::Recovery {
        id,
        reason,
        peer_id,
    } = result
    else {
        panic!("expected recovery");
    };
    assert_eq!(reason, expected);
    assert_eq!(peer_id, None);
    let expected_id = if expected == RecoveryReason::InvalidPointer {
        None
    } else {
        Some(false)
    };
    assert_eq!(id.map(|id| id.is_empty()), expected_id);
}

#[test]
fn existing_file_system_and_wrapped_legacy_keys_keep_their_peer_ids() {
    let peer = Identity::from_secret(&[7; 32]).peer_id();
    for layout in [Layout::Desktop, Layout::Android, Layout::Ios] {
        let root = root();
        let keys = Reader {
            present: true,
            ..Reader::default()
        };
        if layout == Layout::Desktop {
            fs::write(root.join("identity-storage"), b"file\n").unwrap();
            key_file(&root, &[7; 32]);
        } else {
            key_file(&root, b"old wrapped fixture");
        }
        let (slot, opened) = ready(resolve(&root, layout, &keys).unwrap());
        assert_eq!(slot.id, LEGACY);
        assert_eq!(slot.dir, root);
        assert_eq!(opened, peer);
        assert_eq!(keys.calls.get(), usize::from(layout != Layout::Desktop));
    }
    let root = root();
    fs::write(root.join("identity-storage"), b"system\n").unwrap();
    let keys = Reader {
        present: true,
        ..Reader::default()
    };
    assert_eq!(
        ready(resolve(&root, Layout::Desktop, &keys).unwrap()).1,
        peer
    );
    assert!(!root.join("identity.key").exists());
}

#[test]
fn incomplete_and_deleting_slots_keep_the_derived_peer_id_without_becoming_ready() {
    for (marker, expected) in [
        (b"pending\n".as_slice(), RecoveryReason::Incomplete),
        (b"deleting\n", RecoveryReason::Deleting),
    ] {
        let root = root();
        let dir = slot(&root, FIRST, marker);
        key_file(&dir, b"old wrapped fixture");
        let keys = Reader {
            present: true,
            ..Reader::default()
        };
        let result = resolve(&root, Layout::Android, &keys).unwrap();
        assert!(
            matches!(result, Resolution::Recovery { reason, peer_id: Some(peer), .. }
            if reason == expected && peer == Identity::from_secret(&[7; 32]).peer_id())
        );
        assert_eq!(fs::read(dir.join("state")).unwrap(), marker);
    }
}

#[test]
fn missing_denied_and_damaged_slots_are_distinct_and_never_replace_keys() {
    let root = root();
    let dir = slot(&root, FIRST, b"ready\n");
    let keys = Reader::default();
    recovery(
        resolve(&root, Layout::Android, &keys).unwrap(),
        RecoveryReason::Missing,
    );
    assert!(!dir.join("identity.key").exists());
    key_file(&dir, b"old wrapped fixture");
    let denied = Reader {
        denied: true,
        ..Reader::default()
    };
    recovery(
        resolve(&root, Layout::Android, &denied).unwrap(),
        RecoveryReason::Unavailable,
    );
    fs::write(dir.join("state"), b"damaged").unwrap();
    recovery(
        resolve(&root, Layout::Android, &denied).unwrap(),
        RecoveryReason::Damaged,
    );
    assert_eq!(denied.calls.get(), 1);
    assert_eq!(
        fs::read(dir.join("identity.key")).unwrap(),
        b"old wrapped fixture"
    );
}

#[test]
fn empty_inventory_with_orphan_credentials_or_a_saved_pointer_never_becomes_setup() {
    let root = root();
    assert!(matches!(
        resolve(&root, Layout::Ios, &Reader::default()).unwrap(),
        Resolution::Setup
    ));
    let keys = Reader {
        present: true,
        ..Reader::default()
    };
    recovery(
        resolve(&root, Layout::Ios, &keys).unwrap(),
        RecoveryReason::Missing,
    );
    let denied = Reader {
        denied: true,
        ..Reader::default()
    };
    recovery(
        resolve(&root, Layout::Ios, &denied).unwrap(),
        RecoveryReason::Unavailable,
    );
    fs::write(root.join("active-profile"), b"corrupt").unwrap();
    recovery(
        resolve(&root, Layout::Android, &keys).unwrap(),
        RecoveryReason::InvalidPointer,
    );
    fs::write(root.join("active-profile"), format!("{FIRST}\n")).unwrap();
    recovery(
        resolve(&root, Layout::Android, &keys).unwrap(),
        RecoveryReason::Missing,
    );
    fs::write(root.join("profiles"), b"not a directory").unwrap();
    assert!(resolve(&root, Layout::Android, &keys).is_err());
}

#[test]
fn explicit_apple_unselection_and_missing_legacy_do_not_open_a_different_identity() {
    let root = root();
    let dir = slot(&root, FIRST, b"ready\n");
    key_file(&dir, b"old wrapped fixture");
    let keys = Reader {
        present: true,
        ..Reader::default()
    };
    fs::write(root.join("active-profile"), b"none\n").unwrap();
    assert!(
        matches!(resolve(&root, Layout::Ios, &keys).unwrap(), Resolution::Picker(slots) if slots.len() == 1)
    );
    assert_eq!(keys.calls.get(), 0);
    fs::write(root.join("active-profile"), b"legacy-primary\n").unwrap();
    recovery(
        resolve(&root, Layout::Ios, &keys).unwrap(),
        RecoveryReason::Missing,
    );
    assert_eq!(keys.calls.get(), 0);
    assert_eq!(
        ready(resolve(&root, Layout::Android, &keys).unwrap()).0.id,
        FIRST
    );
}

#[test]
fn invalid_raw_lengths_and_oversized_wrapped_files_fail_before_backend_access() {
    let root = root();
    let dir = slot(&root, FIRST, b"ready\n");
    fs::write(dir.join("storage-mode"), b"file\n").unwrap();
    key_file(&dir, &[7; 33]);
    let keys = Reader::default();
    recovery(
        resolve(&root, Layout::Desktop, &keys).unwrap(),
        RecoveryReason::Unavailable,
    );
    key_file(&dir, &[7; 4097]);
    recovery(
        resolve(&root, Layout::Android, &keys).unwrap(),
        RecoveryReason::Unavailable,
    );
    assert_eq!(keys.calls.get(), 0);
}
