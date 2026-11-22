use super::*;
use dyapp_identity::Identity;

const NOW: u64 = 1_700_000_000_000;

fn store(path: &std::path::Path) -> Store {
    Store::open(path, &Identity::try_generate().unwrap()).unwrap()
}

fn opening(context: &Context, store: &Store) -> Opening {
    Opening::new(
        context,
        store.owner_peer_id().to_owned(),
        Identity::try_generate().unwrap().peer_id(),
    )
}

#[test]
fn retirement_and_duplicate_callbacks_cannot_admit_another_disclosure() {
    let dir = dyapp_test_support::test_dir().unwrap();
    let store = store(&dir.path().join("owner.db"));
    store.set_send_profile_views(true).unwrap();
    let context = Context::default();
    let retired = opening(&context, &store);
    retired.retire();
    assert!(!retired.admit_at(&context, &store, NOW).unwrap());
    let admitted = opening(&context, &store);
    let peer = admitted.peer().to_owned();
    assert!(admitted.admit_at(&context, &store, NOW).unwrap());
    admitted.retire();
    assert!(!admitted.admit_at(&context, &store, NOW).unwrap());
    assert_eq!(admitted.peer(), peer);
    assert!(!store.viewed_on(store.owner_peer_id(), &peer, NOW).unwrap());
    assert!(store.known_groups().unwrap().is_empty());
}

#[test]
fn disabled_opening_does_not_become_a_future_preference_trigger() {
    let dir = dyapp_test_support::test_dir().unwrap();
    let store = store(&dir.path().join("owner.db"));
    let context = Context::default();
    let old = opening(&context, &store);
    assert!(!old.admit_at(&context, &store, NOW).unwrap());
    store.set_send_profile_views(true).unwrap();
    assert!(!old.admit_at(&context, &store, NOW).unwrap());
    let fresh = Opening::new(
        &context,
        store.owner_peer_id().to_owned(),
        old.peer().to_owned(),
    );
    assert!(fresh.admit_at(&context, &store, NOW).unwrap());
}

#[test]
fn replaced_owner_or_network_and_aba_context_are_refused() {
    let dir = dyapp_test_support::test_dir().unwrap();
    let other = store(&dir.path().join("other.db"));
    let store = store(&dir.path().join("owner.db"));
    store.set_send_profile_views(true).unwrap();
    other.set_send_profile_views(true).unwrap();
    let mut context = Context::default();
    let old = opening(&context, &store);
    assert!(matches!(
        old.admit_at(&context, &other, NOW),
        Err(Error::ContextChanged)
    ));
    assert!(matches!(
        old.admit_at(&Context::default(), &store, NOW),
        Err(Error::ContextChanged)
    ));
    context.clear();
    context.clear();
    assert!(matches!(
        old.admit_at(&context, &store, NOW),
        Err(Error::ContextChanged)
    ));
    assert!(opening(&context, &store)
        .admit_at(&context, &store, NOW)
        .unwrap());
}

#[test]
fn invalid_own_and_blocked_targets_cannot_be_admitted() {
    let dir = dyapp_test_support::test_dir().unwrap();
    let store = store(&dir.path().join("owner.db"));
    store.set_send_profile_views(true).unwrap();
    let context = Context::default();
    let owner = store.owner_peer_id().to_owned();
    let own = Opening::new(&context, owner.clone(), owner.clone());
    assert!(matches!(
        own.admit_at(&context, &store, NOW),
        Err(Error::OwnPeer)
    ));
    let invalid = Opening::new(&context, owner, "not a peer".into());
    assert!(matches!(
        invalid.admit_at(&context, &store, NOW),
        Err(Error::InvalidPeer)
    ));
    let blocked = opening(&context, &store);
    store.set_blocked(blocked.peer(), true).unwrap();
    assert!(matches!(
        blocked.admit_at(&context, &store, NOW),
        Err(Error::Store(crate::store::Error::BlockedPeer))
    ));
}
