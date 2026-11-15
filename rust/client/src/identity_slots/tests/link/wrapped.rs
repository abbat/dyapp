use super::{incoming, root, WrappedKeys};
use crate::identity_slots::{
    active, Active, ChatKeyKind, ChatKeyStore, KeyReader, Layout, Registry, Slot, State, Storage,
};
use dyapp_identity::Identity;
use std::fs;
use zeroize::Zeroizing;

#[derive(Clone, Copy)]
pub(super) enum Failure {
    Wrap,
    Mismatch,
    Oversized,
    PersistedMismatch,
    Existing,
}

impl ChatKeyStore for WrappedKeys {
    fn wrap(&self, slot: &Slot, kind: ChatKeyKind, secret: &[u8; 32]) -> Result<Vec<u8>, String> {
        assert!(self.contains(slot)?);
        self.generated.borrow_mut().push(kind);
        if kind == ChatKeyKind::Device {
            match self.device_failure.get() {
                Some(Failure::Wrap) => return Err("wrapping denied".into()),
                Some(Failure::Oversized) => return Ok(vec![0; 4_097]),
                Some(Failure::Existing) => {
                    fs::write(slot.dir.join("device.key"), b"existing device").unwrap();
                }
                _ => {}
            }
        }
        Ok(secret.iter().map(|byte| byte ^ 0x5a).collect())
    }

    fn unwrap(
        &self,
        slot: &Slot,
        kind: ChatKeyKind,
        wrapped: &[u8],
    ) -> Result<Zeroizing<[u8; 32]>, String> {
        if kind == ChatKeyKind::Identity {
            return Ok(self
                .open(slot, Some(wrapped))?
                .ok_or("missing owner")?
                .secret());
        }
        let bytes: Vec<_> = wrapped.iter().map(|byte| byte ^ 0x5a).collect();
        let mut secret =
            Zeroizing::new(<[u8; 32]>::try_from(bytes).map_err(|_| "invalid wrapped length")?);
        if kind == ChatKeyKind::Device
            && match self.device_failure.get() {
                Some(Failure::Mismatch) => true,
                Some(Failure::PersistedMismatch) => slot.dir.join("device.key").exists(),
                _ => false,
            }
        {
            secret[0] ^= 1;
        }
        Ok(secret)
    }
}

#[test]
fn wrapped_link_preserves_the_received_device_through_chat_bootstrap() {
    let root = root();
    let registry = Registry::new(root.clone(), Layout::Android);
    let keys = WrappedKeys::default();
    let (source, owner, _) = registry.create(Storage::Wrapped, &keys).unwrap();
    let linked = incoming();
    let device = Identity::generate();
    let (slot, installed, _) = registry
        .link_selected_wrapped(&source.id, &owner.peer_id(), &linked, false, &keys, &device)
        .unwrap();
    assert_eq!(installed.peer_id(), linked.identity.peer_id());
    assert_eq!(slot.state, State::Ready);
    let saved = fs::read(slot.dir.join("device.key")).unwrap();
    assert_ne!(saved.as_slice(), device.secret().as_ref());
    let chat = registry
        .chat_operation(&slot.id, &installed.peer_id(), &keys, &keys)
        .unwrap();
    assert_eq!(chat.device().peer_id(), device.peer_id());
    assert_eq!(fs::read(slot.dir.join("device.key")).unwrap(), saved);
    assert_eq!(
        *keys.generated.borrow(),
        [ChatKeyKind::Device, ChatKeyKind::State]
    );
    drop(chat);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn wrapped_link_failures_preserve_the_source_and_a_pending_destination() {
    for (failure, expected) in [
        (Failure::Wrap, "wrapping denied"),
        (Failure::Mismatch, "Wrapped chat key does not reopen"),
        (
            Failure::Oversized,
            "Wrapped chat key exceeds the storage limit",
        ),
        (Failure::PersistedMismatch, "Saved chat key does not reopen"),
    ] {
        let root = root();
        let registry = Registry::new(root.clone(), Layout::Android);
        let keys = WrappedKeys::default();
        let (source, owner, _) = registry.create(Storage::Wrapped, &keys).unwrap();
        keys.device_failure.set(Some(failure));
        let result = registry.link_selected_wrapped(
            &source.id,
            &owner.peer_id(),
            &incoming(),
            true,
            &keys,
            &Identity::generate(),
        );
        assert_eq!(result.unwrap_err(), expected);
        assert_eq!(
            active(&root, Layout::Android),
            Active::Selected(source.id.clone())
        );
        let slots = registry.list().unwrap();
        assert_eq!(slots.len(), 2);
        let destination = slots.iter().find(|slot| slot.id != source.id).unwrap();
        assert_eq!(destination.state, State::Pending);
        assert_eq!(
            destination.dir.join("device.key").exists(),
            matches!(failure, Failure::PersistedMismatch),
        );
        assert!(!destination.dir.join("client.db").exists());
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn wrapped_link_never_overwrites_a_device_file() {
    let root = root();
    let registry = Registry::new(root.clone(), Layout::Android);
    let keys = WrappedKeys::default();
    let (source, owner, _) = registry.create(Storage::Wrapped, &keys).unwrap();
    keys.device_failure.set(Some(Failure::Existing));
    assert!(registry
        .link_selected_wrapped(
            &source.id,
            &owner.peer_id(),
            &incoming(),
            true,
            &keys,
            &Identity::generate(),
        )
        .is_err());
    let slots = registry.list().unwrap();
    let destination = slots.iter().find(|slot| slot.id != source.id).unwrap();
    assert_eq!(destination.state, State::Pending);
    assert_eq!(
        fs::read(destination.dir.join("device.key")).unwrap(),
        b"existing device"
    );
    assert_eq!(active(&root, Layout::Android), Active::Selected(source.id));
    fs::remove_dir_all(root).unwrap();
}
