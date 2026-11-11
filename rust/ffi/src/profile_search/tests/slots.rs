use super::*;
use crate::slot_registry_tests::workers::{Gate, WorkerCredentials};
use crate::slot_registry_tests::{root, Wrapping};
use crate::{MobileIdentityRegistry, MobileSlotLayout, MobileSlotSelection};
use std::fs::{self, File, OpenOptions, TryLockError};
use std::sync::mpsc::{self, Receiver, Sender};

#[derive(Clone, Copy)]
enum Read {
    Search,
    Names,
    Profiles,
    Photos,
}

impl Read {
    fn run(self, worker: &ProfileSearcher, peer: String) -> Result<(), CoreError> {
        match self {
            Self::Search => worker
                .search_slot(vec![], SearchEmployer::Any, 0)
                .map(|_| ()),
            Self::Names => worker.contact_names_slot(vec![peer]).map(|_| ()),
            Self::Profiles => worker.profiles(vec![peer]).map(|_| ()),
            Self::Photos => worker.fetch_peer_photos(vec![vec![8; 32]], false),
        }
    }
}

struct Fixture {
    root: PathBuf,
    registry: Arc<MobileIdentityRegistry>,
    created: MobileSlotSelection,
    worker: Arc<ProfileSearcher>,
    keys: Arc<WorkerCredentials>,
}

impl Fixture {
    fn new(layout: MobileSlotLayout) -> Self {
        let root = root();
        let keys = Arc::new(WorkerCredentials::default());
        let registry =
            MobileIdentityRegistry::new(root.to_str().unwrap().to_owned(), layout, keys.clone());
        let created = registry.create().unwrap();
        let worker = ProfileSearcher::for_slot(
            registry.clone(),
            created.slot.id.clone(),
            created.key.peer_id(),
        )
        .unwrap();
        Self {
            root,
            registry,
            created,
            worker,
            keys,
        }
    }

    fn lock(&self, layout: MobileSlotLayout) -> File {
        let name = match layout {
            MobileSlotLayout::Android => "profiles.lock",
            MobileSlotLayout::Ios => ".identity-slots.lock",
        };
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(self.root.join(name))
            .unwrap()
    }

    fn refuse_reads(&self) {
        let peer = self.created.key.peer_id();
        for read in [Read::Search, Read::Names, Read::Profiles, Read::Photos] {
            assert!(read.run(&self.worker, peer.clone()).is_err());
        }
    }
}

struct RunningRead {
    entered: Receiver<()>,
    resume: Sender<()>,
    finished: Receiver<Result<(), CoreError>>,
    thread: std::thread::JoinHandle<()>,
}

impl RunningRead {
    fn start(fixture: &Fixture, read: Read) -> Self {
        let (entered, ready) = mpsc::channel();
        let (resume, resumed) = mpsc::channel();
        let (finished, completed) = mpsc::channel();
        *fixture.keys.gate.lock().unwrap() = Some(Gate {
            entered,
            resume: resumed,
        });
        let worker = fixture.worker.clone();
        let peer = fixture.created.key.peer_id();
        let thread = std::thread::spawn(move || {
            let result = read.run(&worker, peer);
            drop(worker);
            finished.send(result).unwrap();
        });
        Self {
            entered: ready,
            resume,
            finished: completed,
            thread,
        }
    }
}

#[test]
fn every_search_disk_path_holds_core_admission_and_idle_handles_allow_removal() {
    for layout in [MobileSlotLayout::Android, MobileSlotLayout::Ios] {
        for read in [Read::Search, Read::Names, Read::Profiles, Read::Photos] {
            let fixture = Fixture::new(layout);
            let running = RunningRead::start(&fixture, read);
            running
                .entered
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
            let lock = fixture.lock(layout);
            assert!(matches!(lock.try_lock(), Err(TryLockError::WouldBlock)));
            running.resume.send(()).unwrap();
            let result = running
                .finished
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
            match read {
                Read::Names => assert!(result.is_ok()),
                _ => assert_eq!(result.unwrap_err().to_string(), "notConnected"),
            }
            running.thread.join().unwrap();
            lock.try_lock().unwrap();
            lock.unlock().unwrap();
            fixture
                .registry
                .remove(
                    fixture.created.slot.id.clone(),
                    Some(fixture.created.key.peer_id()),
                )
                .unwrap();
            fixture.refuse_reads();
            assert!(!PathBuf::from(&fixture.created.slot.directory).exists());
        }
    }
}

#[test]
fn a_foreign_key_cannot_initialize_a_bound_searchers_database() {
    let fixture = Fixture::new(MobileSlotLayout::Android);
    let key = IdentityKey::generate();
    assert!(fixture
        .worker
        .search(key.clone(), vec![], SearchEmployer::Any, 0)
        .is_err());
    assert!(fixture.worker.contact_names(key, vec![]).is_err());
    assert!(!PathBuf::from(fixture.created.slot.directory)
        .join("client.db")
        .exists());
}

#[test]
fn a_bound_searcher_refuses_replacement_before_reading_its_database() {
    let fixture = Fixture::new(MobileSlotLayout::Android);
    fixture.worker.contact_names_slot(vec![]).unwrap();
    let directory = PathBuf::from(&fixture.created.slot.directory);
    let original = fs::read(directory.join("client.db")).unwrap();
    let replacement = IdentityKey::generate().wrap(Arc::new(Wrapping)).unwrap();
    fs::write(directory.join("identity.key"), replacement).unwrap();
    fixture.refuse_reads();
    assert_eq!(fs::read(directory.join("client.db")).unwrap(), original);
}

#[test]
fn keyless_calls_on_unbound_searchers_never_invent_an_identity() {
    let dir = directory("unbound");
    let worker = ProfileSearcher::new(dir.to_str().unwrap().to_owned()).unwrap();
    assert!(worker.search_slot(vec![], SearchEmployer::Any, 0).is_err());
    assert!(worker.contact_names_slot(vec![]).is_err());
    assert!(!dir.join("client.db").exists());
}
