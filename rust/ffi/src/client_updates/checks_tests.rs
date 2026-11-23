use super::{
    checks, client, core, published, MobileUpdateCheck, MobileUpdateCheckFailure,
    MobileUpdateTrigger,
};
use dyapp_test_support::test_dir;

#[test]
fn every_native_trigger_coalesces_under_the_held_update_lease_without_changing_trusted_state() {
    let dir = test_dir().unwrap();
    let lease = published(&dir);
    let updates = client(&dir);
    let before = updates.read_state().unwrap();
    for trigger in [
        MobileUpdateTrigger::LaunchOrResume,
        MobileUpdateTrigger::Timer,
        MobileUpdateTrigger::NetworkRestored,
        MobileUpdateTrigger::Manual,
    ] {
        assert_eq!(updates.check(trigger).unwrap(), MobileUpdateCheck::Busy);
        assert_eq!(updates.read_state().unwrap(), before);
    }
    drop(lease);
}

#[test]
fn failed_completion_preserves_the_trusted_offer_without_becoming_a_success_shaped_result() {
    let dir = test_dir().unwrap();
    let lease = published(&dir);
    let updates = client(&dir);
    let snapshot = lease.load().unwrap();
    let before = updates.project(&snapshot);
    let outcome = core::checker::Outcome::Failed {
        snapshot,
        reason: core::checker::Failure::Transport(core::transport::Error::Status(503)),
    };
    let MobileUpdateCheck::Failed { state, failure } = checks::project(&updates, &outcome) else {
        panic!("Failed completion must not claim validation")
    };
    assert_eq!(state, before);
    assert!(state.offer.is_some());
    assert_eq!(state.last_success, Some(100));
    assert!(matches!(
        failure,
        MobileUpdateCheckFailure::Transport { .. }
    ));
}
