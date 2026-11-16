mod interrupted;
mod received;
mod wrapped;

use super::root;
use crate::identity_slots::*;
use crate::link::Linked;
use crate::store::Store;
use dyapp_identity::network::{self, Network, Settings};
use dyapp_identity::Identity;
use dyapp_profile::Profile;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
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

#[derive(Clone, Copy)]
enum WrappedFailure {
    Missing,
    WrongOwner,
}

#[derive(Default)]
struct WrappedKeys {
    saved: RefCell<BTreeMap<String, Option<Identity>>>,
    failure: Cell<Option<WrappedFailure>>,
    device_failure: Cell<Option<wrapped::Failure>>,
    reservation_failure: Cell<bool>,
    generated: RefCell<Vec<ChatKeyKind>>,
}

impl KeyReader for WrappedKeys {
    fn contains(&self, slot: &Slot) -> Result<bool, String> {
        let exists = self.saved.borrow().contains_key(&slot.id);
        if !exists && self.reservation_failure.get() {
            return Err("reservation denied".into());
        }
        Ok(exists)
    }

    fn open(&self, slot: &Slot, _: Option<&[u8]>) -> Result<Option<Identity>, String> {
        Ok(self
            .saved
            .borrow()
            .get(&slot.id)
            .and_then(|key| key.as_ref().map(|key| Identity::from_secret(&key.secret()))))
    }
}

impl KeyWriter for WrappedKeys {
    fn store(&self, slot: &Slot, identity: &Identity) -> Result<Option<Vec<u8>>, String> {
        if self.contains(slot)? {
            return Err("Refuse an existing credential".into());
        }
        let saved = match self.failure.get() {
            None => Some(Identity::from_secret(&identity.secret())),
            Some(WrappedFailure::Missing) => None,
            Some(WrappedFailure::WrongOwner) => Some(Identity::generate()),
        };
        self.saved.borrow_mut().insert(slot.id.clone(), saved);
        Ok(Some(vec![1]))
    }
}

impl KeyRemover for WrappedKeys {
    fn credential_present(&self, slot: &Slot, _: Credential) -> Result<bool, String> {
        self.contains(slot)
    }

    fn erase_credential(&self, _: &Slot, _: Credential) -> Result<(), String> {
        Err("A failed install must not erase credentials".into())
    }
}

#[test]
fn selected_android_link_refuses_missing_or_wrong_reopened_wrapping_keys() {
    for failure in [WrappedFailure::Missing, WrappedFailure::WrongOwner] {
        let root = root();
        let registry = Registry::new(root.clone(), Layout::Android);
        let keys = WrappedKeys::default();
        let (source, owner, _) = registry.create(Storage::Wrapped, &keys).unwrap();
        keys.failure.set(Some(failure));
        let called = Cell::new(false);
        let device = |_: &Slot| {
            called.set(true);
            Ok(())
        };
        let error = registry.link_selected(
            &source.id,
            &owner.peer_id(),
            &incoming(),
            true,
            &keys,
            &device,
        );
        let expected = match failure {
            WrappedFailure::Missing => "Saved identity is missing",
            WrappedFailure::WrongOwner => "Saved identity does not reopen",
        };
        assert_eq!(error.unwrap_err(), expected);
        assert!(!called.get());
        assert_eq!(
            active(&root, Layout::Android),
            Active::Selected(source.id.clone())
        );
        let slots = registry.list().unwrap();
        assert_eq!(slots.len(), 2);
        assert!(slots
            .iter()
            .any(|slot| slot.id != source.id && slot.state == State::Pending));
        let Resolution::Ready(_, retained) = registry.inspect(&source.id, &keys).unwrap() else {
            panic!("the source must remain ready");
        };
        assert_eq!(retained.peer_id(), owner.peer_id());
        fs::remove_dir_all(root).unwrap();
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

fn incoming() -> Linked {
    Linked {
        identity: Identity::generate(),
        state: b"state".to_vec(),
    }
}

fn refused_selected_link(
    change: impl FnOnce(&Registry, &Slot, &Identity) -> String,
    expected: &str,
) {
    let root = root();
    let registry = Registry::new(root.clone(), Layout::Desktop);
    let (source, owner, _) = registry.create(Storage::File, &Files).unwrap();
    let expected_peer = change(&registry, &source, &owner);
    let before = active(&root, Layout::Desktop);
    let inventory = registry.list().unwrap();
    let called = RefCell::new(false);
    let device = |_: &Slot| {
        *called.borrow_mut() = true;
        Ok(())
    };
    let error = registry.link_selected(
        &source.id,
        &expected_peer,
        &incoming(),
        true,
        &Files,
        &device,
    );
    assert_eq!(error.unwrap_err(), expected);
    assert!(!called.into_inner());
    assert_eq!(registry.list().unwrap(), inventory);
    assert_eq!(active(&root, Layout::Desktop), before);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn selected_link_refuses_another_selected_identity_before_reserving_a_slot() {
    refused_selected_link(
        |registry, _, owner| {
            registry.create(Storage::File, &Files).unwrap();
            owner.peer_id()
        },
        "The link source changed",
    );
}

#[test]
fn selected_link_binds_the_slot_even_when_another_slot_has_the_same_owner() {
    refused_selected_link(
        |registry, source, owner| {
            let copied = Linked {
                identity: Identity::from_secret(&owner.secret()),
                state: b"state".to_vec(),
            };
            registry
                .link(&source.id, &copied, false, &Files, &|_| Ok(()))
                .unwrap();
            owner.peer_id()
        },
        "The link source changed",
    );
}

#[test]
fn selected_link_refuses_a_different_expected_owner_before_reserving_a_slot() {
    refused_selected_link(
        |_, _, _| Identity::generate().peer_id(),
        "The link source changed",
    );
}

#[test]
fn selected_link_refuses_a_source_that_is_no_longer_ready() {
    refused_selected_link(
        |_, source, owner| {
            fs::write(source.dir.join("state"), b"pending\n").unwrap();
            owner.peer_id()
        },
        "The link source is no longer ready",
    );
}

#[test]
fn selected_link_reuses_the_writer_and_removes_only_its_empty_wizard_source() {
    let root = root();
    let registry = Registry::new(root.clone(), Layout::Desktop);
    let (source, owner, _) = registry.create(Storage::File, &Files).unwrap();
    let linked = incoming();
    let called = RefCell::new(false);
    let device = |slot: &Slot| {
        assert_eq!(slot.state, State::Pending);
        *called.borrow_mut() = true;
        Ok(())
    };
    let (slot, identity, _) = registry
        .link_selected(&source.id, &owner.peer_id(), &linked, true, &Files, &device)
        .unwrap();
    assert!(called.into_inner());
    assert_eq!(slot.state, State::Ready);
    assert_eq!(identity.peer_id(), linked.identity.peer_id());
    assert_eq!(
        active(&root, Layout::Desktop),
        Active::Selected(slot.id.clone())
    );
    assert_eq!(registry.list().unwrap(), vec![slot.clone()]);
    let store = Store::open(&slot.dir.join("client.db"), &identity).unwrap();
    assert_eq!(store.link_pending().unwrap().unwrap(), b"state");
    drop(store);
    fs::remove_dir_all(root).unwrap();
}
