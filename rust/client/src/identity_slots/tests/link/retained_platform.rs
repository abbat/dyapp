use super::*;
use zeroize::Zeroizing;

fn save_device(slot: &Slot, device: &Identity) -> Result<(), String> {
    let path = slot.dir.join("device.key");
    crate::identity_slots::write::fresh(&path, &*device.secret())?;
    let bytes = Zeroizing::new(fs::read(path).map_err(|error| error.to_string())?);
    let secret = <&[u8; 32]>::try_from(bytes.as_slice()).map_err(|_| "Saved device size")?;
    if Identity::from_secret(secret).peer_id() != device.peer_id() {
        return Err("Saved device does not reopen".into());
    }
    Ok(())
}

#[test]
fn one_retained_outcome_supports_the_existing_platform_device_writer() {
    let root = root();
    let registry = Registry::new(root.clone(), Layout::Desktop);
    let (source, owner, _) = registry.create(Storage::File, &Files).unwrap();
    let linked = incoming();
    let peer = linked.identity.peer_id();
    let mut received = ReceivedLink::new(
        &registry,
        source.id,
        owner.peer_id(),
        false,
        linked,
        Identity::generate(),
    );
    let (slot, installed, _) = received.install_with(&Files, &save_device).unwrap();
    assert_eq!(installed.peer_id(), peer);
    let id = slot.id.clone();
    let store = Store::open(&slot.dir.join("client.db"), installed).unwrap();
    assert_eq!(store.link_pending().unwrap().unwrap(), b"state");
    drop(store);
    let before = registry.list().unwrap();
    let (slot, _, _) = received
        .install_with(&Files, &|_, _| panic!("a second writer ran"))
        .unwrap();
    assert_eq!(slot.id, id);
    assert_eq!(registry.list().unwrap(), before);
    assert_eq!(active(&root, Layout::Desktop), Active::Selected(id));
    drop(received);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_platform_write_failure_never_retries_into_another_destination() {
    let root = root();
    let registry = Registry::new(root.clone(), Layout::Desktop);
    let (source, owner, _) = registry.create(Storage::File, &Files).unwrap();
    let mut received = ReceivedLink::new(
        &registry,
        source.id.clone(),
        owner.peer_id(),
        false,
        incoming(),
        Identity::generate(),
    );
    let LinkFailure::Destination { slot_id, detail } = received
        .install_with(&Files, &|_, _| Err("platform unavailable".into()))
        .unwrap_err()
    else {
        panic!("the failed platform destination must be retained");
    };
    assert_eq!(detail, "platform unavailable");
    let id = slot_id.clone();
    let LinkFailure::Destination { slot_id, .. } = received
        .install_with(&Files, &|_, _| panic!("failure was retried"))
        .unwrap_err()
    else {
        panic!("the original failure must be retained");
    };
    assert_eq!(slot_id, &id);
    assert_eq!(registry.list().unwrap().len(), 2);
    assert_eq!(active(&root, Layout::Desktop), Active::Selected(source.id));
    drop(received);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn wrapped_installation_still_refuses_desktop_before_any_reservation() {
    let root = root();
    let registry = Registry::new(root.clone(), Layout::Desktop);
    let (source, owner, _) = registry.create(Storage::File, &Files).unwrap();
    let mut received = ReceivedLink::new(
        &registry,
        source.id.clone(),
        owner.peer_id(),
        false,
        incoming(),
        Identity::generate(),
    );
    let keys = WrappedKeys::default();
    assert!(matches!(
        received.install(&keys),
        Err(LinkFailure::Admission(_))
    ));
    assert_eq!(registry.list().unwrap(), vec![source.clone()]);
    assert_eq!(active(&root, Layout::Desktop), Active::Selected(source.id));
    assert!(keys.generated.borrow().is_empty());
    drop(received);
    fs::remove_dir_all(root).unwrap();
}
