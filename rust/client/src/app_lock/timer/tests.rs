use super::{Error, Event, Timer};

#[test]
fn fresh_process_and_quick_return_preserve_the_existing_session_contract() {
    let mut timer = Timer::new(true, 1_000);
    assert!(timer.snapshot(1_000).locked);
    assert_eq!(timer.snapshot(1_000).remaining_ms, None);
    assert_eq!(
        timer.apply(Event::Unlocked, 1_000).unwrap().remaining_ms,
        Some(300_000)
    );
    assert!(timer.apply(Event::Background, 1_000).unwrap().obscured);
    assert!(!timer.snapshot(60_999).locked);
    let returned = timer.apply(Event::Foreground, 60_999).unwrap();
    assert!(!returned.obscured);
    assert!(!returned.locked);
    assert_eq!(returned.remaining_ms, Some(300_000));
}

#[test]
fn exact_background_expiration_precedes_late_input_and_foreground_return() {
    let mut timer = Timer::new(true, 1_000);
    timer.apply(Event::Unlocked, 1_000).unwrap();
    timer.apply(Event::Background, 1_000).unwrap();
    assert_eq!(timer.snapshot(60_999).remaining_ms, Some(1));
    assert!(timer.apply(Event::Interaction, 61_000).unwrap().locked);
    assert!(timer.apply(Event::Foreground, 61_000).unwrap().locked);
    assert_eq!(timer.snapshot(61_000).remaining_ms, None);
}

#[test]
fn foreground_interaction_resets_idle_but_cannot_revive_an_expired_session() {
    let mut timer = Timer::new(true, 1_000);
    timer.apply(Event::Unlocked, 1_000).unwrap();
    assert_eq!(timer.snapshot(300_999).remaining_ms, Some(1));
    assert_eq!(
        timer
            .apply(Event::Interaction, 300_999)
            .unwrap()
            .remaining_ms,
        Some(300_000)
    );
    assert!(timer.apply(Event::Interaction, 600_999).unwrap().locked);
}

#[test]
fn delayed_callbacks_lock_after_both_deadlines_without_requiring_an_exact_tick() {
    for (background_at, late_at) in [(None, 301_001), (Some(2_000), 62_001)] {
        let mut timer = Timer::new(true, 1_000);
        timer.apply(Event::Unlocked, 1_000).unwrap();
        if let Some(background_at) = background_at {
            timer.apply(Event::Background, background_at).unwrap();
        }
        let expired = timer.apply(Event::Interaction, late_at).unwrap();
        assert!(expired.locked);
        assert_eq!(expired.remaining_ms, None);
        assert!(timer.apply(Event::Foreground, late_at).unwrap().locked);
    }
}

#[test]
fn background_input_does_not_extend_grace() {
    let mut timer = Timer::new(true, 1_000);
    timer.apply(Event::Unlocked, 1_000).unwrap();
    timer.apply(Event::Background, 1_000).unwrap();
    assert_eq!(
        timer
            .apply(Event::Interaction, 60_999)
            .unwrap()
            .remaining_ms,
        Some(1)
    );
    assert!(timer.snapshot(61_000).locked);
}

#[test]
fn disabled_mode_enrollment_and_authorized_clear_keep_existing_behavior() {
    let mut timer = Timer::new(false, 1_000);
    assert!(!timer.snapshot(1_000).locked);
    assert!(timer.apply(Event::Background, 1_000).unwrap().obscured);
    assert!(!timer.apply(Event::Foreground, 121_000).unwrap().locked);
    let enrolled = timer.apply(Event::Enrolled, 121_000).unwrap();
    assert!(enrolled.enabled);
    assert!(!enrolled.locked);
    assert!(timer.snapshot(421_000).locked);
    let cleared = timer.apply(Event::Cleared, 421_000).unwrap();
    assert!(!cleared.enabled);
    assert!(!cleared.locked);
    assert_eq!(cleared.remaining_ms, None);
}

#[test]
fn unlock_requires_enabled_foreground_protection() {
    let mut timer = Timer::new(false, 1_000);
    assert_eq!(
        timer.apply(Event::Unlocked, 1_000),
        Err(Error::NotForeground)
    );
    timer.apply(Event::Enrolled, 1_000).unwrap();
    timer.apply(Event::Background, 1_000).unwrap();
    assert_eq!(
        timer.apply(Event::Unlocked, 61_000),
        Err(Error::NotForeground)
    );
    assert!(timer.snapshot(61_000).locked);
}

#[test]
fn backwards_clock_fails_closed_and_integer_maximum_never_overflows() {
    let mut timer = Timer::new(true, 1_000);
    timer.apply(Event::Unlocked, 1_000).unwrap();
    assert_eq!(
        timer.apply(Event::Interaction, 999),
        Err(Error::ClockWentBackwards)
    );
    assert!(timer.snapshot(999).locked);
    let mut timer = Timer::new(true, u64::MAX);
    timer.apply(Event::Unlocked, u64::MAX).unwrap();
    assert_eq!(timer.snapshot(u64::MAX).remaining_ms, Some(300_000));
    assert!(timer.snapshot(0).locked);
}
