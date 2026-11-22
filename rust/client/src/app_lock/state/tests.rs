mod initial;
mod transitions;
mod uncertain;

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Event {
    Read(Entry),
    Write(Entry),
    Remove(Entry),
    Verify,
}

#[derive(Default)]
struct Data {
    entries: Vec<(Entry, Vec<u8>)>,
    events: Vec<Event>,
    failure: Option<(Event, bool)>,
}

#[derive(Default)]
struct Memory {
    data: Mutex<Data>,
}

impl Memory {
    fn access<T>(&self, event: Event, action: impl FnOnce(&mut Data) -> T) -> Result<T, Error> {
        let mut data = self.data.lock().unwrap();
        data.events.push(event);
        let failure = data
            .failure
            .filter(|(wanted, _)| *wanted == event)
            .map(|(_, after)| after);
        if failure.is_some() {
            data.failure = None;
        }
        if failure == Some(false) {
            return Err(Error::Storage("injected protected IO failure".into()));
        }
        let result = action(&mut data);
        if failure == Some(true) {
            return Err(Error::Storage("injected protected IO failure".into()));
        }
        Ok(result)
    }

    fn seed(&self, entry: Entry, bytes: Vec<u8>) {
        let mut data = self.data.lock().unwrap();
        data.entries.retain(|(key, _)| *key != entry);
        data.entries.push((entry, bytes));
    }

    fn raw(&self, entry: Entry) -> Option<Vec<u8>> {
        self.data
            .lock()
            .unwrap()
            .entries
            .iter()
            .find(|(key, _)| *key == entry)
            .map(|(_, data)| data.clone())
    }

    fn failures(&self, format: Format) -> u8 {
        Saved::decode(format, &self.raw(Entry::Record).unwrap())
            .unwrap()
            .failures
    }

    fn fail(&self, event: Event) {
        self.data.lock().unwrap().failure = Some((event, false));
    }

    fn fail_after(&self, event: Event) {
        self.data.lock().unwrap().failure = Some((event, true));
    }

    fn event(&self, event: Event) {
        self.data.lock().unwrap().events.push(event);
    }

    fn take_events(&self) -> Vec<Event> {
        std::mem::take(&mut self.data.lock().unwrap().events)
    }
}

impl Vault for Memory {
    fn read(&self, entry: Entry) -> Result<Option<Vec<u8>>, Error> {
        self.access(Event::Read(entry), |data| {
            data.entries
                .iter()
                .find(|(key, _)| *key == entry)
                .map(|(_, bytes)| bytes.clone())
        })
    }

    fn write(&self, entry: Entry, bytes: &[u8]) -> Result<(), Error> {
        self.access(Event::Write(entry), |data| {
            data.entries.retain(|(key, _)| *key != entry);
            data.entries.push((entry, bytes.to_vec()));
        })
    }

    fn remove(&self, entry: Entry) -> Result<(), Error> {
        self.access(Event::Remove(entry), |data| {
            data.entries.retain(|(key, _)| *key != entry);
        })
    }
}

// Literal framing from the existing Android and iOS writers; dummy verifier bytes are not a KDF output.
fn legacy(format: Format) -> Vec<u8> {
    match format {
        Format::Android => vec![
            1, 4, 1, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21,
            22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43,
            44, 45, 46, 47,
        ],
        Format::Ios => vec![
            1, 1, 4, 1, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
            21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42,
            43, 44, 45, 46, 47,
        ],
    }
}

fn assert_protected_bytes(actual: &[u8], expected: &[u8]) {
    use subtle::ConstantTimeEq;
    assert!(bool::from(actual.ct_eq(expected)), "protected bytes differ");
}

fn fixture(format: Format, failures: u8) -> (Arc<Memory>, AppLock) {
    let memory = Arc::new(Memory::default());
    let mut data = legacy(format);
    data[if format == Format::Android { 1 } else { 2 }] = failures;
    memory.seed(Entry::Record, data);
    let lock = AppLock::new(format, memory.clone());
    (memory, lock)
}

#[test]
fn literal_legacy_records_keep_exact_layout_counters_and_modes() {
    for format in [Format::Android, Format::Ios] {
        let bytes = legacy(format);
        let saved = Saved::decode(format, &bytes).unwrap();
        assert_eq!(saved.mode, Mode::Pin);
        assert_eq!(saved.failures, 4);
        assert_eq!(saved.verifier.len(), 49);
        assert_eq!(saved.verifier[0], 1);
        assert_protected_bytes(&saved.encode(format).unwrap(), &bytes);
    }
    for (bytes, mode) in [([1, 0, 0], Mode::Off), ([1, 2, 0], Mode::Biometric)] {
        let saved = Saved::decode(Format::Ios, &bytes).unwrap();
        assert_eq!(saved.mode, mode);
        assert_protected_bytes(&saved.encode(Format::Ios).unwrap(), &bytes);
    }
}

#[test]
fn corrupt_records_never_become_fresh_enrollment_even_with_other_markers() {
    for format in [Format::Android, Format::Ios] {
        let (memory, lock) = fixture(format, 0);
        memory.seed(Entry::Record, vec![1, 2]);
        memory.seed(Entry::Skipped, vec![]);
        memory.seed(Entry::Biometric, vec![]);
        assert_eq!(lock.mode(), Err(Error::InvalidRecord));
        assert_eq!(lock.enroll("123456".into()), Err(Error::InvalidRecord));
        assert_eq!(lock.attempt("123456".into()), Err(Error::InvalidRecord));
    }
    assert!(Saved::decode(Format::Ios, &[1, 255, 0]).is_err());
    for format in [Format::Android, Format::Ios] {
        let mut bytes = legacy(format);
        bytes[0] = 0;
        assert!(Saved::decode(format, &bytes).is_err());
        bytes[0] = 1;
        bytes[if format == Format::Android { 1 } else { 2 }] = 6;
        assert!(Saved::decode(format, &bytes).is_err());
        bytes.pop();
        assert!(Saved::decode(format, &bytes).is_err());
    }
}

