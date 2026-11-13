use super::*;

#[tokio::test]
async fn legacy_pending_hide_requires_review_before_any_network_request() {
    let owner = Identity::from_secret(&[7; 32]);
    let store = Store::open(&crate::store::tests::temp_db_path(), &owner).unwrap();
    let legacy = Profile::tombstone(2, 0, 0).sign(&owner);
    store.hold_search_profile(&legacy).unwrap();
    let before = store.link_state().unwrap();
    let (node, asked) = fake(|_| panic!("legacy Hide must not be sent")).await;
    assert!(matches!(
        publish_due(&store, &mut [node], 1000).await,
        Err(store::Error::SearchWithdrawalReviewRequired)
    ));
    assert!(asked.lock().unwrap().is_empty());
    assert_eq!(store.due_search_profile(1000).unwrap(), Some(legacy));
    assert_eq!(store.link_state().unwrap(), before);
}

#[tokio::test]
async fn unsupported_search_withdrawal_preserves_the_queue_without_downgrading() {
    let owner = Identity::from_secret(&[7; 32]);
    let store = Store::open(&crate::store::tests::temp_db_path(), &owner).unwrap();
    let hidden = Profile::search_tombstone(2, 0, 0).sign(&owner);
    store.hold_search_profile(&hidden).unwrap();
    let before = store.link_state().unwrap();
    let (node, asked) = fake(|request| {
        assert!(matches!(request, Request::Withdraw(_)));
        status(Status::Unsupported, 0)
    })
    .await;
    assert!(!publish_due(&store, &mut [node], 1000).await.unwrap());
    assert_eq!(
        asked.lock().unwrap().as_slice(),
        &[Request::Withdraw(hidden.clone())]
    );
    assert_eq!(store.due_search_profile(1000).unwrap(), Some(hidden));
    assert_eq!(store.link_state().unwrap(), before);
}

#[tokio::test]
async fn search_withdrawal_respects_server_backoff_across_reopening() {
    let owner = Identity::from_secret(&[7; 32]);
    let path = crate::store::tests::temp_db_path();
    let store = Store::open(&path, &owner).unwrap();
    let hidden = Profile::search_tombstone(2, 0, 0).sign(&owner);
    store.hold_search_profile(&hidden).unwrap();
    let mut replies = [Status::RateLimited, Status::Ok].into_iter();
    let (node, asked) = fake(move |request| {
        assert!(matches!(request, Request::Withdraw(_)));
        status(
            replies
                .next()
                .expect("no request before the retry deadline"),
            120,
        )
    })
    .await;
    let mut nodes = vec![node];
    assert!(!publish_due(&store, &mut nodes, 1000).await.unwrap());
    drop(store);
    let store = Store::open_existing(&path, &owner).unwrap();
    assert_eq!(store.search_profile_minutes_left(1000).unwrap(), Some(2));
    assert_eq!(store.due_search_profile(1119).unwrap(), None);
    assert!(!publish_due(&store, &mut nodes, 1119).await.unwrap());
    assert_eq!(asked.lock().unwrap().len(), 1);
    assert_eq!(
        store.due_search_profile(1120).unwrap(),
        Some(hidden.clone())
    );
    assert!(publish_due(&store, &mut nodes, 1120).await.unwrap());
    assert_eq!(
        asked.lock().unwrap().as_slice(),
        &[Request::Withdraw(hidden.clone()), Request::Withdraw(hidden)]
    );
    assert_eq!(store.due_search_profile(u64::MAX).unwrap(), None);
}

async fn source(record: SignedRecord) -> Node {
    let owner = dyapp_identity::sha256(&record.public_key).to_vec();
    let version = dyapp_profile::verify_search(&record)
        .unwrap()
        .profile
        .version;
    fake(move |request| match request {
        Request::Query(_) => ok(vec![hit(owner.clone(), version)], Vec::new(), false),
        Request::Get(_) => ok(Vec::new(), vec![record.clone()], false),
        _ => panic!("not a discovery read"),
    })
    .await
    .0
}

#[tokio::test]
async fn search_withdrawal_suppresses_an_older_cached_hit_without_revoking_ordinary_authority() {
    let owner = Identity::from_secret(&[7; 32]);
    let public = record(&owner, 1, 30);
    let hidden = Profile::search_tombstone(2, 0, 0).sign(&owner);
    let mut cache = Profiles::default();
    cache
        .accept(dyapp_profile::verify(&public).unwrap())
        .unwrap();
    let mut nodes = vec![source(public).await, source(hidden).await];
    let result = search(&mut nodes, &[], 0, 0, &cache, &[]).await;
    assert_eq!(result.profiles.len(), 0);
    assert_eq!(result.failures.len(), 0);
    assert!(!result.partial);
    let held = cache.get(&owner.peer_id()).unwrap();
    assert_eq!(held.profile.version, 1);
    assert!(!held.profile.deleted);
}

#[test]
fn equal_version_withdrawal_wins_in_both_record_orders() {
    let owner = Identity::from_secret(&[7; 32]);
    let public = record(&owner, 2, 30);
    let hidden = Profile::search_tombstone(2, 0, 0).sign(&owner);
    for records in [
        vec![public.clone(), hidden.clone()],
        vec![hidden.clone(), public.clone()],
    ] {
        assert_eq!(search_results::check(&[], 0, &[records]).len(), 0);
    }
    let newer = record(&owner, 3, 30);
    let result = search_results::check(&[], 0, &[vec![hidden, newer]]);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].profile.version, 3);
}

#[test]
fn ordinary_profile_cache_refuses_search_only_verified_records() {
    let owner = Identity::from_secret(&[7; 32]);
    let mut cache = Profiles::default();
    cache
        .accept(dyapp_profile::verify(&record(&owner, 1, 30)).unwrap())
        .unwrap();
    let hidden =
        dyapp_profile::verify_search(&Profile::search_tombstone(2, 0, 0).sign(&owner)).unwrap();
    let result = cache.accept(hidden).map(|held| held.profile.version);
    assert!(matches!(result, Err(Error::Input(_))));
    let held = cache.get(&owner.peer_id()).unwrap();
    assert_eq!(held.profile.version, 1);
    assert!(!held.profile.deleted);
}
