//! Real native searchers share authority without retaining its mutex during network work.
use super::*;
use dyapp_identity::Identity;
use dyapp_profile::Profile;

fn key(secret: u8) -> Arc<IdentityKey> {
    Arc::new(IdentityKey {
        identity: Identity::from_secret(&[secret; 32]),
    })
}

fn profile(name: &str, version: u64) -> VerifiedProfile {
    dyapp_profile::verify(
        &Profile {
            version,
            name: name.into(),
            ..Default::default()
        }
        .sign(&Identity::from_secret(&[2; 32])),
    )
    .unwrap()
}

#[test]
fn native_workers_share_current_names_clear_together_and_isolate_other_identities() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let dir = test_dir.to_path_buf();
    let owner = key(1);
    let first = ProfileSearcher::new(dir.to_str().unwrap().into()).unwrap();
    let second = ProfileSearcher::new(dir.join(".").to_str().unwrap().into()).unwrap();
    assert!(Arc::ptr_eq(&first.cache, &second.cache));
    let current = profile("Current", 3);
    let peer = current.peer_id.clone();
    let store = Store::open(&dir.join("client.db"), &owner.identity).unwrap();
    store.set_verified(&current).unwrap();
    first.cache.lock().unwrap().accept(current).unwrap();
    let read = |worker: &ProfileSearcher| {
        worker
            .contact_names(owner.clone(), vec![peer.clone()])
            .unwrap()
            .remove(0)
    };
    assert_eq!(read(&second).name, "Current");
    assert_eq!(read(&second).verified, Some(true));
    let other_dir = test_dir.path().join("other");
    std::fs::create_dir_all(&other_dir).unwrap();
    let other = ProfileSearcher::new(other_dir.to_str().unwrap().into()).unwrap();
    assert_ne!(owner.peer_id(), key(3).peer_id());
    assert_eq!(
        other
            .contact_names(key(3), vec![peer.clone()])
            .unwrap()
            .remove(0)
            .name,
        String::new()
    );
    dyapp_client::contact_verification::shared(&dir)
        .unwrap()
        .lock()
        .unwrap()
        .clear();
    for worker in [&first, &second] {
        assert_eq!(read(worker).name, String::new());
        assert_eq!(read(worker).verified, None);
    }
    second
        .cache
        .lock()
        .unwrap()
        .accept(profile("Refreshed", 4))
        .unwrap();
    assert_eq!(read(&first).name, "Refreshed");
    assert_eq!(read(&second).name, "Refreshed");
}

#[test]
fn a_context_change_during_fetch_rejects_all_late_results() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let dir = test_dir.to_path_buf();
    let worker = ProfileSearcher::new(dir.to_str().unwrap().into()).unwrap();
    let shared = Arc::clone(&worker.cache);
    let result = worker.fetch_and_cache(|| {
        shared.lock().unwrap().clear();
        Ok(vec![profile("Late", 3)])
    });
    assert!(result.unwrap_err().to_string().contains("notConnected"));
    assert_eq!(shared.lock().unwrap().profiles().all(), []);
}

#[test]
fn concurrent_newer_authority_is_neither_replaced_nor_projected_as_the_old_result() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let dir = test_dir.to_path_buf();
    let worker = ProfileSearcher::new(dir.to_str().unwrap().into()).unwrap();
    let shared = Arc::clone(&worker.cache);
    let newer = profile("Newer", 4);
    let peer = newer.peer_id.clone();
    let result = worker
        .fetch_and_cache(|| {
            shared.lock().unwrap().accept(newer).unwrap();
            Ok(vec![profile("Older", 3)])
        })
        .unwrap();
    assert!(result.is_empty());
    assert_eq!(
        shared
            .lock()
            .unwrap()
            .profiles()
            .get(&peer)
            .unwrap()
            .profile
            .name,
        "Newer"
    );
}

#[test]
fn concurrent_tombstones_prevent_a_later_live_result_from_reappearing() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let dir = test_dir.to_path_buf();
    let worker = ProfileSearcher::new(dir.to_str().unwrap().into()).unwrap();
    let owner = Identity::from_secret(&[2; 32]);
    let source = Profile {
        name: "Current".into(),
        version: 3,
        ..Default::default()
    }
    .sign(&owner);
    let peer = owner.peer_id();
    worker
        .cache
        .lock()
        .unwrap()
        .accept(dyapp_profile::verify(&source).unwrap())
        .unwrap();
    let deletion = Profile {
        previous: Some(dyapp_profile::ProfileCondition::observed(Some(&source)).unwrap()),
        ..Profile::tombstone(4, 0, 0)
    };
    let tombstone = dyapp_profile::verify(&deletion.sign(&owner)).unwrap();
    let result = worker
        .fetch_and_cache(|| {
            worker.cache.lock().unwrap().accept(tombstone).unwrap();
            Ok(vec![profile("Resurrected", 5)])
        })
        .unwrap();
    assert!(result.is_empty());
    assert!(
        worker
            .cache
            .lock()
            .unwrap()
            .profiles()
            .get(&peer)
            .unwrap()
            .profile
            .deleted
    );
}

#[test]
fn bulk_fetch_releases_the_context_guard_and_preserves_failed_fetch_authority() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let dir = test_dir.to_path_buf();
    let worker = ProfileSearcher::new(dir.to_str().unwrap().into()).unwrap();
    let shared = dyapp_client::contact_verification::shared(&dir).unwrap();
    let fetched = profile("Current", 3);
    let peer = fetched.peer_id.clone();
    let result = worker
        .fetch_and_cache(|| {
            assert!(
                shared.try_lock().is_ok(),
                "network work holds the context mutex"
            );
            Ok(vec![fetched])
        })
        .unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(
        shared
            .lock()
            .unwrap()
            .profiles()
            .get(&peer)
            .unwrap()
            .profile
            .name,
        "Current"
    );
    assert!(worker
        .fetch_and_cache(|| {
            assert!(
                shared.try_lock().is_ok(),
                "network work holds the context mutex"
            );
            Err(CoreError::Failed("controlled fetch failure".into()))
        })
        .is_err());
    assert_eq!(
        shared
            .lock()
            .unwrap()
            .profiles()
            .get(&peer)
            .unwrap()
            .profile
            .version,
        3
    );
}
