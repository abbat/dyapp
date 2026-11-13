use super::*;

#[test]
fn recovery_preserves_stronger_pending_work_and_current_private_fields() {
    let f = Fixture::new(false);
    let nonce = dyapp_profile::solve(&f.owner.public_key(), 1, 0, 10_000).unwrap();
    let stronger = Profile::tombstone(5, nonce, 1).sign(&f.owner);
    f.store.hold_search_profile(&stronger).unwrap();
    let mut expected = f.store.profile_draft().unwrap().unwrap();
    let plan = f.plan();
    expected.version = 8;
    expected.pow_nonce = nonce;
    expected.pow_bits = 1;
    assert_eq!(
        f.store
            .confirm_search_withdrawal_recovery(&f.owner, &plan)
            .unwrap(),
        expected
    );
    let record = f.store.due_search_profile(1120).unwrap().unwrap();
    let profile = dyapp_profile::verify_search(&record).unwrap().profile;
    assert_eq!((profile.pow_nonce, profile.pow_bits), (nonce, 1));
    assert_eq!(
        f.store
            .confirm_search_withdrawal_recovery(&f.owner, &plan)
            .unwrap(),
        expected
    );
}
