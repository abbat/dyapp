use super::*;
use crate::updates::MIN_RETRY;

#[test]
fn all_triggers_share_one_live_ticket_even_after_interval_changes() {
    let mut checker = Coordinator::default();
    let ticket = checker.begin(100, 0, Trigger::LaunchOrResume).unwrap();
    checker.set_interval(Interval::Week);
    for trigger in [
        Trigger::Manual,
        Trigger::Timer,
        Trigger::NetworkRestored,
        Trigger::LaunchOrResume,
    ] {
        assert!(checker.begin(200, 0, trigger).is_none());
    }
    assert_eq!(
        checker.checkpoint(),
        Checkpoint {
            interval: Interval::Week,
            state: CheckState {
                last_attempt: Some(100),
                ..CheckState::default()
            },
            in_flight: true,
        }
    );
    assert!(checker.finish(ticket, 220, Completion::Validated));
    let saved = checker.checkpoint();
    assert!(!saved.in_flight);
    assert_eq!(saved.state.last_success, Some(220));
    assert_eq!(saved.state.consecutive_failures, 0);
    assert!(checker.begin(221, 0, Trigger::Timer).is_none());
    let manual = checker.begin(221, 0, Trigger::Manual).unwrap();
    assert!(checker.finish(manual, 221, Completion::Validated));
}

#[test]
fn abandoned_attempt_is_reaped_once_and_obeys_failure_backoff() {
    let mut checker = Coordinator::default();
    drop(checker.begin(100, 0, Trigger::Manual).unwrap());
    let saved = checker.checkpoint();
    assert!(!saved.in_flight);
    assert_eq!(saved.state.consecutive_failures, 1);
    assert_eq!(checker.checkpoint(), saved);
    assert!(checker
        .begin(100 + MIN_RETRY - 1, 0, Trigger::Timer)
        .is_none());
    let retry = checker.begin(100 + MIN_RETRY, 0, Trigger::Timer).unwrap();
    assert!(checker.finish(retry, 100 + MIN_RETRY, Completion::Failed));
    assert_eq!(checker.checkpoint().state.consecutive_failures, 2);
    assert!(checker
        .begin(100 + 3 * MIN_RETRY - 1, 0, Trigger::LaunchOrResume)
        .is_none());
    let retry = checker
        .begin(100 + 3 * MIN_RETRY, 0, Trigger::LaunchOrResume)
        .unwrap();
    assert!(checker.finish(retry, 100 + 3 * MIN_RETRY, Completion::Validated));
}

#[test]
fn a_foreign_completion_cannot_finish_the_current_request() {
    let mut first = Coordinator::default();
    let mut second = Coordinator::default();
    let owned = first.begin(100, 0, Trigger::Manual).unwrap();
    let foreign = second.begin(100, 0, Trigger::Manual).unwrap();
    assert!(!first.finish(foreign, 200, Completion::Validated));
    assert!(first.checkpoint().in_flight);
    assert_eq!(first.checkpoint().state.last_success, None);
    assert_eq!(second.checkpoint().state.consecutive_failures, 1);
    assert!(first.finish(owned, 200, Completion::Validated));
    assert_eq!(first.checkpoint().state.last_success, Some(200));
}

#[test]
fn restart_accounts_for_an_interrupted_request_without_repeating_the_failure() {
    let mut checker = Coordinator::default();
    checker.set_interval(Interval::TwelveHours);
    let ticket = checker.begin(100, 0, Trigger::Manual).unwrap();
    let interrupted = checker.checkpoint();
    drop(ticket);
    drop(checker);
    let mut restored = Coordinator::restore(interrupted);
    let recovered = restored.checkpoint();
    assert_eq!(recovered.interval, Interval::TwelveHours);
    assert!(!recovered.in_flight);
    assert_eq!(recovered.state.consecutive_failures, 1);
    assert!(restored
        .begin(100 + MIN_RETRY - 1, 0, Trigger::Timer)
        .is_none());
    let mut restored_again = Coordinator::restore(recovered);
    assert_eq!(restored_again.checkpoint(), recovered);
    let ticket = restored_again
        .begin(100 + MIN_RETRY, 0, Trigger::Timer)
        .unwrap();
    assert!(restored_again.finish(ticket, 100 + MIN_RETRY, Completion::Validated));
}

#[test]
fn rollback_gets_one_catchup_then_retains_backoff_even_if_completion_clock_moves() {
    let mut checker = Coordinator::restore(Checkpoint {
        interval: Interval::Day,
        state: CheckState {
            last_attempt: Some(1_000),
            last_success: Some(2_000),
            consecutive_failures: 0,
        },
        in_flight: false,
    });
    let ticket = checker.begin(100, 0, Trigger::Timer).unwrap();
    assert!(checker.finish(ticket, 99, Completion::Failed));
    let recovered = checker.checkpoint().state;
    assert_eq!(recovered.last_attempt, Some(99));
    assert_eq!(recovered.last_success, Some(99));
    assert_eq!(recovered.consecutive_failures, 1);
    assert!(checker
        .begin(99 + MIN_RETRY - 1, 0, Trigger::NetworkRestored)
        .is_none());
    let retry = checker
        .begin(99 + MIN_RETRY, 0, Trigger::NetworkRestored)
        .unwrap();
    assert!(checker.finish(retry, 99 + MIN_RETRY, Completion::Validated));
}

#[test]
fn interval_changes_reuse_cadence_and_failure_counts_saturate() {
    let mut checker = Coordinator::restore(Checkpoint {
        interval: Interval::Week,
        state: CheckState {
            last_attempt: Some(100),
            last_success: Some(100),
            consecutive_failures: 0,
        },
        in_flight: false,
    });
    let now = 100 + Interval::SixHours.seconds();
    assert!(checker.begin(now, 0, Trigger::LaunchOrResume).is_none());
    checker.set_interval(Interval::SixHours);
    let ticket = checker.begin(now, 0, Trigger::LaunchOrResume).unwrap();
    assert!(checker.finish(ticket, now, Completion::Validated));
    let saved = checker.checkpoint();
    let mut saturated = Coordinator::restore(Checkpoint {
        state: CheckState {
            consecutive_failures: u32::MAX,
            ..saved.state
        },
        ..saved
    });
    let ticket = saturated.begin(now, 0, Trigger::Manual).unwrap();
    assert!(saturated.finish(ticket, now, Completion::Failed));
    assert_eq!(saturated.checkpoint().state.consecutive_failures, u32::MAX);
}
