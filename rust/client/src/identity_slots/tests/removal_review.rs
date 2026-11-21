use super::{root, slot, FIRST, SECOND};
use crate::identity_slots::{Active, KeyReader, Layout, Registry, Slot};
use dyapp_identity::Identity;
use std::collections::BTreeSet;
use std::fs::{self, OpenOptions, TryLockError};
use std::path::PathBuf;

#[derive(Default)]
struct Keys {
    denied: BTreeSet<String>,
    lock: Option<PathBuf>,
}

impl KeyReader for Keys {
    fn contains(&self, candidate: &Slot) -> Result<bool, String> {
        if self.denied.contains(&candidate.id) {
            Err("Controlled key access failure".into())
        } else {
            Ok(true)
        }
    }

    fn open(&self, candidate: &Slot, _: Option<&[u8]>) -> Result<Option<Identity>, String> {
        self.contains(candidate)?;
        if let Some(path) = &self.lock {
            let writer = OpenOptions::new()
                .read(true)
                .write(true)
                .open(path)
                .unwrap();
            assert!(matches!(writer.try_lock(), Err(TryLockError::WouldBlock)));
        }
        Ok(Some(Identity::from_secret(&[7; 32])))
    }
}

#[test]
fn review_uses_the_original_peer_and_only_ready_readable_local_copies() {
    let root = root();
    let original = slot(&root, FIRST, b"ready\n");
    let other = slot(&root, SECOND, b"ready\n");
    fs::write(original.join("identity.key"), b"wrapped fixture").unwrap();
    fs::write(other.join("identity.key"), b"wrapped fixture").unwrap();
    let registry = Registry::new(root.clone(), Layout::Android);
    let peer = Identity::from_secret(&[7; 32]).peer_id();
    let mut keys = Keys {
        lock: Some(root.join("profiles.lock")),
        ..Keys::default()
    };
    let review = registry.removal_review(FIRST, Some(&peer), &keys).unwrap();
    assert!(review.allowed && review.has_other_copy);
    assert_eq!(
        crate::identity_slots::active(&root, Layout::Android),
        Active::Absent
    );
    keys.denied.insert(SECOND.into());
    let review = registry.removal_review(FIRST, Some(&peer), &keys).unwrap();
    assert!(review.allowed && !review.has_other_copy);
    keys.denied.clear();
    fs::write(other.join("state"), b"pending\n").unwrap();
    assert!(
        !registry
            .removal_review(FIRST, Some(&peer), &keys)
            .unwrap()
            .has_other_copy
    );
    assert!(registry
        .removal_review(FIRST, Some("different-peer"), &keys)
        .is_err());
    assert!(registry.removal_review(FIRST, None, &keys).is_err());
    keys.denied.insert(FIRST.into());
    assert!(registry.removal_review(FIRST, Some(&peer), &keys).is_err());
    assert!(!registry.removal_review(FIRST, None, &keys).unwrap().allowed);
    assert_eq!(
        fs::read(original.join("identity.key")).unwrap(),
        b"wrapped fixture"
    );
    assert_eq!(fs::read(original.join("state")).unwrap(), b"ready\n");
}

#[test]
fn review_rejects_unknown_targets_and_keeps_explicit_missing_key_cleanup() {
    let root = root();
    let candidate = slot(&root, FIRST, b"ready\n");
    let registry = Registry::new(root.clone(), Layout::Android);
    let keys = Keys::default();
    let review = registry.removal_review(FIRST, None, &keys).unwrap();
    assert!(review.allowed && !review.has_other_copy);
    assert!(registry.removal_review(SECOND, None, &keys).is_err());
    assert!(!candidate.join("identity.key").exists());
    assert!(!root.join("active-profile").exists());
    assert_eq!(fs::read(candidate.join("state")).unwrap(), b"ready\n");
}
