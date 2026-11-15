use super::*;
use dyapp_identity::Identity;
use dyapp_profile::Profile;
use std::path::PathBuf;

fn dir() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let next = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "dyapp-desktop-shared-context-{}-{next}",
        std::process::id()
    ));
    std::fs::create_dir_all(path.join("child")).unwrap();
    path
}

fn profile(secret: u8) -> VerifiedProfile {
    let owner = Identity::from_secret(&[secret; 32]);
    dyapp_profile::verify(
        &Profile {
            version: 1,
            ..Profile::default()
        }
        .sign(&owner),
    )
    .unwrap()
}

#[test]
fn canonical_workers_share_profiles_and_external_invalidation() {
    let dir = dir();
    let mut cache = ProfileCache::default();
    let epoch = cache.bind(&dir).unwrap();
    let alias = contact_verification::shared(&dir.join("child").join("..")).unwrap();
    assert!(Arc::ptr_eq(&cache.context, &alias));
    alias.lock().unwrap().accept(profile(2)).unwrap();
    assert_eq!(cache.snapshot().unwrap().1.all().len(), 1);
    assert!(cache.accept_if_current(epoch, profile(3)).unwrap());
    assert_eq!(alias.lock().unwrap().profiles().all().len(), 2);
    let other = contact_verification::shared(&dir.join("child")).unwrap();
    assert_eq!(other.lock().unwrap().profiles().all(), []);

    alias.lock().unwrap().clear();
    assert!(!cache.current(epoch).unwrap());
    assert!(cache.snapshot().is_err());
    assert!(!cache.accept_if_current(epoch, profile(2)).unwrap());
    let next = cache.bind(&dir.join(".")).unwrap();
    assert_ne!(next, epoch);
    assert_eq!(cache.snapshot().unwrap().1.all(), []);
    assert!(cache.accept_if_current(next, profile(2)).unwrap());
}

#[test]
fn result_finalization_holds_shared_activation_admission() {
    let dir = dir();
    let mut cache = ProfileCache::default();
    let epoch = cache.bind(&dir).unwrap();
    let alias = contact_verification::shared(&dir).unwrap();
    assert_eq!(
        cache
            .with_current(epoch, |context, _| {
                assert!(alias.try_lock().is_err());
                context.accept(profile(2)).unwrap();
                7
            })
            .unwrap(),
        Some(7)
    );
    alias.lock().unwrap().clear();
    assert_eq!(
        cache
            .with_current::<u8>(epoch, |_, _| {
                panic!("stale result finalized");
            })
            .unwrap(),
        None
    );
}

#[test]
fn lifecycle_clear_invalidates_tickets_seen_through_an_alias() {
    let dir = dir();
    let state = SearchState::default();
    let owner = Identity::from_secret(&[1; 32]);
    let store = dyapp_client::store::Store::open(&dir.join("client.db"), &owner).unwrap();
    let fetched = profile(2);
    let epoch = state.0.lock().unwrap().bind(&dir).unwrap();
    let alias = contact_verification::shared(&dir.join(".")).unwrap();
    let ticket = alias
        .lock()
        .unwrap()
        .begin(&store, &fetched.peer_id)
        .unwrap();
    assert!(state
        .0
        .lock()
        .unwrap()
        .accept_if_current(epoch, fetched.clone())
        .unwrap());
    state.clear().unwrap();
    assert!(!state.0.lock().unwrap().current(epoch).unwrap());
    assert_eq!(state.0.lock().unwrap().snapshot().unwrap().1.all(), []);
    assert!(matches!(
        alias.lock().unwrap().confirm(&store, ticket, fetched),
        Err(contact_verification::Error::ContextChanged)
    ));
}

#[test]
fn epoch_exhaustion_fails_closed_instead_of_reviving_old_work() {
    let dir = dir();
    let mut cache = ProfileCache::default();
    cache.bind(&dir).unwrap();
    cache.epoch = u64::MAX;
    assert!(cache.accept_if_current(u64::MAX, profile(2)).unwrap());
    assert_eq!(cache.clear(), Err("context epoch exhausted".into()));
    assert!(!cache.current(u64::MAX).unwrap());
    assert!(!cache.accept_if_current(u64::MAX, profile(2)).unwrap());
    assert!(cache.snapshot().is_err());
    assert_eq!(cache.bind(&dir), Err("context epoch exhausted".into()));
}

#[test]
fn poisoned_authority_returns_errors_without_admission() {
    let dir = dir();
    let mut cache = ProfileCache::default();
    let epoch = cache.bind(&dir).unwrap();
    let context = Arc::clone(&cache.context);
    assert!(std::thread::spawn(move || {
        let _guard = context.lock().unwrap();
        panic!("controlled context fault");
    })
    .join()
    .is_err());
    assert!(cache.snapshot().is_err());
    assert!(cache.current(epoch).is_err());
    assert!(cache.accept_if_current(epoch, profile(2)).is_err());
    assert!(cache.clear().is_err());
    assert!(cache.bind(&dir).is_err());
}
