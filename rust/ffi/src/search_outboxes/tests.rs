use super::*;
use crate::slot_registry_tests::{root, Credentials};
use crate::{IdentityKey, MobileSlotKeys, MobileSlotLayout, MobileSlotSelection, WrapError};
use dyapp_client::store::Store;
use dyapp_profile::Profile;
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

#[derive(Default)]
struct Keys {
    inner: Credentials,
    opened: Mutex<BTreeSet<String>>,
}
impl MobileSlotKeys for Keys {
    fn contains(&self, id: String) -> Result<bool, WrapError> {
        self.inner.contains(id)
    }
    fn open_wrapped(
        &self,
        id: String,
        wrapped: Vec<u8>,
    ) -> Result<Option<Arc<IdentityKey>>, WrapError> {
        self.opened.lock().unwrap().insert(id.clone());
        self.inner.open_wrapped(id, wrapped)
    }
    fn wrap(&self, id: String, key: Arc<IdentityKey>) -> Result<Vec<u8>, WrapError> {
        self.inner.wrap(id, key)
    }
    fn erase(&self, id: String) -> Result<(), WrapError> {
        self.inner.erase(id)
    }
}
fn queue(registry: &MobileIdentityRegistry, deadline: u64) -> MobileSlotSelection {
    let selected = registry.create().unwrap();
    set_queue(&selected, deadline);
    selected
}

fn set_queue(selected: &MobileSlotSelection, deadline: u64) {
    let store = Store::open(
        &Path::new(&selected.slot.directory).join("client.db"),
        &selected.key.identity,
    )
    .unwrap();
    store.set_profile_hidden(true).unwrap();
    store
        .hold_search_profile(&Profile::search_tombstone(1, 0, 0).sign(&selected.key.identity))
        .unwrap();
    store
        .search_profile_rate_limited(0, Duration::from_secs(deadline))
        .unwrap();
}

#[test]
fn all_owner_queues_are_visited_without_changing_selection() {
    for layout in [MobileSlotLayout::Android, MobileSlotLayout::Ios] {
        let keys = Arc::new(Keys::default());
        let registry =
            MobileIdentityRegistry::new(root().to_str().unwrap().into(), layout, keys.clone());
        let first = queue(&registry, u64::MAX - 1);
        let second = queue(&registry, u64::MAX);
        keys.opened.lock().unwrap().clear();
        let worker = MobileSearchOutboxes::new(Arc::clone(&registry));
        let SearchOutboxWake::At { ready_at, delay_ms } = worker.poll().unwrap() else {
            panic!("future queues need a deadline");
        };
        assert_eq!(ready_at, u64::MAX - 1);
        assert_eq!(delay_ms, u64::MAX);
        let opened = keys.opened.lock().unwrap();
        assert!(opened.contains(&first.slot.id));
        assert!(opened.contains(&second.slot.id));
        drop(opened);
        assert!(
            matches!(registry.resolve().unwrap(), MobileSlotResolution::Ready { key, .. } if key.peer_id() == second.key.peer_id())
        );
    }
}

#[test]
fn a_failed_due_owner_does_not_prevent_another_owner_from_being_visited() {
    for layout in [MobileSlotLayout::Android, MobileSlotLayout::Ios] {
        let keys = Arc::new(Keys::default());
        let registry =
            MobileIdentityRegistry::new(root().to_str().unwrap().into(), layout, keys.clone());
        let first = registry.create().unwrap();
        let second = registry.create().unwrap();
        let (failed, waiting) = if registry.list().unwrap()[0].id == first.slot.id {
            (first, second)
        } else {
            (second, first)
        };
        set_queue(&failed, 0);
        set_queue(&waiting, u64::MAX);
        keys.opened.lock().unwrap().clear();
        assert_eq!(
            MobileSearchOutboxes::new(Arc::clone(&registry))
                .poll()
                .unwrap(),
            SearchOutboxWake::Retry
        );
        let opened = keys.opened.lock().unwrap();
        assert!(opened.contains(&failed.slot.id));
        assert!(opened.contains(&waiting.slot.id));
    }
}
