use super::{Failure, RecoveryState, Saved};
use dyapp_client::store::Store;
use dyapp_identity::Identity;
use dyapp_profile::Profile;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture {
    root: PathBuf,
    owner: Identity,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "dyapp-search-recovery-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        // Exclusive creation refuses collisions rather than reusing another test's files.
        std::fs::create_dir(&root).unwrap();
        let owner = Identity::from_secret(&[7; 32]);
        let store = Store::open(&root.join("client.db"), &owner).unwrap();
        store
            .search_profile_sent(
                &Profile {
                    version: 3,
                    ..Profile::default()
                }
                .sign(&owner),
                1000,
            )
            .unwrap();
        store
            .save_profile_draft(&Profile {
                version: 4,
                name: "private draft".into(),
                ..Profile::default()
            })
            .unwrap();
        store.set_profile_hidden(true).unwrap();
        store.own_profile_age(7, 1000).unwrap();
        store
            .hold_search_profile(&Profile::tombstone(4, 0, 0).sign(&owner))
            .unwrap();
        Self { root, owner }
    }

    fn store(&self) -> Store {
        Store::open_existing(&self.root.join("client.db"), &self.owner).unwrap()
    }

    fn prepare(&self, reviews: &RecoveryState) -> String {
        reviews
            .prepare(&self.owner, &self.root, 1)
            .unwrap()
            .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // The exclusive fixture root owns every file; no real installation is opened.
        std::fs::remove_dir_all(&self.root).unwrap();
    }
}

#[test]
fn fresh_profile_has_no_recovery_and_creates_no_database() {
    let fixture = Fixture::new();
    let reviews = RecoveryState::default();
    let old = fixture.prepare(&reviews);
    let fresh = fixture.root.join("fresh");
    std::fs::create_dir(&fresh).unwrap();
    assert_eq!(reviews.prepare(&fixture.owner, &fresh, 1).unwrap(), None);
    assert_eq!(std::fs::read_dir(&fresh).unwrap().count(), 0);
    assert_eq!(
        reviews.confirm(&fixture.owner, &fixture.root, 1, &old),
        Err(Failure::Closed)
    );
    let path = fresh.join("client.db");
    std::fs::write(&path, b"not a database").unwrap();
    assert!(matches!(
        reviews.prepare(&fixture.owner, &fresh, 1),
        Err(Failure::Failed { .. })
    ));
    assert_eq!(std::fs::read(path).unwrap(), b"not a database");
}

#[test]
fn prepare_and_unconfirmed_retry_do_not_sign_or_send_and_old_dismiss_is_scoped() {
    let f = Fixture::new();
    let reviews = RecoveryState::default();
    let store = f.store();
    let draft = store.profile_draft().unwrap();
    let pending = store.due_search_profile(1000).unwrap();
    assert!(pending.is_some());
    let old = f.prepare(&reviews);
    let current = f.prepare(&reviews);
    assert_ne!(old, current);
    assert_eq!(
        reviews.confirm(&f.owner, &f.root, 1, &old),
        Err(Failure::Closed)
    );
    reviews.dismiss(&old).unwrap();
    reviews
        .retry(&f.owner, &f.root, 1, &current, 1000, |_, _| {
            panic!("Unconfirmed review cannot deliver");
        })
        .unwrap();
    assert_eq!(store.profile_draft().unwrap(), draft);
    assert_eq!(store.due_search_profile(1000).unwrap(), pending);
    assert_eq!(
        reviews
            .confirm(&f.owner, &f.root, 1, &current)
            .unwrap()
            .version,
        8
    );
    reviews.dismiss(&current).unwrap();
    assert_eq!(
        reviews.confirm(&f.owner, &f.root, 1, &current),
        Err(Failure::Closed)
    );
}

#[test]
fn wrong_owner_directory_and_generation_never_mutate_or_open_a_successor() {
    let f = Fixture::new();
    let reviews = RecoveryState::default();
    let token = f.prepare(&reviews);
    let before = f.store().profile_draft().unwrap();
    let other = Identity::from_secret(&[8; 32]);
    assert_eq!(
        reviews.confirm(&other, &f.root, 1, &token),
        Err(Failure::WrongOwner)
    );
    let absent = f.root.join("absent");
    assert_eq!(
        reviews.confirm(&f.owner, &absent, 1, &token),
        Err(Failure::Changed)
    );
    assert!(!absent.exists());
    assert_eq!(
        reviews.confirm(&f.owner, &f.root, 2, &token),
        Err(Failure::Changed)
    );
    assert_eq!(f.store().profile_draft().unwrap(), before);
    assert_eq!(
        reviews
            .confirm(&f.owner, &f.root, 1, &token)
            .unwrap()
            .version,
        8
    );
}

#[test]
fn confirmed_delivery_failure_retains_bytes_and_retry_never_advances_the_draft() {
    let f = Fixture::new();
    let reviews = RecoveryState::default();
    let token = f.prepare(&reviews);
    let saved = reviews.confirm(&f.owner, &f.root, 1, &token).unwrap();
    let store = f.store();
    let record = store.due_search_profile(1000).unwrap().unwrap();
    assert_eq!(
        reviews.retry(&f.owner, &f.root, 1, &token, 1000, |_, _| {
            Err("controlled transport failure".into())
        }),
        Err(Failure::Failed {
            reason: "controlled transport failure".into()
        })
    );
    assert_eq!(
        store.due_search_profile(1000).unwrap(),
        Some(record.clone())
    );
    let mut sent = false;
    let acknowledged = reviews
        .retry(&f.owner, &f.root, 1, &token, 1000, |db, plan| {
            assert_eq!(
                db.due_search_withdrawal_recovery(plan, 1000).unwrap(),
                Some(record.clone())
            );
            db.search_profile_sent(&record, 1000).unwrap();
            sent = true;
            Ok(())
        })
        .unwrap();
    assert!(sent);
    assert_eq!(
        acknowledged,
        Saved {
            version: saved.version,
            hidden: true,
            search_pending: false
        }
    );
    reviews
        .retry(&f.owner, &f.root, 1, &token, 1000, |_, _| {
            panic!("Acknowledged review cannot deliver again");
        })
        .unwrap();
    assert_eq!(store.profile_draft().unwrap().unwrap().version, 8);
}

#[test]
fn changed_core_review_returns_a_typed_refusal_without_overwriting_local_edits() {
    let f = Fixture::new();
    let reviews = RecoveryState::default();
    let token = f.prepare(&reviews);
    let store = f.store();
    let mut edited = store.profile_draft().unwrap().unwrap();
    edited.name = "newer local edit".into();
    store.save_profile_draft(&edited).unwrap();
    let failure = reviews.confirm(&f.owner, &f.root, 1, &token).unwrap_err();
    assert_eq!(
        serde_json::to_value(failure).unwrap(),
        serde_json::json!({"kind": "changed"})
    );
    assert_eq!(store.profile_draft().unwrap(), Some(edited));
}
