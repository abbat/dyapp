use super::{root, FIRST};
use crate::identity_slots::*;
use dyapp_identity::Identity;
use std::cell::RefCell;
use std::fs;

#[derive(Default)]
struct Writer {
    secret: RefCell<Option<[u8; 32]>>,
    fail: bool,
    mismatch: bool,
    collision: bool,
}

impl KeyReader for Writer {
    fn contains(&self, _: &Slot) -> Result<bool, String> {
        Ok(self.collision)
    }

    fn open(&self, _: &Slot, wrapped: Option<&[u8]>) -> Result<Option<Identity>, String> {
        let secret = wrapped
            .map(|bytes| <[u8; 32]>::try_from(bytes).unwrap())
            .or(*self.secret.borrow());
        Ok(secret.map(|mut secret| {
            if self.mismatch {
                secret[0] ^= 1;
            }
            Identity::from_secret(&secret)
        }))
    }
}

impl KeyWriter for Writer {
    fn store(&self, slot: &Slot, identity: &Identity) -> Result<Option<Vec<u8>>, String> {
        assert_eq!(fs::read(slot.dir.join("state")).unwrap(), b"pending\n");
        if self.fail {
            return Err("platform denied".into());
        }
        let secret = *identity.secret();
        *self.secret.borrow_mut() = Some(secret);
        Ok((slot.storage == Some(Storage::Wrapped)).then(|| secret.to_vec()))
    }
}

#[test]
fn all_storage_layouts_reopen_before_becoming_ready_and_selected() {
    for (layout, storage) in [
        (Layout::Desktop, Storage::File),
        (Layout::Desktop, Storage::System),
        (Layout::Android, Storage::Wrapped),
        (Layout::Ios, Storage::Wrapped),
    ] {
        let root = root();
        let registry = Registry::new(root.clone(), layout);
        let keys = Writer::default();
        let (slot, identity) = registry.create(storage, &keys).unwrap();
        assert_eq!(slot.state, State::Ready);
        assert_eq!(fs::read(slot.dir.join("state")).unwrap(), b"ready\n");
        assert_eq!(active(&root, layout), Active::Selected(slot.id.clone()));
        let Resolution::Ready(opened, reopened) = registry.resolve(&keys).unwrap() else {
            panic!("created key must reopen");
        };
        assert_eq!(opened.id, slot.id);
        assert_eq!(reopened.peer_id(), identity.peer_id());
    }
}

#[test]
fn denial_or_reopen_mismatch_preserves_the_previous_selection_and_pending_state() {
    for keys in [
        Writer {
            fail: true,
            ..Writer::default()
        },
        Writer {
            mismatch: true,
            ..Writer::default()
        },
    ] {
        let root = root();
        let previous = format!("{FIRST}\n");
        fs::write(root.join("active-profile"), &previous).unwrap();
        let registry = Registry::new(root.clone(), Layout::Android);
        assert!(registry.create(Storage::Wrapped, &keys).is_err());
        assert_eq!(
            fs::read(root.join("active-profile")).unwrap(),
            previous.as_bytes()
        );
        let slots = registry.list().unwrap();
        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0].state, State::Pending);
    }
}

#[test]
fn existing_aliases_are_not_reused_and_stranded_pointer_temps_are_not_overwritten() {
    let root = root();
    let registry = Registry::new(root.clone(), Layout::Android);
    let collision = Writer {
        collision: true,
        ..Writer::default()
    };
    assert!(registry.create(Storage::Wrapped, &collision).is_err());
    assert_eq!(registry.list().unwrap(), [] as [Slot; 0]);
    fs::write(root.join("active-profile.tmp"), b"stranded").unwrap();
    fs::write(root.join("active-profile"), format!("{FIRST}\n")).unwrap();
    assert!(registry
        .create(Storage::Wrapped, &Writer::default())
        .is_err());
    assert_eq!(
        fs::read(root.join("active-profile.tmp")).unwrap(),
        b"stranded"
    );
    assert_eq!(
        active(&root, Layout::Android),
        Active::Selected(FIRST.into())
    );
}
