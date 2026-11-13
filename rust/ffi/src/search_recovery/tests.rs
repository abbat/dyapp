use super::*;
use crate::slot_registry_tests::{root, Credentials, Wrapping};
use crate::{MobileSlotLayout, MobileSlotSelection};
use dyapp_profile::Profile;
use std::{fs, path::PathBuf};

struct Fixture {
    registry: Arc<MobileIdentityRegistry>,
    slot: MobileSlotSelection,
    worker: Arc<MobileSearchRecovery>,
    dir: PathBuf,
}
impl Fixture {
    fn new(layout: MobileSlotLayout) -> Self {
        let registry = MobileIdentityRegistry::new(
            root().to_str().unwrap().into(),
            layout,
            Arc::new(Credentials::default()),
        );
        let slot = registry.create().unwrap();
        drop(
            registry
                .open_chats(slot.slot.id.clone(), slot.key.peer_id(), Arc::new(Wrapping))
                .unwrap(),
        );
        let worker = MobileSearchRecovery::new(
            Arc::clone(&registry),
            slot.slot.id.clone(),
            slot.key.peer_id(),
            Arc::new(Wrapping),
        );
        let dir = PathBuf::from(&slot.slot.directory);
        let store = Store::open_existing(&dir.join("client.db"), &slot.key.identity).unwrap();
        store
            .save_profile_draft(&Profile {
                version: 2,
                name: "original private draft".into(),
                ..Default::default()
            })
            .unwrap();
        store.set_profile_hidden(true).unwrap();
        store
            .hold_search_profile(&Profile::tombstone(2, 0, 0).sign(&slot.key.identity))
            .unwrap();
        Self {
            registry,
            slot,
            worker,
            dir,
        }
    }
    fn store(&self) -> Store {
        Store::open_existing(&self.dir.join("client.db"), &self.slot.key.identity).unwrap()
    }
    fn credentials(&self) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        (
            fs::read(self.dir.join("device.key")).unwrap(),
            fs::read(self.dir.join("chat-state.key")).unwrap(),
            self.store().sealed_device().unwrap().unwrap(),
        )
    }
}

fn create_while_review_is_idle(registry: &Arc<MobileIdentityRegistry>) -> MobileSlotSelection {
    let registry = Arc::clone(registry);
    let (sent, done) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let _ = sent.send(registry.create());
    });
    let result = done
        .recv_timeout(std::time::Duration::from_secs(30))
        .expect("idle review retained registry admission");
    worker.join().unwrap();
    result.unwrap()
}

#[test]
fn both_layouts_release_idle_admission_and_confirm_only_the_original_slot() {
    for layout in [MobileSlotLayout::Android, MobileSlotLayout::Ios] {
        let f = Fixture::new(layout);
        let before = f.credentials();
        let review = f.worker.prepare().unwrap().unwrap();
        assert_eq!(f.store().profile_draft().unwrap().unwrap().version, 2);
        let other = create_while_review_is_idle(&f.registry);
        let saved = review.confirm().unwrap();
        assert_eq!(
            saved,
            SearchRecoverySaved {
                version: 3,
                hidden: true,
                search_pending: true
            }
        );
        assert_eq!(review.confirm().unwrap(), saved);
        let record = f.store().due_search_profile(u64::MAX).unwrap().unwrap();
        assert!(
            dyapp_profile::verify_search(&record)
                .unwrap()
                .profile
                .search_only
        );
        assert!(dyapp_profile::verify(&record).is_err());
        f.store().search_profile_sent(&record, 1000).unwrap();
        assert_eq!(
            review.confirm().unwrap(),
            SearchRecoverySaved {
                search_pending: false,
                ..saved
            }
        );
        assert!(f.worker.prepare().unwrap().is_none());
        assert_eq!(f.credentials(), before);
        assert!(!PathBuf::from(&other.slot.directory)
            .join("client.db")
            .exists());
        assert!(
            matches!(f.registry.resolve().unwrap(), crate::MobileSlotResolution::Ready { key, .. } if key.peer_id() == other.key.peer_id())
        );
    }
}

#[test]
fn close_is_idempotent_and_precedes_credential_admission() {
    let f = Fixture::new(MobileSlotLayout::Android);
    let review = f.worker.prepare().unwrap().unwrap();
    let draft = f.store().profile_draft().unwrap();
    review.dismiss().unwrap();
    review.dismiss().unwrap();
    fs::rename(f.dir.join("device.key"), f.dir.join("missing-device.key")).unwrap();
    assert!(matches!(review.confirm(), Err(SearchRecoveryError::Closed)));
    assert!(matches!(review.retry(), Err(SearchRecoveryError::Closed)));
    assert_eq!(f.store().profile_draft().unwrap(), draft);
    assert!(!f.dir.join("device.key").exists());
}

