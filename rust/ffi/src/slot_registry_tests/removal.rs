use super::Credentials;
use crate::{
    identity_removal_allowed, MobileIdentityRegistry, MobileSlotLayout, MobileSlotRecovery,
};
use std::sync::{atomic::Ordering, Arc};

#[test]
fn exported_removal_review_preserves_selection_and_credentials_and_checks_displayed_peer() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let root = test_dir.to_path_buf();
    let keys = Arc::new(Credentials::default());
    let registry = MobileIdentityRegistry::new(
        root.to_str().unwrap().to_owned(),
        MobileSlotLayout::Android,
        keys.clone(),
    );
    let created = registry.create().unwrap();
    let peer = created.key.peer_id();
    let selected = std::fs::read(root.join("active-profile")).unwrap();
    let review = registry
        .removal_review(created.slot.id.clone(), Some(peer))
        .unwrap();
    assert!(review.allowed && !review.has_other_copy);
    assert!(registry
        .removal_review(created.slot.id.clone(), Some("wrong-peer".into()))
        .is_err());
    assert!(registry
        .removal_review(created.slot.id.clone(), None)
        .is_err());
    assert!(identity_removal_allowed(
        Some(created.slot.id.clone()),
        None,
        true
    ));
    assert!(!identity_removal_allowed(
        Some("legacy-primary".into()),
        None,
        true
    ));
    assert!(!identity_removal_allowed(
        Some(created.slot.id.clone()),
        Some(MobileSlotRecovery::Unavailable),
        true
    ));
    assert_eq!(keys.erasures.load(Ordering::SeqCst), 0);
    assert!(keys.aliases.lock().unwrap().contains(&created.slot.id));
    assert_eq!(
        std::fs::read(root.join("active-profile")).unwrap(),
        selected
    );
}
