use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Fail {
    Read,
    Write,
    Remove,
}

#[derive(Default)]
struct Data {
    entries: Vec<(MobileAppLockEntry, Vec<u8>)>,
    failure: Option<Fail>,
}

#[derive(Default)]
struct Protected {
    data: Mutex<Data>,
    calls: AtomicUsize,
}

impl Protected {
    fn access<T>(
        &self,
        kind: Fail,
        action: impl FnOnce(&mut Data) -> T,
    ) -> Result<T, AppLockStorageError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        let mut data = self.data.lock().unwrap();
        if data.failure == Some(kind) {
            data.failure = None;
            return Err(AppLockStorageError::Failed {
                reason: "protected IO refused".into(),
            });
        }
        Ok(action(&mut data))
    }

    fn fail(&self, kind: Fail) {
        self.data.lock().unwrap().failure = Some(kind);
    }
}

impl MobileAppLockStorage for Protected {
    fn read_entry(
        &self,
        entry: MobileAppLockEntry,
    ) -> Result<Option<Vec<u8>>, AppLockStorageError> {
        self.access(Fail::Read, |data| {
            data.entries
                .iter()
                .find(|(key, _)| *key == entry)
                .map(|(_, bytes)| bytes.clone())
        })
    }

    fn write_entry(
        &self,
        entry: MobileAppLockEntry,
        bytes: Vec<u8>,
    ) -> Result<(), AppLockStorageError> {
        self.access(Fail::Write, |data| {
            data.entries.retain(|(key, _)| *key != entry);
            data.entries.push((entry, bytes));
        })
    }

    fn remove_entry(&self, entry: MobileAppLockEntry) -> Result<(), AppLockStorageError> {
        self.access(Fail::Remove, |data| {
            data.entries.retain(|(key, _)| *key != entry);
        })
    }
}

fn error<T>(result: Result<T, CoreError>) -> String {
    match result {
        Err(error) => error.to_string(),
        Ok(_) => panic!("expected protected IO failure"),
    }
}

#[test]
fn native_handle_retains_storage_and_projects_the_core_transitions_for_both_formats() {
    for format in [MobileAppLockFormat::Android, MobileAppLockFormat::Ios] {
        let storage = Arc::new(Protected::default());
        let weak = Arc::downgrade(&storage);
        let lock = MobileAppLock::new(format, storage.clone());
        assert_eq!(storage.calls.load(Ordering::Relaxed), 0);
        drop(storage);
        assert!(weak.upgrade().is_some());
        assert_eq!(lock.mode().unwrap(), MobileAppLockMode::New);
        lock.skip().unwrap();
        assert_eq!(lock.mode().unwrap(), MobileAppLockMode::Off);
        lock.use_biometric().unwrap();
        assert_eq!(lock.mode().unwrap(), MobileAppLockMode::Biometric);
        lock.reset_after_device_credential(Some("123456".into()))
            .unwrap();
        assert_eq!(lock.mode().unwrap(), MobileAppLockMode::Pin);
        assert_eq!(
            lock.attempt("654321".into()).unwrap(),
            MobilePinAttempt::Wrong
        );
        assert_eq!(
            lock.attempt("123456".into()).unwrap(),
            MobilePinAttempt::Unlocked
        );
        assert_eq!(
            lock.change("123456".into(), None).unwrap(),
            MobilePinAttempt::Unlocked
        );
        assert_eq!(lock.mode().unwrap(), MobileAppLockMode::Off);
        drop(lock);
        assert!(weak.upgrade().is_none());
    }
}

#[test]
fn native_initial_setup_cannot_replace_another_protected_choice() {
    for format in [MobileAppLockFormat::Android, MobileAppLockFormat::Ios] {
        let storage = Arc::new(Protected::default());
        let lock = MobileAppLock::new(format, storage.clone());
        lock.use_biometric_initial().unwrap();
        assert_eq!(
            error(lock.enroll_initial("123456".into())),
            "App lock is already enabled"
        );
        assert_eq!(
            error(lock.use_biometric_initial()),
            "App lock is already enabled"
        );
        assert_eq!(lock.mode().unwrap(), MobileAppLockMode::Biometric);
        lock.reset_after_device_credential(None).unwrap();
        lock.enroll_initial("123456".into()).unwrap();
        let before = storage.read_entry(MobileAppLockEntry::Record).unwrap();
        assert_eq!(
            error(lock.use_biometric_initial()),
            "App lock is already enabled"
        );
        assert_eq!(
            storage.read_entry(MobileAppLockEntry::Record).unwrap(),
            before
        );
        assert_eq!(lock.mode().unwrap(), MobileAppLockMode::Pin);
    }
}

#[test]
fn native_protected_io_errors_are_not_absence_or_success() {
    for format in [MobileAppLockFormat::Android, MobileAppLockFormat::Ios] {
        let storage = Arc::new(Protected::default());
        let lock = MobileAppLock::new(format, storage.clone());
        storage.fail(Fail::Read);
        assert_eq!(error(lock.mode()), "protected IO refused");
        storage.fail(Fail::Write);
        assert_eq!(error(lock.skip()), "protected IO refused");
        assert_eq!(lock.mode().unwrap(), MobileAppLockMode::New);
        assert_eq!(
            error(lock.enroll("bad".into())),
            "PIN must contain 6 to 64 decimal digits"
        );
        assert_eq!(lock.mode().unwrap(), MobileAppLockMode::New);
    }
}

#[test]
fn native_android_removal_failure_keeps_pin_precedence_after_marker_creation() {
    let storage = Arc::new(Protected::default());
    let lock = MobileAppLock::new(MobileAppLockFormat::Android, storage.clone());
    lock.enroll("123456".into()).unwrap();
    storage.fail(Fail::Remove);
    assert_eq!(error(lock.use_biometric()), "protected IO refused");
    assert_eq!(lock.mode().unwrap(), MobileAppLockMode::Pin);
    assert_eq!(
        lock.attempt("123456".into()).unwrap(),
        MobilePinAttempt::Unlocked
    );
    lock.use_biometric().unwrap();
    assert_eq!(lock.mode().unwrap(), MobileAppLockMode::Biometric);
}

#[test]
fn native_legacy_failure_limit_refuses_pin_and_change_without_rewriting_record() {
    for format in [MobileAppLockFormat::Android, MobileAppLockFormat::Ios] {
        let storage = Arc::new(Protected::default());
        // Legacy outer headers with a dummy verifier: the limit must precede KDF work.
        let record = match format {
            MobileAppLockFormat::Android => {
                let mut bytes = vec![1; 51];
                bytes[1] = 5;
                bytes
            }
            MobileAppLockFormat::Ios => {
                let mut bytes = vec![1; 52];
                bytes[2] = 5;
                bytes
            }
        };
        storage
            .write_entry(MobileAppLockEntry::Record, record)
            .unwrap();
        let lock = MobileAppLock::new(format, storage.clone());
        let calls = storage.calls.load(Ordering::Relaxed);
        assert_eq!(
            lock.attempt("123456".into()).unwrap(),
            MobilePinAttempt::DeviceCredentialRequired
        );
        assert_eq!(
            lock.change("123456".into(), Some("654321".into())).unwrap(),
            MobilePinAttempt::DeviceCredentialRequired
        );
        assert_eq!(storage.calls.load(Ordering::Relaxed), calls + 2);
        assert_eq!(lock.mode().unwrap(), MobileAppLockMode::Pin);
    }
}
