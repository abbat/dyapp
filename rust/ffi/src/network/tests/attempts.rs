use super::*;
use crate::slot_registry_tests::root;
use crate::slot_registry_tests::workers::{Gate, WorkerCredentials};
use crate::{ConnectionAttempt, MobileIdentityRegistry, MobileSlotLayout};
use std::fs::{File, OpenOptions, TryLockError};
use std::sync::mpsc::{self, Receiver, Sender};

fn draft() -> NetworkSettings {
    NetworkSettings {
        seeds: vec!["not an address".into()],
        name: String::new(),
    }
}

#[test]
fn cancellation_before_dispatch_preserves_files_and_a_new_retry_is_independent() {
    let directory = temp_dir("attempt-before-dispatch");
    let checker = ConnectionCheck::new(directory.clone()).unwrap();
    let attempt = ConnectionAttempt::new(checker.clone());
    attempt.cancel();
    assert!(attempt.is_cancelled());
    assert_eq!(attempt.run(draft(), 0), Err(CheckError::Cancelled));
    assert!(!PathBuf::from(&directory).join("network-draft").exists());
    assert!(matches!(
        attempt.run(draft(), 0),
        Err(CheckError::Failed { .. })
    ));
    let retry = ConnectionAttempt::new(checker);
    assert!(!retry.is_cancelled());
    assert_eq!(
        retry.run(draft(), 0).unwrap(),
        vec![SeedRow::InvalidAddress]
    );
    assert!(matches!(
        retry.run(draft(), 0),
        Err(CheckError::Failed { .. })
    ));
}

#[test]
fn cancellation_at_registration_wins_over_an_immediately_ready_check() {
    let checker = ConnectionCheck::new(temp_dir("attempt-registration")).unwrap();
    let cancellation = Cancellation::new();
    assert_eq!(
        checker.run_attempt_with_ready(draft(), 0, &cancellation, || cancellation.cancel()),
        Err(CheckError::Cancelled)
    );
    assert_eq!(
        checker.run(draft(), 0).unwrap(),
        vec![SeedRow::InvalidAddress]
    );
}

struct Admission {
    keys: Arc<WorkerCredentials>,
    checker: Arc<ConnectionCheck>,
    directory: PathBuf,
    lock: File,
}

impl Admission {
    fn new(layout: MobileSlotLayout) -> Self {
        let root = root();
        let keys = Arc::new(WorkerCredentials::default());
        let registry =
            MobileIdentityRegistry::new(root.to_str().unwrap().into(), layout, keys.clone());
        let created = registry.create().unwrap();
        let checker =
            ConnectionCheck::for_slot(registry, created.slot.id, created.key.peer_id()).unwrap();
        let name = match layout {
            MobileSlotLayout::Android => "profiles.lock",
            MobileSlotLayout::Ios => ".identity-slots.lock",
        };
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .open(root.join(name))
            .unwrap();
        Self {
            keys,
            checker,
            directory: created.slot.directory.into(),
            lock,
        }
    }
}

struct RunningAttempt {
    entered: Receiver<()>,
    resume: Sender<()>,
    finished: Receiver<Result<Vec<SeedRow>, CheckError>>,
    thread: std::thread::JoinHandle<()>,
}

impl RunningAttempt {
    fn start(keys: &WorkerCredentials, attempt: Arc<ConnectionAttempt>) -> Self {
        let (entered, ready) = mpsc::channel();
        let (resume, resumed) = mpsc::channel();
        let (finished, completed) = mpsc::channel();
        *keys.gate.lock().unwrap() = Some(Gate {
            entered,
            resume: resumed,
        });
        let thread = std::thread::spawn(move || {
            let result = attempt.run(draft(), 0);
            drop(attempt);
            finished.send(result).unwrap();
        });
        Self {
            entered: ready,
            resume,
            finished: completed,
            thread,
        }
    }

    fn finish(self) -> Result<Vec<SeedRow>, CheckError> {
        self.resume.send(()).unwrap();
        let result = self.finished.recv_timeout(Duration::from_secs(5)).unwrap();
        self.thread.join().unwrap();
        result
    }
}

#[test]
fn cancellation_during_admission_retains_the_lease_until_actual_completion_without_writes() {
    for layout in [MobileSlotLayout::Android, MobileSlotLayout::Ios] {
        let fixture = Admission::new(layout);
        fixture.lock.try_lock().unwrap();
        fixture.lock.unlock().unwrap();
        let attempt = ConnectionAttempt::new(fixture.checker.clone());
        let running = RunningAttempt::start(&fixture.keys, attempt.clone());
        running
            .entered
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        attempt.cancel();
        assert!(matches!(
            fixture.lock.try_lock(),
            Err(TryLockError::WouldBlock)
        ));
        assert!(!fixture.directory.join("network-draft").exists());
        assert_eq!(running.finish(), Err(CheckError::Cancelled));
        fixture.lock.try_lock().unwrap();
        fixture.lock.unlock().unwrap();
        assert!(!fixture.directory.join("network-draft").exists());
        assert!(!fixture.directory.join("network").exists());
        let retry = ConnectionAttempt::new(fixture.checker);
        assert_eq!(
            retry.run(draft(), 0).unwrap(),
            vec![SeedRow::InvalidAddress]
        );
    }
}
