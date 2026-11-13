use super::root;
use crate::identity_slots::*;
use crate::link::Linked;
use crate::store::Store;
use dyapp_identity::network::{self, Network, Settings};
use dyapp_identity::Identity;
use dyapp_profile::Profile;
use std::cell::RefCell;
use std::fs;

/// File storage keeps every key in the slot directory, so the platform holds nothing.
struct Files;

impl KeyReader for Files {
    fn contains(&self, _: &Slot) -> Result<bool, String> {
        Ok(false)
    }

    fn open(&self, _: &Slot, _: Option<&[u8]>) -> Result<Option<Identity>, String> {
        Ok(None)
    }
}

impl KeyWriter for Files {
    fn store(&self, _: &Slot, _: &Identity) -> Result<Option<Vec<u8>>, String> {
        unreachable!("file storage writes the key itself")
    }
}

impl KeyRemover for Files {
    fn credential_present(&self, _: &Slot, _: Credential) -> Result<bool, String> {
        Ok(false)
    }

    fn erase_credential(&self, _: &Slot, _: Credential) -> Result<(), String> {
        Ok(())
    }
}

/// Links into a fresh registry; returns whether the waiting slot was kept.
fn run(wizard: bool, published: bool) -> bool {
    let root = root();
    let registry = Registry::new(root.clone(), Layout::Desktop);
    let (old, current, _) = registry.create(Storage::File, &Files).unwrap();
    let rows = Network {
        seeds: vec!["127.0.0.1:1".into()],
        name: String::new(),
    };
    network::save(&old.dir, Settings::Active, &rows).unwrap();
    if published {
        Store::open(&old.dir.join("client.db"), &current)
            .unwrap()
            .save_profile_draft(&Profile::default())
            .unwrap();
    }
    let linked = Linked {
        identity: Identity::generate(),
        state: b"state".to_vec(),
    };
    let called = RefCell::new(None);
    let device = |slot: &Slot| {
        assert_eq!(slot.state, State::Pending);
        *called.borrow_mut() = Some(slot.id.clone());
        Ok(())
    };
    let (slot, identity, _) = registry
        .link(&old.id, &linked, wizard, &Files, &device)
        .unwrap();

    assert_eq!(called.into_inner().as_deref(), Some(slot.id.as_str()));
    assert_eq!(slot.state, State::Ready);
    assert_eq!(identity.peer_id(), linked.identity.peer_id());
    assert_eq!(
        active(&root, Layout::Desktop),
        Active::Selected(slot.id.clone())
    );
    let copied = network::load(&slot.dir, Settings::Active).unwrap();
    assert_eq!(copied, Some(rows));
    let store = Store::open(&slot.dir.join("client.db"), &linked.identity).unwrap();
    assert_eq!(store.link_pending().unwrap().unwrap(), b"state");
    drop(store);
    let kept = list(&root, Layout::Desktop)
        .unwrap()
        .iter()
        .any(|s| s.id == old.id);
    fs::remove_dir_all(root).unwrap();
    kept
}

#[test]
fn a_record_makes_a_ready_slot_and_only_an_empty_wizard_slot_goes() {
    assert!(!run(true, false));
    assert!(run(false, false));
    assert!(run(true, true));
}

#[test]
fn a_failed_device_key_leaves_the_new_slot_pending_and_the_selection_alone() {
    let root = root();
    let registry = Registry::new(root.clone(), Layout::Desktop);
    let (old, _, _) = registry.create(Storage::File, &Files).unwrap();
    let linked = Linked {
        identity: Identity::generate(),
        state: b"state".to_vec(),
    };
    let device = |_: &Slot| Err("denied".to_owned());
    let error = registry.link(&old.id, &linked, true, &Files, &device);
    assert_eq!(error.unwrap_err(), "denied");

    assert_eq!(
        active(&root, Layout::Desktop),
        Active::Selected(old.id.clone())
    );
    let slots = list(&root, Layout::Desktop).unwrap();
    assert_eq!(slots.len(), 2);
    assert!(slots
        .iter()
        .any(|s| s.id == old.id && s.state == State::Ready));
    assert!(slots
        .iter()
        .any(|s| s.id != old.id && s.state == State::Pending));
    let unknown = registry.link("missing", &linked, true, &Files, &device);
    assert_eq!(unknown.unwrap_err(), "Unknown identity");
    fs::remove_dir_all(root).unwrap();
}
