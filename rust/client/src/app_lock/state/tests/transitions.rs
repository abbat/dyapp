use super::*;

#[test]
fn skip_and_biometric_choices_keep_setup_done_and_allow_explicit_os_recovery() {
    for format in [Format::Android, Format::Ios] {
        let memory = Arc::new(Memory::default());
        let lock = AppLock::new(format, memory);
        assert_eq!(lock.mode(), Ok(Mode::New));
        assert_eq!(
            lock.reset_after_device_credential(None),
            Err(Error::NotEnabled)
        );
        lock.skip().unwrap();
        assert_eq!(lock.mode(), Ok(Mode::Off));
        lock.use_biometric().unwrap();
        lock.skip().unwrap();
        assert_eq!(lock.mode(), Ok(Mode::Biometric));
        lock.reset_after_device_credential(Some("123456".into()))
            .unwrap();
        assert_eq!(lock.mode(), Ok(Mode::Pin));
        assert_eq!(lock.attempt("123456".into()), Ok(Attempt::Unlocked));
        lock.reset_after_device_credential(None).unwrap();
        assert_eq!(lock.mode(), Ok(Mode::Off));
        assert_eq!(lock.attempt("123456".into()), Err(Error::NotEnrolled));
        assert_eq!(
            lock.reset_after_device_credential(None),
            Err(Error::NotEnabled)
        );
    }
}

#[test]
fn changing_pin_requires_current_authentication_and_limit_cannot_be_bypassed() {
    for format in [Format::Android, Format::Ios] {
        let memory = Arc::new(Memory::default());
        let lock = AppLock::new(format, memory.clone());
        lock.enroll("123456".into()).unwrap();
        assert_eq!(lock.enroll("654321".into()), Err(Error::AlreadyEnrolled));
        assert_eq!(
            lock.change("654321".into(), Some("111111".into())),
            Ok(Attempt::Wrong)
        );
        assert_eq!(memory.failures(format), 1);
        assert_eq!(
            lock.change("123456".into(), Some("111111".into())),
            Ok(Attempt::Unlocked)
        );
        assert_eq!(lock.attempt("123456".into()), Ok(Attempt::Wrong));
        assert_eq!(lock.change("111111".into(), None), Ok(Attempt::Unlocked));
        assert_eq!(lock.mode(), Ok(Mode::Off));
        lock.enroll("222222".into()).unwrap();
        for _ in 0..4 {
            assert_eq!(lock.attempt("654321".into()), Ok(Attempt::Wrong));
        }
        assert_eq!(
            lock.change("654321".into(), None),
            Ok(Attempt::DeviceCredentialRequired)
        );
        assert_eq!(
            lock.change("222222".into(), None),
            Ok(Attempt::DeviceCredentialRequired)
        );
        assert_eq!(lock.mode(), Ok(Mode::Pin));
        lock.reset_after_device_credential(Some("222222".into()))
            .unwrap();
        assert_eq!(lock.attempt("222222".into()), Ok(Attempt::Unlocked));
    }
}

#[test]
fn invalid_replacement_and_failed_record_write_leave_existing_record_and_mode() {
    for format in [Format::Android, Format::Ios] {
        let (memory, lock) = fixture(format, 0);
        let before = memory.raw(Entry::Record).unwrap();
        assert_eq!(
            lock.reset_after_device_credential(Some("bad".into())),
            Err(Error::Pin(pin::Error::InvalidPin))
        );
        assert_protected_bytes(&memory.raw(Entry::Record).unwrap(), &before);
        memory.fail(Event::Write(Entry::Record));
        assert!(matches!(
            lock.reset_after_device_credential(Some("123456".into())),
            Err(Error::Storage(_))
        ));
        assert_protected_bytes(&memory.raw(Entry::Record).unwrap(), &before);
        assert_eq!(lock.mode(), Ok(Mode::Pin));
    }
}

#[test]
fn android_biometric_marker_precedes_pin_removal_and_failed_steps_keep_pin_effective() {
    for event in [
        Event::Write(Entry::Biometric),
        Event::Read(Entry::Record),
        Event::Remove(Entry::Record),
    ] {
        let (memory, lock) = fixture(Format::Android, 0);
        memory.fail(event);
        assert!(matches!(lock.use_biometric(), Err(Error::Storage(_))));
        assert_eq!(lock.mode(), Ok(Mode::Pin));
        assert!(memory.raw(Entry::Record).is_some());
    }
    let (memory, lock) = fixture(Format::Android, 0);
    lock.use_biometric().unwrap();
    assert_eq!(
        memory.take_events(),
        vec![
            Event::Write(Entry::Biometric),
            Event::Read(Entry::Record),
            Event::Remove(Entry::Record)
        ]
    );
    assert_eq!(lock.mode(), Ok(Mode::Biometric));
    assert!(memory.raw(Entry::Record).is_none());
    lock.reset_after_device_credential(None).unwrap();
    assert_eq!(lock.mode(), Ok(Mode::Off));
}

#[test]
fn ios_biometric_selection_replaces_one_item_and_never_uses_android_markers() {
    let (memory, lock) = fixture(Format::Ios, 0);
    lock.use_biometric().unwrap();
    assert_eq!(memory.take_events(), vec![Event::Write(Entry::Record)]);
    assert_eq!(lock.mode(), Ok(Mode::Biometric));
    assert!(memory.raw(Entry::Biometric).is_none());
    lock.skip().unwrap();
    assert_eq!(lock.mode(), Ok(Mode::Biometric));
    assert!(memory.raw(Entry::Skipped).is_none());
}

#[test]
fn skip_does_not_disable_or_rewrite_an_existing_pin() {
    for format in [Format::Android, Format::Ios] {
        let (memory, lock) = fixture(format, 0);
        let before = memory.raw(Entry::Record).unwrap();
        lock.skip().unwrap();
        assert_eq!(lock.mode(), Ok(Mode::Pin));
        assert_protected_bytes(&memory.raw(Entry::Record).unwrap(), &before);
        assert_eq!(
            memory.raw(Entry::Skipped).is_some(),
            format == Format::Android
        );
    }
}

#[test]
fn existing_pin_wins_over_android_markers_and_invalid_enrollment_writes_nothing() {
    let (memory, lock) = fixture(Format::Android, 0);
    memory.seed(Entry::Skipped, vec![]);
    memory.seed(Entry::Biometric, vec![]);
    assert_eq!(lock.mode(), Ok(Mode::Pin));
    for format in [Format::Android, Format::Ios] {
        let memory = Arc::new(Memory::default());
        let lock = AppLock::new(format, memory.clone());
        assert_eq!(
            lock.enroll("bad".into()),
            Err(Error::Pin(pin::Error::InvalidPin))
        );
        assert_eq!(lock.mode(), Ok(Mode::New));
        assert!(memory.raw(Entry::Record).is_none());
    }
}
