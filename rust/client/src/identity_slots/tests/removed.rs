use super::{root, slot, FIRST};
use crate::identity_slots::{
    KeyReader, Layout, RecoveryReason, Registry, Resolution, Slot, State, Storage,
};
use dyapp_identity::Identity;
use std::fs;

struct NoCredentials;

impl KeyReader for NoCredentials {
    fn contains(&self, _: &Slot) -> Result<bool, String> {
        Ok(false)
    }

    fn open(&self, _: &Slot, _: Option<&[u8]>) -> Result<Option<Identity>, String> {
        panic!("no fixture has an identity file");
    }
}

#[test]
fn hidden_interrupted_removal_is_recovery_even_after_markers_were_cleaned() {
    for layout in [Layout::Android, Layout::Desktop] {
        let root = root();
        let hidden = root.join("profiles").join(format!(".{FIRST}.removed"));
        fs::create_dir_all(&hidden).unwrap();
        fs::write(hidden.join("client.db"), b"preserve encrypted data").unwrap();
        let registry = Registry::new(root.clone(), layout);
        let slots = registry.list().unwrap();
        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0].id, FIRST);
        assert_eq!(slots[0].dir, hidden);
        assert_eq!(slots[0].state, State::Deleting);
        assert!(matches!(
            registry.resolve(&NoCredentials).unwrap(),
            Resolution::Recovery { .. }
        ));
        assert_eq!(
            fs::read(hidden.join("client.db")).unwrap(),
            b"preserve encrypted data"
        );
        assert_eq!(
            crate::identity_slots::list(&root, Layout::Ios).unwrap(),
            [] as [Slot; 0]
        );
    }
}

#[test]
fn visible_and_hidden_identity_collision_is_damaged_in_both_creation_orders() {
    for hidden_first in [true, false] {
        let root = root();
        let hidden = format!(".{FIRST}.removed");
        let names = if hidden_first {
            [hidden.as_str(), FIRST]
        } else {
            [FIRST, hidden.as_str()]
        };
        for name in names {
            slot(&root, name, b"ready\n");
        }
        let slots = crate::identity_slots::list(&root, Layout::Android).unwrap();
        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0].state, State::Broken);
        assert_eq!(slots[0].storage, None);
        assert_eq!(slots[0].dir, root.join("profiles").join(FIRST));
        for name in names {
            assert_eq!(
                fs::read(root.join("profiles").join(name).join("state")).unwrap(),
                b"ready\n"
            );
        }
    }
}

#[test]
fn malformed_hidden_names_are_foreign_and_hidden_regular_files_are_broken() {
    let root = root();
    for name in [format!("{FIRST}.removed"), ".not-a-uuid.removed".to_owned()] {
        slot(&root, &name, b"ready\n");
    }
    let hidden = root.join("profiles").join(format!(".{FIRST}.removed"));
    fs::write(&hidden, b"foreign file").unwrap();
    let slots = crate::identity_slots::list(&root, Layout::Android).unwrap();
    assert_eq!(slots.len(), 1);
    assert_eq!(slots[0].state, State::Broken);
    assert_eq!(fs::read(hidden).unwrap(), b"foreign file");
}

#[test]
fn apple_tombstone_keeps_a_partially_cleaned_regular_directory_recoverable() {
    let root = root();
    let dir = root.join("profiles").join(FIRST);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        root.join("profiles").join(format!("{FIRST}.deleting")),
        b"deleting\n",
    )
    .unwrap();
    let slots = crate::identity_slots::list(&root, Layout::Ios).unwrap();
    assert_eq!(slots[0].state, State::Deleting);
    fs::write(dir.join("state"), b"malformed").unwrap();
    assert_eq!(
        crate::identity_slots::list(&root, Layout::Ios).unwrap()[0].state,
        State::Broken
    );
}

#[cfg(unix)]
#[test]
fn apple_tombstone_never_upgrades_a_symlinked_slot_to_deleting() {
    let root = root();
    let outside = root.join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("identity.key"), b"untouched").unwrap();
    fs::create_dir(root.join("profiles")).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("profiles").join(FIRST)).unwrap();
    fs::write(
        root.join("profiles").join(format!("{FIRST}.deleting")),
        b"deleting\n",
    )
    .unwrap();
    let registry = Registry::new(root, Layout::Ios);
    assert_eq!(registry.list().unwrap()[0].state, State::Broken);
    assert!(matches!(
        registry.resolve(&NoCredentials).unwrap(),
        Resolution::Recovery {
            reason: RecoveryReason::Damaged,
            ..
        }
    ));
    assert_eq!(
        fs::read(outside.join("identity.key")).unwrap(),
        b"untouched"
    );
}

#[cfg(unix)]
#[test]
fn hidden_symlink_is_not_followed() {
    let root = root();
    let outside = root.join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("identity.key"), b"untouched").unwrap();
    fs::create_dir(root.join("profiles")).unwrap();
    std::os::unix::fs::symlink(
        &outside,
        root.join("profiles").join(format!(".{FIRST}.removed")),
    )
    .unwrap();
    let slots = crate::identity_slots::list(&root, Layout::Android).unwrap();
    assert_eq!(slots[0].state, State::Broken);
    assert_eq!(slots[0].storage, None::<Storage>);
    assert_eq!(
        fs::read(outside.join("identity.key")).unwrap(),
        b"untouched"
    );
}