#[test]
fn retry_never_confirms_and_skips_network_until_the_record_is_due() {
    for layout in [MobileSlotLayout::Android, MobileSlotLayout::Ios] {
        let f = Fixture::new(layout);
        let review = f.worker.prepare().unwrap().unwrap();
        let credentials = f.credentials();
        let pending = f.store().due_search_profile(u64::MAX).unwrap();
        assert!(review.retry().unwrap().search_pending);
        assert_eq!(f.store().due_search_profile(u64::MAX).unwrap(), pending);
        assert_eq!(f.store().profile_draft().unwrap().unwrap().version, 2);
        review.confirm().unwrap();
        f.store()
            .search_profile_rate_limited(0, std::time::Duration::from_secs(u64::MAX))
            .unwrap();
        assert!(review.retry().unwrap().search_pending);
        assert_eq!(f.credentials(), credentials);
        let record = f.store().due_search_profile(u64::MAX).unwrap().unwrap();
        f.store().search_profile_sent(&record, 1000).unwrap();
        assert!(!review.retry().unwrap().search_pending);
        f.store().set_profile_hidden(false).unwrap();
        assert!(matches!(review.retry(), Err(SearchRecoveryError::Changed)));
    }
}

#[test]
fn independent_draft_edit_requires_new_review_without_overwriting_it() {
    let f = Fixture::new(MobileSlotLayout::Android);
    let review = f.worker.prepare().unwrap().unwrap();
    let mut draft = f.store().profile_draft().unwrap().unwrap();
    draft.name = "edited after preparation".into();
    f.store().save_profile_draft(&draft).unwrap();
    assert!(matches!(
        review.confirm(),
        Err(SearchRecoveryError::Changed)
    ));
    assert_eq!(f.store().profile_draft().unwrap(), Some(draft));
}

fn replace_credential(f: &Fixture, name: &str, replacement: &[u8; 32]) {
    let admitted = f.worker.owner.open().unwrap();
    let operation = &admitted.operation;
    let sealed = if name == "device.key" {
        let device = dyapp_identity::Identity::from_secret(replacement);
        dyapp_messaging::mls::Device::new(operation.identity(), device.public_key(), 1)
            .unwrap()
            .seal(operation.state_key())
            .unwrap()
    } else {
        let prior = admitted.store.sealed_device().unwrap().unwrap();
        dyapp_messaging::mls::Device::open(&prior, operation.state_key())
            .unwrap()
            .seal(replacement)
            .unwrap()
    };
    fs::write(f.dir.join(name), replacement.map(|byte| byte ^ 0x5a)).unwrap();
    f.store().stage_outbound(&sealed, &[]).unwrap();
}

#[test]
fn changed_device_or_sealing_key_refuses_the_original_review() {
    for name in ["device.key", "chat-state.key"] {
        let f = Fixture::new(MobileSlotLayout::Android);
        let review = f.worker.prepare().unwrap().unwrap();
        let replacement = [23u8; 32].map(|byte| byte ^ 0x5a).to_vec();
        replace_credential(&f, name, &[23; 32]);
        let result = review.confirm();
        assert!(
            matches!(result, Err(SearchRecoveryError::Changed)),
            "{name}: {result:?}"
        );
        assert_eq!(f.store().profile_draft().unwrap().unwrap().version, 2);
        assert_eq!(fs::read(f.dir.join(name)).unwrap(), replacement);
    }
}

#[test]
fn inconsistent_device_credentials_fail_before_review_without_repairing_state() {
    let f = Fixture::new(MobileSlotLayout::Android);
    let review = f.worker.prepare().unwrap().unwrap();
    let sealed = f.store().sealed_device().unwrap();
    let replacement = [23u8; 32].map(|byte| byte ^ 0x5a);
    fs::write(f.dir.join("device.key"), replacement).unwrap();
    assert!(
        matches!(review.confirm(), Err(SearchRecoveryError::Failed { reason })
        if reason == "The saved chat device key changed")
    );
    assert_eq!(f.store().profile_draft().unwrap().unwrap().version, 2);
    assert_eq!(f.store().sealed_device().unwrap(), sealed);
    assert_eq!(fs::read(f.dir.join("device.key")).unwrap(), replacement);
}

#[test]
fn a_removed_original_directory_cannot_be_recreated_by_confirmation() {
    let f = Fixture::new(MobileSlotLayout::Android);
    let review = f.worker.prepare().unwrap().unwrap();
    let retired = f.dir.with_extension("retired");
    fs::rename(&f.dir, &retired).unwrap();
    assert!(review.confirm().is_err());
    assert!(!f.dir.exists());
    let store = Store::open_existing(&retired.join("client.db"), &f.slot.key.identity).unwrap();
    assert_eq!(store.profile_draft().unwrap().unwrap().version, 2);
}

#[test]
fn preparation_never_bootstraps_missing_chat_credentials_or_database() {
    let registry = MobileIdentityRegistry::new(
        root().to_str().unwrap().into(),
        MobileSlotLayout::Android,
        Arc::new(Credentials::default()),
    );
    let slot = registry.create().unwrap();
    let worker = MobileSearchRecovery::new(
        registry,
        slot.slot.id,
        slot.key.peer_id(),
        Arc::new(Wrapping),
    );
    assert!(worker.prepare().is_err());
    let dir = PathBuf::from(slot.slot.directory);
    for name in ["client.db", "device.key", "chat-state.key"] {
        assert!(!dir.join(name).exists());
    }
}

#[test]
fn wrong_expected_owner_is_refused_before_any_recovery() {
    let f = Fixture::new(MobileSlotLayout::Android);
    let worker = MobileSearchRecovery::new(
        Arc::clone(&f.registry),
        f.slot.slot.id.clone(),
        "another owner".into(),
        Arc::new(Wrapping),
    );
    assert!(worker.prepare().is_err());
    assert_eq!(f.store().profile_draft().unwrap().unwrap().version, 2);
}
