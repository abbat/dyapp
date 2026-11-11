use super::*;
use crate::{CoreError, ProfileSolve, ProfileWorkStep};
use std::fs::{self, OpenOptions, TryLockError};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

struct Gate {
    entered: Sender<()>,
    resume: Receiver<()>,
}

#[derive(Default)]
struct WorkerCredentials {
    keys: Credentials,
    gate: Mutex<Option<Gate>>,
}

impl MobileSlotKeys for WorkerCredentials {
    fn contains(&self, id: String) -> Result<bool, WrapError> {
        self.keys.contains(id)
    }

    fn open_wrapped(
        &self,
        id: String,
        wrapped: Vec<u8>,
    ) -> Result<Option<Arc<IdentityKey>>, WrapError> {
        let gate = self.gate.lock().unwrap().take();
        if let Some(gate) = gate {
            gate.entered.send(()).unwrap();
            gate.resume.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        self.keys.open_wrapped(id, wrapped)
    }

    fn wrap(&self, id: String, key: Arc<IdentityKey>) -> Result<Vec<u8>, WrapError> {
        self.keys.wrap(id, key)
    }

    fn erase(&self, id: String) -> Result<(), WrapError> {
        self.keys.erase(id)
    }
}

struct RunningStep {
    entered: Receiver<()>,
    resume: Sender<()>,
    finished: Receiver<Result<ProfileWorkStep, CoreError>>,
    thread: std::thread::JoinHandle<()>,
}

impl RunningStep {
    fn start(keys: &WorkerCredentials, worker: Arc<ProfileSolve>) -> Self {
        let (entered, ready) = mpsc::channel();
        let (resume, resumed) = mpsc::channel();
        let (finished, completed) = mpsc::channel();
        *keys.gate.lock().unwrap() = Some(Gate {
            entered,
            resume: resumed,
        });
        let thread = std::thread::spawn(move || {
            let result = worker.run_step(true);
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
fn profile_worker_holds_admission_through_cancellation_but_not_between_steps() {
    for layout in [MobileSlotLayout::Android, MobileSlotLayout::Ios] {
        let root = root();
        let keys = Arc::new(WorkerCredentials::default());
        let registry =
            MobileIdentityRegistry::new(root.to_str().unwrap().to_owned(), layout, keys.clone());
        let created = registry.create().unwrap();
        let solve = ProfileSolve::for_slot(
            registry.clone(),
            created.slot.id.clone(),
            created.key.peer_id(),
        )
        .unwrap();
        let directory = PathBuf::from(&created.slot.directory);
        assert!(!directory.join("client.db").exists());
        let step = RunningStep::start(&keys, solve.clone());
        step.entered.recv_timeout(Duration::from_secs(5)).unwrap();
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
        solve.cancel();
        // Drop the caller's owner while the native step still has work to finish.
        let idle = solve.clone();
        drop(solve);
        assert!(matches!(lock.try_lock(), Err(TryLockError::WouldBlock)));
        step.resume.send(()).unwrap();
        let result = step
            .finished
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        assert!(result.cancelled);
        step.thread.join().unwrap();
        lock.try_lock().unwrap();
        lock.unlock().unwrap();
        registry.select(created.slot.id.clone()).unwrap();
        registry
            .remove(created.slot.id, Some(created.key.peer_id()))
            .unwrap();
        assert!(!directory.exists());
        assert!(idle.run_step(true).is_err());
        assert!(!directory.exists());
    }
}

#[test]
fn profile_worker_refuses_replaced_keys_before_touching_the_database() {
    let root = root();
    let registry = MobileIdentityRegistry::new(
        root.to_str().unwrap().to_owned(),
        MobileSlotLayout::Android,
        Arc::new(Credentials::default()),
    );
    let created = registry.create().unwrap();
    assert!(ProfileSolve::for_slot(
        registry.clone(),
        created.slot.id.clone(),
        "wrong peer".into()
    )
    .is_err());
    let solve = ProfileSolve::for_slot(registry, created.slot.id, created.key.peer_id()).unwrap();
    solve.run_step(true).unwrap();
    let directory = PathBuf::from(created.slot.directory);
    let original = fs::read(directory.join("client.db")).unwrap();
    let replacement = IdentityKey::generate().wrap(Arc::new(Wrapping)).unwrap();
    fs::write(directory.join("identity.key"), replacement).unwrap();
    assert!(solve.run_step(true).is_err());
    assert_eq!(fs::read(directory.join("client.db")).unwrap(), original);
}
