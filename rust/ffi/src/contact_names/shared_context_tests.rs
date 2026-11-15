//! Real native searchers share core authority and retain the activation guard during fetch.
use super::*;
use dyapp_identity::Identity;
use dyapp_profile::Profile;
use std::path::PathBuf;

fn directory() -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../build/native-contact-context-tests")
        .join(Identity::generate().peer_id());
    std::fs::create_dir_all(&path).unwrap();
    path
}

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
    let dir = directory();
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
    let other_dir = directory();
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
fn bulk_fetch_holds_the_activation_guard_through_result_admission() {
    let dir = directory();
    let worker = ProfileSearcher::new(dir.to_str().unwrap().into()).unwrap();
    let shared = dyapp_client::contact_verification::shared(&dir).unwrap();
    let fetched = profile("Current", 3);
    let peer = fetched.peer_id.clone();
    let result = worker
        .fetch_and_cache(|| {
            assert!(matches!(
                shared.try_lock(),
                Err(std::sync::TryLockError::WouldBlock)
            ));
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
            assert!(matches!(
                shared.try_lock(),
                Err(std::sync::TryLockError::WouldBlock)
            ));
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
