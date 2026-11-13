//! Exercise the common desktop review state without building a platform UI SDK.
#[path = "../../../clients/desktop/identity_gate.rs"]
mod identity_gate;
#[path = "../../../clients/desktop/search_recovery_state.rs"]
mod search_recovery_state;

use dyapp_client::store::Store;
use dyapp_identity::Identity;
use dyapp_profile::Profile;
use identity_gate::{IdentityGate, Lease};
use search_recovery_state::RecoveryState;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

type Active = (Arc<Identity>, PathBuf);
struct Fixture {
    key: Arc<Identity>,
    dir: PathBuf,
    gate: Arc<IdentityGate<Active>>,
    reviews: RecoveryState,
}
impl Fixture {
    fn new() -> Self {
        let key = Arc::new(Identity::generate());
        let dir = std::env::temp_dir().join(format!("dyapp-desktop-search-{}", key.peer_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(&dir.join("client.db"), &key).unwrap();
        store
            .save_profile_draft(&Profile {
                version: 2,
                name: "private draft".into(),
                ..Default::default()
            })
            .unwrap();
        store.set_profile_hidden(true).unwrap();
        store
            .hold_search_profile(&Profile::tombstone(2, 0, 0).sign(&key))
            .unwrap();
        Self {
            gate: IdentityGate::new(Some((Arc::clone(&key), dir.clone()))),
            key,
            dir,
            reviews: RecoveryState::default(),
        }
    }
    fn store(&self) -> Store {
        Store::open_existing(&self.dir.join("client.db"), &self.key).unwrap()
    }
    fn lease(&self) -> Lease<Active> {
        self.gate.lease().unwrap()
    }
    fn prepare(&self) -> String {
        let lease = self.lease();
        let (key, dir) = lease.active();
        self.reviews
            .prepare(key, dir, lease.generation())
            .unwrap()
            .unwrap()
    }
}

fn switch_selection(gate: &Arc<IdentityGate<Active>>, active: Active) {
    let transition = gate.start_transition().unwrap();
    let (done, ready) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let result = transition
            .wait()
            .and_then(|()| transition.commit(Some(active)));
        done.send(result).unwrap();
    });
    ready
        .recv_timeout(Duration::from_secs(30))
        .expect("idle review blocked identity transition")
        .unwrap();
    worker.join().unwrap();
}

#[test]
fn idle_review_retains_no_key_or_lease_and_refuses_a_new_identity_generation() {
    let f = Fixture::new();
    let token = f.prepare();
    assert_eq!(Arc::strong_count(&f.key), 2);
    switch_selection(&f.gate, (Arc::new(Identity::generate()), f.dir.clone()));
    let lease = f.lease();
    let (key, dir) = lease.active();
    assert!(f
        .reviews
        .confirm(key, dir, lease.generation(), &token)
        .is_err());
    assert!(f
        .reviews
        .retry(key, dir, lease.generation(), &token, 1000, |_, _| panic!(
            "changed owner must not send"
        ))
        .is_err());
    drop(lease);
    switch_selection(&f.gate, (Arc::clone(&f.key), f.dir.clone()));
    let lease = f.lease();
    assert!(f
        .reviews
        .confirm(&f.key, &f.dir, lease.generation(), &token)
        .is_err());
    assert_eq!(f.store().profile_draft().unwrap().unwrap().version, 2);
}

#[test]
fn dismissed_or_replaced_handles_cannot_confirm_or_dismiss_a_new_review() {
    let f = Fixture::new();
    let old = f.prepare();
    let current = f.prepare();
    assert_ne!(old, current);
    f.reviews.dismiss(&old).unwrap();
    let lease = f.lease();
    assert!(f
        .reviews
        .confirm(&f.key, &f.dir, lease.generation(), &old)
        .is_err());
    let saved = f
        .reviews
        .confirm(&f.key, &f.dir, lease.generation(), &current)
        .unwrap();
    assert_eq!(
        serde_json::to_value(saved).unwrap(),
        serde_json::json!({"version":3,"hidden":true,"searchPending":true})
    );
    f.reviews.dismiss(&current).unwrap();
    f.reviews.dismiss(&current).unwrap();
    assert!(f
        .reviews
        .retry(
            &f.key,
            &f.dir,
            lease.generation(),
            &current,
            1000,
            |_, _| panic!("closed review must not send")
        )
        .is_err());
}

#[test]
fn retry_never_confirms_and_delegates_only_the_due_confirmed_receipt() {
    let f = Fixture::new();
    let token = f.prepare();
    let lease = f.lease();
    let generation = lease.generation();
    f.reviews
        .retry(&f.key, &f.dir, generation, &token, 1000, |_, _| {
            panic!("unconfirmed review must not send")
        })
        .unwrap();
    assert_eq!(f.store().profile_draft().unwrap().unwrap().version, 2);
    f.reviews
        .confirm(&f.key, &f.dir, generation, &token)
        .unwrap();
    let record = f.store().due_search_profile(1000).unwrap().unwrap();
    f.store()
        .search_profile_rate_limited(1000, Duration::from_secs(120))
        .unwrap();
    f.reviews
        .retry(&f.key, &f.dir, generation, &token, 1119, |_, _| {
            panic!("server backoff must be honored")
        })
        .unwrap();
    let saved = f
        .reviews
        .retry(&f.key, &f.dir, generation, &token, 1120, |store, plan| {
            assert_eq!(
                store.due_search_withdrawal_recovery(plan, 1120).unwrap(),
                Some(record.clone())
            );
            store
                .search_profile_sent(&record, 1120)
                .map_err(|error| error.to_string())
        })
        .unwrap();
    assert_eq!(
        serde_json::to_value(saved).unwrap(),
        serde_json::json!({"version":3,"hidden":true,"searchPending":false})
    );
    f.reviews
        .retry(&f.key, &f.dir, generation, &token, 1121, |_, _| {
            panic!("ACK must be idempotent")
        })
        .unwrap();
}

#[test]
fn a_newer_head_is_not_sent_through_the_old_review() {
    let f = Fixture::new();
    let token = f.prepare();
    let lease = f.lease();
    f.reviews
        .confirm(&f.key, &f.dir, lease.generation(), &token)
        .unwrap();
    let newer = Profile {
        version: 4,
        ..Default::default()
    }
    .sign(&f.key);
    f.store().hold_search_profile(&newer).unwrap();
    assert!(f
        .reviews
        .retry(
            &f.key,
            &f.dir,
            lease.generation(),
            &token,
            u64::MAX,
            |_, _| panic!("old review must not send a newer head")
        )
        .is_err());
    assert_eq!(f.store().due_search_profile(u64::MAX).unwrap(), Some(newer));
}

#[test]
fn changing_database_coordinates_refuses_before_opening_or_creating_a_store() {
    let f = Fixture::new();
    let token = f.prepare();
    let other = f.dir.join("other");
    let lease = f.lease();
    assert!(f
        .reviews
        .confirm(&f.key, &other, lease.generation(), &token)
        .is_err());
    assert!(!other.exists());
    let reviews = RecoveryState::default();
    assert!(reviews.prepare(&f.key, &other, lease.generation()).is_err());
    assert!(!other.exists());
}
