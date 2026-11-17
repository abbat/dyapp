use super::{root, Credentials};
use crate::slot_keys::SlotKeys;
use crate::{IdentityKey, MobileSlotKeys};
use dyapp_client::identity_slots::{ChatKeyStore, Layout, Registry, Resolution, Storage};
use dyapp_client::link::Linked;
use dyapp_identity::Identity;
use std::sync::Arc;

#[test]
fn opaque_existing_wrap_refuses_missing_alias_and_keeps_the_owner() {
    let callbacks = Arc::new(Credentials::default());
    let device = IdentityKey::generate().unwrap();
    assert!(callbacks
        .wrap_existing("absent".into(), device.clone())
        .is_err());
    assert!(!callbacks.contains("absent".into()).unwrap());
    let owner = IdentityKey::generate().unwrap();
    let owner_peer = owner.peer_id();
    let wrapped_owner = callbacks.wrap("slot".into(), owner.clone()).unwrap();
    assert!(callbacks.wrap("slot".into(), device.clone()).is_err());
    let wrapped_device = callbacks
        .wrap_existing("slot".into(), device.clone())
        .unwrap();
    assert_eq!(
        callbacks
            .open_wrapped("slot".into(), wrapped_device)
            .unwrap()
            .unwrap()
            .peer_id(),
        device.peer_id()
    );
    assert_eq!(
        callbacks
            .open_wrapped("slot".into(), wrapped_owner)
            .unwrap()
            .unwrap()
            .peer_id(),
        owner_peer
    );
    callbacks.erase("slot".into()).unwrap();
    let keys = SlotKeys(callbacks);
    let slot = dyapp_client::identity_slots::Slot {
        id: "slot".into(),
        dir: root(),
        state: dyapp_client::identity_slots::State::Pending,
        storage: Some(Storage::Wrapped),
    };
    assert!(ChatKeyStore::wrap(
        &keys,
        &slot,
        dyapp_client::identity_slots::ChatKeyKind::Device,
        &device.identity.secret()
    )
    .is_err());
}

#[test]
fn opaque_slot_callbacks_feed_the_core_link_writer_and_chat_bootstrap() {
    let keys = SlotKeys(Arc::new(Credentials::default()));
    let registry = Registry::new(root(), Layout::Android);
    let (source, owner, _) = registry.create(Storage::Wrapped, &keys).unwrap();
    let linked = Linked {
        identity: Identity::generate(),
        state: b"pending".to_vec(),
    };
    let device = Identity::generate();
    let (slot, installed, _) = registry
        .link_selected_wrapped(&source.id, &owner.peer_id(), &linked, false, &keys, &device)
        .unwrap();
    assert_eq!(installed.peer_id(), linked.identity.peer_id());
    let Resolution::Ready(_, reopened) = registry.inspect(&slot.id, &keys).unwrap() else {
        panic!("linked slot must reopen");
    };
    assert_eq!(reopened.peer_id(), linked.identity.peer_id());
    let operation = registry
        .chat_operation(&slot.id, &installed.peer_id(), &keys, &keys)
        .unwrap();
    assert_eq!(operation.device().peer_id(), device.peer_id());
}
