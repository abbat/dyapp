use super::wrapped::Failure;
use super::*;

fn check_marker(
    failure: Failure,
    root: &std::path::Path,
    registry: &Registry,
    keys: &WrappedKeys,
    destination: &Slot,
    peer: &str,
) {
    match failure {
        Failure::BlockPending => {
            let identity = keys.open(destination, None).unwrap().unwrap();
            let store = Store::open(&destination.dir.join("client.db"), &identity).unwrap();
            assert_eq!(store.link_pending().unwrap(), None);
            assert!(!store.is_linked_device().unwrap());
            assert!(destination.dir.join("device.key").is_file());
        }
        Failure::BlockReady => {
            assert_eq!(
                fs::read(destination.dir.join("state.tmp")).unwrap(),
                b"retained"
            );
        }
        Failure::BlockSelection => {
            assert_eq!(
                fs::read(root.join("active-profile.tmp")).unwrap(),
                b"retained"
            );
            let Resolution::Ready(_, reopened) = registry.inspect(&destination.id, keys).unwrap()
            else {
                panic!("the unselected ready destination must remain recoverable");
            };
            assert_eq!(reopened.peer_id(), peer);
        }
        _ => {}
    }
}

#[test]
fn wrapped_link_separates_source_rejection_from_unconfirmed_reservation() {
    let root = root();
    let registry = Registry::new(root.clone(), Layout::Android);
    let keys = WrappedKeys::default();
    let (source, owner, _) = registry.create(Storage::Wrapped, &keys).unwrap();
    let linked = incoming();
    let device = Identity::generate();
    let rejected =
        registry.link_selected_wrapped(&source.id, "wrong owner", &linked, true, &keys, &device);
    assert!(matches!(rejected, Err(LinkFailure::Admission(_))));
    assert_eq!(registry.list().unwrap(), vec![source.clone()]);
    keys.reservation_failure.set(true);
    let interrupted =
        registry.link_selected_wrapped(&source.id, &owner.peer_id(), &linked, true, &keys, &device);
    assert!(
        matches!(interrupted, Err(LinkFailure::Reservation(ref detail)) if detail == "reservation denied")
    );
    assert_eq!(registry.list().unwrap(), vec![source.clone()]);
    assert_eq!(active(&root, Layout::Android), Active::Selected(source.id));
    assert_eq!(linked.state, b"state");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn known_destination_failures_report_the_exact_slot_without_claiming_rollback() {
    for (failure, expected_state, stored_state) in [
        (Failure::Wrap, State::Pending, false),
        (Failure::BlockStore, State::Pending, false),
        (Failure::BlockPending, State::Pending, false),
        (Failure::BlockReady, State::Pending, true),
        (Failure::BlockSelection, State::Ready, true),
    ] {
        let root = root();
        let registry = Registry::new(root.clone(), Layout::Android);
        let keys = WrappedKeys::default();
        let (source, owner, _) = registry.create(Storage::Wrapped, &keys).unwrap();
        keys.device_failure.set(Some(failure));
        let linked = incoming();
        let linked_peer = linked.identity.peer_id();
        let device = Identity::generate();
        let result = registry.link_selected_wrapped(
            &source.id,
            &owner.peer_id(),
            &linked,
            true,
            &keys,
            &device,
        );
        let LinkFailure::Destination { slot_id, detail } = result.unwrap_err() else {
            panic!("a reserved destination must be reported");
        };
        assert_ne!(detail, "");
        if matches!(failure, Failure::BlockPending) {
            assert!(detail.contains("pending insert denied"), "{detail}");
        }
        let slots = registry.list().unwrap();
        assert_eq!(slots.len(), 2);
        let destination = slots.iter().find(|slot| slot.id == slot_id).unwrap();
        assert_ne!(destination.id, source.id);
        assert_eq!(destination.state, expected_state);
        assert_eq!(
            active(&root, Layout::Android),
            Active::Selected(source.id.clone())
        );
        assert_eq!(linked.identity.peer_id(), linked_peer);
        assert_eq!(linked.state, b"state");
        if stored_state {
            let store = Store::open(&destination.dir.join("client.db"), &linked.identity).unwrap();
            assert_eq!(store.link_pending().unwrap().unwrap(), linked.state);
        }
        check_marker(failure, &root, &registry, &keys, destination, &linked_peer);
        fs::remove_dir_all(root).unwrap();
    }
}
