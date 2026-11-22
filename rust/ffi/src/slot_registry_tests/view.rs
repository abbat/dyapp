use super::*;

#[test]
fn native_profile_switch_away_and_back_does_not_reauthorize_the_old_view() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let registry = MobileIdentityRegistry::new(
        test_dir.to_path_buf().to_str().unwrap().into(),
        MobileSlotLayout::Android,
        Arc::new(Credentials::default()),
    );
    let first = registry.create().unwrap();
    let token = registry.view.current().unwrap().generation();
    let second = registry.create().unwrap();
    registry.select(first.slot.id.clone()).unwrap();
    assert!(registry.view.admit(&token).is_err());
    let selected = registry.resolve().unwrap();
    assert!(
        matches!(selected, MobileSlotResolution::Ready { key, .. } if key.peer_id() == first.key.peer_id())
    );
    let current = registry.view.current().unwrap().generation();
    assert!(registry.view.admit(&current).is_ok());
    let before_remove = registry.view.current().unwrap().generation();
    registry
        .remove(second.slot.id, Some(second.key.peer_id()))
        .unwrap();
    assert!(registry.view.admit(&before_remove).is_err());
}

#[test]
fn a_failed_native_selection_invalidates_the_prior_view_without_changing_owner() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let registry = MobileIdentityRegistry::new(
        test_dir.to_path_buf().to_str().unwrap().into(),
        MobileSlotLayout::Android,
        Arc::new(Credentials::default()),
    );
    let first = registry.create().unwrap();
    let token = registry.view.current().unwrap().generation();
    assert!(registry.select("not a slot".into()).is_err());
    assert!(registry.view.admit(&token).is_err());
    assert!(
        matches!(registry.resolve().unwrap(), MobileSlotResolution::Ready { key, .. } if key.peer_id() == first.key.peer_id())
    );
}
