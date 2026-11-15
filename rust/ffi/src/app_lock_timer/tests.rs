use super::{MobileAppLockEvent as Event, MobileAppLockTimer};

#[test]
fn native_timer_projects_core_deadlines_and_all_authorized_events() {
    let timer = MobileAppLockTimer::new(true, 1_000);
    assert!(timer.snapshot(1_000).unwrap().locked);
    let unlocked = timer.transition(Event::Unlocked, 1_000).unwrap();
    assert!(!unlocked.locked);
    assert_eq!(unlocked.remaining_ms, Some(300_000));
    assert!(timer.transition(Event::Background, 1_000).unwrap().obscured);
    let foreground = timer.transition(Event::Foreground, 60_999).unwrap();
    assert!(!foreground.obscured);
    assert!(!foreground.locked);
    assert_eq!(foreground.remaining_ms, Some(300_000));
    let interaction = timer.transition(Event::Interaction, 360_998).unwrap();
    assert_eq!(interaction.remaining_ms, Some(300_000));
    assert!(timer.snapshot(660_998).unwrap().locked);
    let cleared = timer.transition(Event::Cleared, 660_998).unwrap();
    assert!(!cleared.enabled);
    assert!(!cleared.locked);
    assert_eq!(cleared.remaining_ms, None);
    assert!(timer.transition(Event::Enrolled, 660_998).unwrap().enabled);
    assert!(!timer.snapshot(660_998).unwrap().locked);
}

#[test]
fn native_timer_never_unlocks_after_invalid_clock_or_background_authentication() {
    let timer = MobileAppLockTimer::new(true, 1_000);
    timer.transition(Event::Unlocked, 1_000).unwrap();
    assert!(timer.transition(Event::Interaction, 999).is_err());
    assert!(timer.snapshot(999).unwrap().locked);
    timer.transition(Event::Background, 1_000).unwrap();
    assert!(timer.transition(Event::Unlocked, 1_000).is_err());
    assert!(timer.snapshot(1_000).unwrap().locked);
}
