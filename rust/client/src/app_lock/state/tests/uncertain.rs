use super::*;

#[test]
fn failed_acknowledgement_after_increment_is_not_refunded_or_verified() {
    for format in [Format::Android, Format::Ios] {
        let (memory, lock) = fixture(format, 0);
        memory.fail_after(Event::Write(Entry::Record));
        assert!(matches!(
            lock.attempt_with("123456", |_, _| panic!(
                "unconfirmed write must stop verification"
            )),
            Err(Error::Storage(_))
        ));
        assert_eq!(memory.failures(format), 1);
        let restarted = AppLock::new(format, memory.clone());
        assert_eq!(
            restarted.attempt_with("wrong", |_, _| Ok(false)),
            Ok(Attempt::Wrong)
        );
        assert_eq!(memory.failures(format), 2);
    }
}

#[test]
fn failed_reset_acknowledgement_does_not_report_unlocked_even_when_zero_committed() {
    for format in [Format::Android, Format::Ios] {
        let (memory, lock) = fixture(format, 0);
        assert!(matches!(
            lock.attempt_with("123456", |_, _| {
                memory.fail_after(Event::Write(Entry::Record));
                Ok(true)
            }),
            Err(Error::Storage(_))
        ));
        assert_eq!(memory.failures(format), 0);
        assert_eq!(lock.mode(), Ok(Mode::Pin));
    }
}

#[test]
fn interrupted_android_biometric_switch_reopens_as_pin_or_biometric_never_unlocked() {
    for event in [Event::Write(Entry::Biometric), Event::Remove(Entry::Record)] {
        let (memory, lock) = fixture(Format::Android, 0);
        memory.fail_after(event);
        assert!(matches!(lock.use_biometric(), Err(Error::Storage(_))));
        let restarted = AppLock::new(Format::Android, memory.clone());
        assert_eq!(
            restarted.mode(),
            Ok(if event == Event::Write(Entry::Biometric) {
                Mode::Pin
            } else {
                Mode::Biometric
            })
        );
        assert!(memory.raw(Entry::Biometric).is_some());
    }
}

#[test]
fn protected_write_error_does_not_claim_mode_or_replacement_was_rolled_back() {
    for format in [Format::Android, Format::Ios] {
        let (memory, lock) = fixture(format, 0);
        memory.fail_after(Event::Write(Entry::Record));
        assert!(matches!(
            lock.reset_after_device_credential(Some("123456".into())),
            Err(Error::Storage(_))
        ));
        let restarted = AppLock::new(format, memory);
        assert_eq!(restarted.mode(), Ok(Mode::Pin));
        assert_eq!(restarted.attempt("123456".into()), Ok(Attempt::Unlocked));
    }
    let (memory, lock) = fixture(Format::Ios, 0);
    memory.fail_after(Event::Write(Entry::Record));
    assert!(matches!(lock.use_biometric(), Err(Error::Storage(_))));
    assert_eq!(
        AppLock::new(Format::Ios, memory).mode(),
        Ok(Mode::Biometric)
    );
}
