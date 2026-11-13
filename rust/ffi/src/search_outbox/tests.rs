use super::*;
use crate::slot_registry_tests::{root, Credentials};
use crate::{MobileSlotLayout, MobileSlotSelection};
use dyapp_profile::Profile;
use std::path::PathBuf;
use std::time::Duration;

struct Fixture {
    root: PathBuf,
    keys: Arc<Credentials>,
    layout: MobileSlotLayout,
    registry: Arc<MobileIdentityRegistry>,
    selected: MobileSlotSelection,
    worker: Arc<MobileSearchOutbox>,
    dir: PathBuf,
}
impl Fixture {
    fn new(layout: MobileSlotLayout) -> Self {
        let root = root();
        let keys = Arc::new(Credentials::default());
        let registry =
            MobileIdentityRegistry::new(root.to_str().unwrap().into(), layout, keys.clone());
        let selected = registry.create().unwrap();
        let worker = MobileSearchOutbox::new(
            Arc::clone(&registry),
            selected.slot.id.clone(),
            selected.key.peer_id(),
        );
        let dir = PathBuf::from(&selected.slot.directory);
        Self {
            root,
            keys,
            layout,
            registry,
            selected,
            worker,
            dir,
        }
    }
    fn store(&self) -> Store {
        Store::open(&self.dir.join("client.db"), &self.selected.key.identity).unwrap()
    }
    fn reopen(&self) -> Arc<MobileSearchOutbox> {
        let registry = MobileIdentityRegistry::new(
            self.root.to_str().unwrap().into(),
            self.layout,
            self.keys.clone(),
        );
        MobileSearchOutbox::new(
            registry,
            self.selected.slot.id.clone(),
            self.selected.key.peer_id(),
        )
    }
}

fn create_while_idle(registry: &Arc<MobileIdentityRegistry>) -> MobileSlotSelection {
    let registry = Arc::clone(registry);
    let (done, ready) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        done.send(registry.create()).unwrap();
    });
    let selected = ready
        .recv_timeout(Duration::from_secs(30))
        .expect("idle outbox retained admission")
        .unwrap();
    worker.join().unwrap();
    selected
}

#[test]
fn empty_existing_slots_remain_idle_without_creating_database_or_chat_credentials() {
    for layout in [MobileSlotLayout::Android, MobileSlotLayout::Ios] {
        let f = Fixture::new(layout);
        assert_eq!(f.worker.status().unwrap(), SearchOutboxStatus::Idle);
        assert_eq!(f.worker.poll().unwrap(), SearchOutboxStatus::Idle);
        assert!(!f.dir.join("client.db").exists());
        assert!(!f.dir.join("device.key").exists());
        assert!(!f.dir.join("chat-state.key").exists());
        create_while_idle(&f.registry);
    }
}

#[test]
fn legacy_records_stay_blocked_after_restart_without_active_network_or_signing() {
    for layout in [MobileSlotLayout::Android, MobileSlotLayout::Ios] {
        let f = Fixture::new(layout);
        let record = Profile::tombstone(2, 0, 0).sign(&f.selected.key.identity);
        f.store().hold_search_profile(&record).unwrap();
        let reopened = f.reopen();
        assert_eq!(
            reopened.status().unwrap(),
            SearchOutboxStatus::ReviewRequired
        );
        assert_eq!(reopened.poll().unwrap(), SearchOutboxStatus::ReviewRequired);
        assert_eq!(
            f.store().due_search_profile(u64::MAX).unwrap(),
            Some(record)
        );
        assert!(!f.dir.join("device.key").exists());
    }
}

#[test]
fn confirmed_queue_reopens_for_original_owner_and_keeps_backoff_without_a_review_handle() {
    for layout in [MobileSlotLayout::Android, MobileSlotLayout::Ios] {
        let f = Fixture::new(layout);
        let store = f.store();
        store
            .save_profile_draft(&Profile {
                version: 2,
                ..Default::default()
            })
            .unwrap();
        store.set_profile_hidden(true).unwrap();
        store
            .hold_search_profile(&Profile::tombstone(2, 0, 0).sign(&f.selected.key.identity))
            .unwrap();
        let plan = store.prepare_search_withdrawal_recovery().unwrap().unwrap();
        store
            .confirm_search_withdrawal_recovery(&f.selected.key.identity, &plan)
            .unwrap();
        drop(plan);
        let record = store.due_search_profile(u64::MAX).unwrap().unwrap();
        drop(store);
        check_reopened(&f, &record);
    }
}

fn check_reopened(f: &Fixture, record: &dyapp_identity::SignedRecord) {
    let reopened = f.reopen();
    let other = create_while_idle(&f.registry);
    assert_eq!(
        reopened.status().unwrap(),
        SearchOutboxStatus::Scheduled { ready_at: 0 }
    );
    assert!(matches!(reopened.poll(), Err(CoreError::Failed(reason)) if reason == "notConnected"));
    assert_eq!(
        f.store().due_search_profile(u64::MAX).unwrap().as_ref(),
        Some(record)
    );
    f.store()
        .search_profile_rate_limited(0, Duration::from_secs(u64::MAX))
        .unwrap();
    assert_eq!(
        reopened.poll().unwrap(),
        SearchOutboxStatus::Scheduled { ready_at: u64::MAX }
    );
    f.store().search_profile_sent(record, 1000).unwrap();
    assert_eq!(reopened.poll().unwrap(), SearchOutboxStatus::Idle);
    assert!(!PathBuf::from(&other.slot.directory)
        .join("client.db")
        .exists());
    assert!(!f.dir.join("device.key").exists());
    assert!(!f.dir.join("chat-state.key").exists());
}
