use super::*;

#[test]
fn remembered_withdrawal_is_distinct_from_foreign_equal_version_tombstone() {
    let identity = Identity::from_secret(&[7; 32]);
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    let mut draft = Profile {
        version: 7,
        ..Default::default()
    };
    assert!(work(&mut draft, &identity.public_key(), 1, 1024));
    store.save_profile_draft(&draft).unwrap();
    store.set_profile_hidden(true).unwrap();
    let own = Profile::tombstone(7, 0, 0);
    let foreign = Profile::tombstone(7, 1, 0);
    store.hold_search_profile(&own.sign(&identity)).unwrap();
    store
        .search_profile_sent(&own.sign(&identity), 1000)
        .unwrap();
    assert!(store.search_profile_known(&own.sign(&identity)).unwrap());
    assert!(!store
        .search_profile_known(&foreign.sign(&identity))
        .unwrap());
    assert!(!crate::publish::from_another_device(&store, &identity, &own, &draft).unwrap());
    assert!(crate::publish::from_another_device(&store, &identity, &foreign, &draft).unwrap());
    let legacy = Profile::tombstone(7, draft.pow_nonce, draft.pow_bits);
    assert!(!crate::publish::from_another_device(&store, &identity, &legacy, &draft).unwrap());
}
