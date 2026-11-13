use super::*;

#[test]
fn retry_requires_confirmation_and_preserves_backoff_across_reopening() {
    let f = Fixture::new(false);
    let plan = f.plan();
    let before = rows(&f.store);
    assert_eq!(
        f.store
            .due_search_withdrawal_recovery(&plan, u64::MAX)
            .unwrap(),
        None
    );
    assert_eq!(rows(&f.store), before);
    f.store
        .confirm_search_withdrawal_recovery(&f.owner, &plan)
        .unwrap();
    let record = f.store.due_search_profile(1120).unwrap().unwrap();
    drop(f.store);
    let store = Store::open_existing(&f.path, &f.owner).unwrap();
    assert_eq!(
        store.due_search_withdrawal_recovery(&plan, 1119).unwrap(),
        None
    );
    assert_eq!(
        store.due_search_withdrawal_recovery(&plan, 1120).unwrap(),
        Some(record.clone())
    );
    store.search_profile_sent(&record, 1120).unwrap();
    assert_eq!(
        store
            .due_search_withdrawal_recovery(&plan, u64::MAX)
            .unwrap(),
        None
    );
}

#[test]
fn retry_refuses_a_newer_head_even_before_it_is_due() {
    let f = Fixture::new(false);
    let plan = f.plan();
    f.store
        .confirm_search_withdrawal_recovery(&f.owner, &plan)
        .unwrap();
    let newer = Profile {
        version: 9,
        ..Default::default()
    }
    .sign(&f.owner);
    f.store.hold_search_profile(&newer).unwrap();
    let before = rows(&f.store);
    assert!(matches!(
        f.store.due_search_withdrawal_recovery(&plan, 1001),
        Err(Error::SearchWithdrawalRecoveryChanged)
    ));
    assert_eq!(rows(&f.store), before);
    assert_eq!(f.store.due_search_profile(u64::MAX).unwrap(), Some(newer));
}

#[test]
fn retry_refuses_changed_visibility_or_completed_link_context() {
    for visibility in [true, false] {
        let f = Fixture::new(false);
        let plan = f.plan();
        f.store
            .confirm_search_withdrawal_recovery(&f.owner, &plan)
            .unwrap();
        if visibility {
            f.store.set_profile_hidden(false).unwrap();
        } else {
            f.store
                .db
                .execute(
                    "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, ?2, ?3)",
                    params!["link_enrollment", 1, vec![1u8]],
                )
                .unwrap();
        }
        let before = rows(&f.store);
        assert!(matches!(
            f.store.due_search_withdrawal_recovery(&plan, u64::MAX),
            Err(Error::SearchWithdrawalRecoveryChanged)
        ));
        assert_eq!(rows(&f.store), before);
    }
}