#[test]
fn persisted_failure_limit_survives_new_handles_and_does_not_call_verifier_again() {
    for format in [Format::Android, Format::Ios] {
        let (memory, _) = fixture(format, 0);
        let mut verified = 0;
        for failures in 1..=5 {
            let lock = AppLock::new(format, memory.clone());
            let result = lock
                .attempt_with("wrong", |_, _| {
                    verified += 1;
                    Ok(false)
                })
                .unwrap();
            assert_eq!(memory.failures(format), failures);
            assert_eq!(
                result,
                if failures == 5 {
                    Attempt::DeviceCredentialRequired
                } else {
                    Attempt::Wrong
                }
            );
        }
        assert_eq!(verified, 5);
        let lock = AppLock::new(format, memory.clone());
        assert_eq!(
            lock.attempt_with("123456", |_, _| panic!("limit must precede KDF")),
            Ok(Attempt::DeviceCredentialRequired)
        );
        assert_eq!(memory.failures(format), 5);
    }
}

#[test]
fn increment_is_durable_before_verification_and_verifier_errors_consume_it() {
    for format in [Format::Android, Format::Ios] {
        let (memory, lock) = fixture(format, 0);
        assert_eq!(
            lock.attempt_with("wrong", |_, _| {
                assert_eq!(memory.failures(format), 1);
                memory.event(Event::Verify);
                Ok(false)
            }),
            Ok(Attempt::Wrong)
        );
        assert_eq!(
            memory.take_events(),
            vec![
                Event::Read(Entry::Record),
                Event::Write(Entry::Record),
                Event::Verify
            ]
        );
        assert_eq!(
            lock.attempt("not digits".into()),
            Err(Error::Pin(pin::Error::InvalidPin))
        );
        assert_eq!(memory.failures(format), 2);
        assert_eq!(
            lock.attempt_with("123456", |_, _| Err(pin::Error::Unavailable)),
            Err(Error::Pin(pin::Error::Unavailable))
        );
        assert_eq!(memory.failures(format), 3);
    }
}

#[test]
fn failed_increment_never_runs_verifier_and_failed_reset_never_reports_unlocked() {
    for format in [Format::Android, Format::Ios] {
        let (memory, lock) = fixture(format, 0);
        memory.fail(Event::Write(Entry::Record));
        assert!(matches!(
            lock.attempt_with("123456", |_, _| panic!("write must precede verify")),
            Err(Error::Storage(_))
        ));
        assert_eq!(memory.failures(format), 0);
        memory.take_events();
        assert!(matches!(
            lock.attempt_with("123456", |_, _| {
                memory.event(Event::Verify);
                memory.fail(Event::Write(Entry::Record));
                Ok(true)
            }),
            Err(Error::Storage(_))
        ));
        assert_eq!(memory.failures(format), 1);
        assert_eq!(
            memory.take_events(),
            vec![
                Event::Read(Entry::Record),
                Event::Write(Entry::Record),
                Event::Verify,
                Event::Write(Entry::Record)
            ]
        );
        let restarted = AppLock::new(format, memory.clone());
        assert_eq!(
            restarted.attempt_with("123456", |_, _| Ok(true)),
            Ok(Attempt::Unlocked)
        );
        assert_eq!(memory.failures(format), 0);
    }
}

#[test]
fn correct_fifth_attempt_is_allowed_but_requires_a_durable_counter_reset() {
    for format in [Format::Android, Format::Ios] {
        let (memory, lock) = fixture(format, 4);
        assert_eq!(
            lock.attempt_with("123456", |_, _| {
                assert_eq!(memory.failures(format), 5);
                Ok(true)
            }),
            Ok(Attempt::Unlocked)
        );
        assert_eq!(memory.failures(format), 0);
    }
}

#[test]
fn unsupported_verifier_version_preserves_legacy_outer_record_admission() {
    for format in [Format::Android, Format::Ios] {
        let (memory, lock) = fixture(format, 0);
        let mut bytes = memory.raw(Entry::Record).unwrap();
        bytes[if format == Format::Android { 2 } else { 3 }] = 2;
        memory.seed(Entry::Record, bytes);
        if format == Format::Android {
            assert_eq!(lock.mode(), Err(Error::InvalidRecord));
        } else {
            assert_eq!(lock.mode(), Ok(Mode::Pin));
            assert_eq!(
                lock.attempt("123456".into()),
                Err(Error::Pin(pin::Error::InvalidRecord))
            );
            assert_eq!(memory.failures(format), 1);
        }
    }
}

#[test]
fn io_read_failures_are_not_missing_records_or_new_setup() {
    for format in [Format::Android, Format::Ios] {
        let (memory, lock) = fixture(format, 0);
        memory.fail(Event::Read(Entry::Record));
        assert!(matches!(lock.mode(), Err(Error::Storage(_))));
        assert_eq!(lock.mode(), Ok(Mode::Pin));
    }
}
