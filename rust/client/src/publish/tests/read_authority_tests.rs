use super::*;

#[tokio::test]
async fn a_legacy_hide_reply_is_not_ordinary_profile_authority() {
    let owner = Identity::from_secret(&[7; 32]);
    let captured = Profile::tombstone(2, 0, 0).sign(&owner);
    let (mut node, held) = profile_node(Vec::new()).await;
    *held.lock().unwrap() = Some(captured);
    assert!(matches!(
        node.get_profile(&owner.peer_id()).await,
        Err(Error::Profile(dyapp_profile::Error::Invalid(_)))
    ));
}

#[test]
fn a_legacy_hide_cannot_replace_the_ordinary_profile_cache() {
    let owner = Identity::from_secret(&[7; 32]);
    let mut cache = crate::profiles::Profiles::default();
    let source = Profile {
        version: 1,
        ..Default::default()
    }
    .sign(&owner);
    cache
        .accept(dyapp_profile::verify(&source).unwrap())
        .unwrap();
    let captured = Profile::tombstone(2, 0, 0).sign(&owner);
    let historical = dyapp_profile::verify_search(&captured).unwrap();
    assert!(cache.accept(historical).is_err());
    let retained = cache.get(&owner.peer_id()).unwrap();
    assert_eq!(retained.profile.version, 1);
    assert!(!retained.profile.deleted);
}
