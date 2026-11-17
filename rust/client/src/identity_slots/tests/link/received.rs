use super::wrapped::Failure;
use super::*;

fn mutation_available(root: &std::path::Path) {
    let guard = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.join("profiles.lock"))
        .unwrap();
    guard.try_lock().unwrap();
}

#[test]
fn a_received_record_never_retries_an_interrupted_destination_implicitly() {
    let root = root();
    let registry = Registry::new(root.clone(), Layout::Android);
    let keys = WrappedKeys::default();
    let (source, owner, _) = registry.create(Storage::Wrapped, &keys).unwrap();
    let mut received = ReceivedLink::new(
        &registry,
        source.id.clone(),
        owner.peer_id(),
        false,
        incoming(),
        Identity::try_generate().unwrap(),
    );
    mutation_available(&root);
    assert_eq!(registry.list().unwrap(), vec![source.clone()]);
    keys.device_failure.set(Some(Failure::Wrap));
    let LinkFailure::Destination { slot_id, detail } = received.install(&keys).unwrap_err() else {
        panic!("the failed installation must retain its destination");
    };
    let failed = (slot_id.clone(), detail.clone());
    mutation_available(&root);
    keys.device_failure.set(None);
    let LinkFailure::Destination { slot_id, detail } = received.install(&keys).unwrap_err() else {
        panic!("polling again must return the original failure");
    };
    assert_eq!((slot_id.clone(), detail.clone()), failed);
    let slots = registry.list().unwrap();
    assert_eq!(slots.len(), 2);
    assert!(slots
        .iter()
        .any(|slot| slot.id == failed.0 && slot.state == State::Pending));
    assert_eq!(active(&root, Layout::Android), Active::Selected(source.id));
    assert_eq!(*keys.generated.borrow(), [ChatKeyKind::Device]);
    drop(received);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_platform_unwind_keeps_an_unconfirmed_outcome_instead_of_retrying() {
    let root = root();
    let registry = Registry::new(root.clone(), Layout::Android);
    let keys = WrappedKeys::default();
    let (source, owner, _) = registry.create(Storage::Wrapped, &keys).unwrap();
    let mut received = ReceivedLink::new(
        &registry,
        source.id.clone(),
        owner.peer_id(),
        false,
        incoming(),
        Identity::try_generate().unwrap(),
    );
    keys.device_failure.set(Some(Failure::Panic));
    let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = received.install(&keys);
    }));
    assert!(unwound.is_err());
    mutation_available(&root);
    keys.device_failure.set(None);
    assert!(matches!(
        received.install(&keys),
        Err(LinkFailure::Unconfirmed(_))
    ));
    assert_eq!(registry.list().unwrap().len(), 2);
    assert_eq!(active(&root, Layout::Android), Active::Selected(source.id));
    assert_eq!(*keys.generated.borrow(), [ChatKeyKind::Device]);
    drop(received);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_completed_received_link_returns_its_receipt_without_another_installation() {
    let root = root();
    let registry = Registry::new(root.clone(), Layout::Android);
    let keys = WrappedKeys::default();
    let (source, owner, _) = registry.create(Storage::Wrapped, &keys).unwrap();
    let linked = incoming();
    let peer = linked.identity.peer_id();
    let mut received = ReceivedLink::new(
        &registry,
        source.id,
        owner.peer_id(),
        false,
        linked,
        Identity::try_generate().unwrap(),
    );
    let (slot, installed, _) = received.install(&keys).unwrap();
    assert_eq!(installed.peer_id(), peer);
    let id = slot.id.clone();
    mutation_available(&root);
    // The receipt describes the completed attempt even after an explicit later selection.
    let (other, _, _) = registry.create(Storage::Wrapped, &keys).unwrap();
    let before = registry.list().unwrap();
    let (slot, installed, _) = received.install(&keys).unwrap();
    assert_eq!(slot.id, id);
    assert_eq!(installed.peer_id(), peer);
    assert_eq!(registry.list().unwrap(), before);
    assert_eq!(active(&root, Layout::Android), Active::Selected(other.id));
    assert_eq!(*keys.generated.borrow(), [ChatKeyKind::Device]);
    drop(received);
    fs::remove_dir_all(root).unwrap();
}
