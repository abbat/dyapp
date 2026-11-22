use super::*;

fn assert_read_only(memory: &Memory) {
    assert!(memory
        .take_events()
        .iter()
        .all(|event| matches!(event, Event::Read(_))));
}

#[test]
fn initial_choices_accept_new_and_off_but_never_replace_protected_modes() {
    for format in [Format::Android, Format::Ios] {
        for skipped in [false, true] {
            let memory = Arc::new(Memory::default());
            let lock = AppLock::new(format, memory.clone());
            if skipped {
                lock.skip().unwrap();
            }
            lock.use_biometric_initial().unwrap();
            let before = memory.data.lock().unwrap().entries.clone();
            memory.take_events();
            assert_eq!(
                lock.enroll_initial("bad".into()),
                Err(Error::AlreadyEnabled)
            );
            assert_eq!(lock.use_biometric_initial(), Err(Error::AlreadyEnabled));
            assert_read_only(&memory);
            assert_eq!(memory.data.lock().unwrap().entries, before);
            assert_eq!(lock.mode(), Ok(Mode::Biometric));

            lock.reset_after_device_credential(None).unwrap();
            lock.enroll_initial("123456".into()).unwrap();
            let before = memory.raw(Entry::Record).unwrap();
            memory.take_events();
            assert_eq!(lock.use_biometric_initial(), Err(Error::AlreadyEnabled));
            assert_eq!(
                lock.enroll_initial("654321".into()),
                Err(Error::AlreadyEnabled)
            );
            assert_read_only(&memory);
            assert_protected_bytes(&memory.raw(Entry::Record).unwrap(), &before);
            assert_eq!(lock.mode(), Ok(Mode::Pin));
            // The distinct authenticated recovery path still replaces an existing PIN.
            lock.reset_after_device_credential(Some("654321".into()))
                .unwrap();
            assert_eq!(lock.attempt("654321".into()), Ok(Attempt::Unlocked));
        }
    }
}

#[test]
fn initial_refusal_preserves_failure_count_and_all_protected_record_bytes() {
    for format in [Format::Android, Format::Ios] {
        let (memory, lock) = fixture(format, MAX_FAILURES);
        let before = memory.raw(Entry::Record).unwrap();
        assert_eq!(
            lock.enroll_initial("123456".into()),
            Err(Error::AlreadyEnabled)
        );
        assert_eq!(lock.use_biometric_initial(), Err(Error::AlreadyEnabled));
        assert_read_only(&memory);
        assert_protected_bytes(&memory.raw(Entry::Record).unwrap(), &before);
        assert_eq!(memory.failures(format), MAX_FAILURES);
    }
}

#[test]
fn failed_initial_admission_does_not_create_markers_or_overwrite_invalid_records() {
    for format in [Format::Android, Format::Ios] {
        let memory = Arc::new(Memory::default());
        let lock = AppLock::new(format, memory.clone());
        memory.fail(Event::Read(Entry::Record));
        assert!(matches!(
            lock.use_biometric_initial(),
            Err(Error::Storage(_))
        ));
        assert_read_only(&memory);
        assert_eq!(lock.mode(), Ok(Mode::New));
        memory.seed(Entry::Record, vec![255]);
        assert_eq!(
            lock.enroll_initial("123456".into()),
            Err(Error::InvalidRecord)
        );
        assert_eq!(lock.use_biometric_initial(), Err(Error::InvalidRecord));
        assert_read_only(&memory);
        assert_eq!(memory.raw(Entry::Record), Some(vec![255]));
        assert!(memory.raw(Entry::Biometric).is_none());
    }
}

#[derive(Default)]
struct GuardProbe {
    memory: Memory,
    owner: std::sync::OnceLock<std::sync::Weak<AppLock>>,
}

impl GuardProbe {
    fn assert_guarded(&self) {
        let lock = self.owner.get().unwrap().upgrade().unwrap();
        assert!(matches!(
            lock.operation.try_lock(),
            Err(std::sync::TryLockError::WouldBlock)
        ));
    }
}

impl Vault for GuardProbe {
    fn read(&self, entry: Entry) -> Result<Option<Vec<u8>>, Error> {
        self.assert_guarded();
        self.memory.read(entry)
    }

    fn write(&self, entry: Entry, bytes: &[u8]) -> Result<(), Error> {
        self.assert_guarded();
        self.memory.write(entry, bytes)
    }

    fn remove(&self, entry: Entry) -> Result<(), Error> {
        self.assert_guarded();
        self.memory.remove(entry)
    }
}

#[test]
fn initial_admission_and_durable_io_retain_the_same_operation_guard() {
    let storage = Arc::new(GuardProbe::default());
    let lock = Arc::new(AppLock::new(Format::Android, storage.clone()));
    storage.owner.set(Arc::downgrade(&lock)).unwrap();
    lock.use_biometric_initial().unwrap();
    assert_eq!(
        lock.enroll_initial("123456".into()),
        Err(Error::AlreadyEnabled)
    );
    lock.reset_after_device_credential(None).unwrap();
    lock.enroll_initial("123456".into()).unwrap();
    assert_eq!(lock.mode(), Ok(Mode::Pin));
}
