use super::*;
use crate::slot_registry_tests::{root, Credentials};
use crate::{IdentityKey, MobileIdentityRegistry, MobileSlotLayout, MobileSlotSelection};
use std::fs::{self, OpenOptions, TryLockError};
use std::sync::mpsc::{self, Receiver, Sender};

fn fixture(
    layout: MobileSlotLayout,
) -> (Arc<MobileIdentityRegistry>, MobileSlotSelection, PathBuf) {
    let root = root();
    let registry = MobileIdentityRegistry::new(
        root.to_str().unwrap().to_owned(),
        layout,
        Arc::new(Credentials::default()),
    );
    let created = registry.create().unwrap();
    (registry, created, root)
}

struct RunningCheck {
    ready: Receiver<()>,
    resume: Sender<()>,
    finished: Receiver<Result<Vec<SeedRow>, CheckError>>,
    thread: std::thread::JoinHandle<()>,
}

impl RunningCheck {
    fn start(worker: Arc<ConnectionCheck>, draft: NetworkSettings) -> Self {
        let (entered, ready) = mpsc::channel();
        let (resume, resumed) = mpsc::channel();
        let (finished, completed) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            let result = worker.run_with_ready(draft, 60_000, || {
                entered.send(()).unwrap();
                resumed.recv_timeout(Duration::from_secs(5)).unwrap();
            });
            drop(worker);
            finished.send(result).unwrap();
        });
        Self {
            ready,
            resume,
            finished: completed,
            thread,
        }
    }
}

#[test]
fn registered_network_work_retains_protection_through_cancellation_only() {
    for layout in [MobileSlotLayout::Android, MobileSlotLayout::Ios] {
        let (registry, created, root) = fixture(layout);
        let check = ConnectionCheck::for_slot(
            registry.clone(),
            created.slot.id.clone(),
            created.key.peer_id(),
        )
        .unwrap();
        let (mute, _udp) = reserved_endpoint();
        let draft = NetworkSettings {
            seeds: vec![mute.local_addr().unwrap().to_string()],
            name: String::new(),
        };
        let running = RunningCheck::start(check.clone(), draft);
        running.ready.recv_timeout(Duration::from_secs(5)).unwrap();
        let name = match layout {
            MobileSlotLayout::Android => "profiles.lock",
            MobileSlotLayout::Ios => ".identity-slots.lock",
        };
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .open(root.join(name))
            .unwrap();
        assert!(matches!(lock.try_lock(), Err(TryLockError::WouldBlock)));
        check.cancel();
        let idle = check.clone();
        drop(check);
        assert!(matches!(lock.try_lock(), Err(TryLockError::WouldBlock)));
        running.resume.send(()).unwrap();
        assert_eq!(
            running
                .finished
                .recv_timeout(Duration::from_secs(5))
                .unwrap(),
            Err(CheckError::Cancelled)
        );
        running.thread.join().unwrap();
        lock.try_lock().unwrap();
        lock.unlock().unwrap();
        remove_with_idle_check(&registry, created, &idle);
    }
}

fn remove_with_idle_check(
    registry: &MobileIdentityRegistry,
    created: MobileSlotSelection,
    idle: &ConnectionCheck,
) {
    registry.select(created.slot.id.clone()).unwrap();
    registry
        .remove(created.slot.id, Some(created.key.peer_id()))
        .unwrap();
    assert!(matches!(idle.activate(), Err(CheckError::Failed { .. })));
    assert!(idle
        .run(
            NetworkSettings {
                seeds: vec![],
                name: String::new()
            },
            0
        )
        .is_err());
    assert!(!PathBuf::from(created.slot.directory).exists());
}

#[test]
fn checked_network_cannot_activate_or_overwrite_draft_after_identity_replacement() {
    for layout in [MobileSlotLayout::Android, MobileSlotLayout::Ios] {
        let (registry, created, _) = fixture(layout);
        let check =
            ConnectionCheck::for_slot(registry, created.slot.id, created.key.peer_id()).unwrap();
        let directory = PathBuf::from(created.slot.directory);
        assert!(!directory.join("network-draft").exists());
        let (port, peer) = node();
        let draft = NetworkSettings {
            seeds: vec![format!("127.0.0.1:{port}/{peer}")],
            name: String::new(),
        };
        assert_eq!(
            check.run(draft.clone(), 5000).unwrap(),
            vec![SeedRow::Connected { peer_id: peer }]
        );
        check.activate().unwrap();
        let active = fs::read(directory.join("network")).unwrap();
        let saved = fs::read(directory.join("network-draft")).unwrap();
        let replacement = IdentityKey::generate()
            .wrap(Arc::new(crate::slot_registry_tests::Wrapping))
            .unwrap();
        fs::write(directory.join("identity.key"), replacement).unwrap();
        assert!(matches!(check.activate(), Err(CheckError::Failed { .. })));
        assert!(matches!(
            check.run(
                NetworkSettings {
                    name: "different".into(),
                    ..draft
                },
                0
            ),
            Err(CheckError::Failed { .. })
        ));
        assert_eq!(fs::read(directory.join("network")).unwrap(), active);
        assert_eq!(fs::read(directory.join("network-draft")).unwrap(), saved);
    }
}

#[test]
fn binding_a_checker_refuses_another_identity_without_writing_settings() {
    let (registry, created, _) = fixture(MobileSlotLayout::Android);
    assert!(ConnectionCheck::for_slot(registry, created.slot.id, "wrong peer".into()).is_err());
    let directory = PathBuf::from(created.slot.directory);
    assert!(!directory.join("network-draft").exists());
    assert!(!directory.join("network").exists());
}
