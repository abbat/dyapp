use super::*;
use crate::updates::{availability, coordinator::Checkpoint, transport::Cache, CheckState};
use dyapp_test_support::test_dir;
use rusqlite::Connection;

fn versions() -> (Version, Version) {
    (
        Version::parse("1.0.0").unwrap(),
        Version::parse("2.0.0").unwrap(),
    )
}

fn seed(root: &Path) -> storage::LockedState {
    let mut owner = storage::LockedState::try_open(root).unwrap().unwrap();
    let mut snapshot = owner.load().unwrap();
    let bytes = br#"{"schema":1,"channel":"stable","targets":{"android":{"version":"2.0.0","tag":"v2.0.0","method":"package"}}}"#;
    snapshot
        .admit(Cache::restore(bytes.to_vec(), None).unwrap())
        .unwrap();
    snapshot.set_checkpoint(Checkpoint {
        state: CheckState {
            last_attempt: Some(1),
            last_success: Some(1),
            consecutive_failures: 0,
        },
        ..Checkpoint::default()
    });
    owner.save(&snapshot).unwrap();
    owner
}

#[test]
fn independent_actions_commit_during_an_http_lease_and_survive_restart() {
    let root = test_dir().unwrap();
    let owner = seed(&root);
    let (installed, expected) = versions();
    let (claimed, first) =
        claim_notification(&root, Platform::Android, installed, expected).unwrap();
    assert!(first);
    assert!(claimed.notice(Platform::Android, installed).is_some());
    assert!(
        !claim_notification(&root, Platform::Android, installed, expected)
            .unwrap()
            .1
    );
    assert!(
        dismiss(&root, Platform::Android, installed, expected)
            .unwrap()
            .1
    );
    assert!(matches!(
        availability::project(&owner.load().unwrap(), Platform::Android, installed),
        availability::Availability::Available(_)
    ));
    drop(owner);
    assert!(
        !claim_notification(&root, Platform::Android, installed, expected)
            .unwrap()
            .1
    );
    assert!(
        !dismiss(&root, Platform::Android, installed, expected)
            .unwrap()
            .1
    );
}

#[test]
fn a_pending_plan_is_committed_once_and_keeps_the_in_app_notice() {
    let root = test_dir().unwrap();
    let owner = seed(&root);
    let (installed, _) = versions();
    let expected = owner.load().unwrap().notice(Platform::Android, installed);
    let plan = claim_pending_notification(&root, Platform::Android, installed).unwrap();
    assert_eq!(plan, expected);
    assert!(plan.is_some());
    assert!(owner
        .load()
        .unwrap()
        .notice(Platform::Android, installed)
        .is_some());
    assert!(
        claim_pending_notification(&root, Platform::Android, installed)
            .unwrap()
            .is_none()
    );
    drop(owner);
    assert!(
        claim_pending_notification(&root, Platform::Android, installed)
            .unwrap()
            .is_none()
    );
}

#[test]
fn the_plan_selects_the_latest_committed_version_without_a_native_candidate() {
    let root = test_dir().unwrap();
    let mut owner = seed(&root);
    let (installed, _) = versions();
    let bytes = br#"{"schema":1,"channel":"stable","targets":{"android":{"version":"2.1.0","tag":"v2.1.0","method":"package"}}}"#;
    let mut current = owner.load().unwrap();
    current
        .admit(Cache::restore(bytes.to_vec(), None).unwrap())
        .unwrap();
    owner.save(&current).unwrap();
    let plan = claim_pending_notification(&root, Platform::Android, installed)
        .unwrap()
        .unwrap();
    assert_eq!(plan.version(), Version::parse("2.1.0").unwrap());
    assert_eq!(
        plan.release_page(),
        "https://github.com/abbat/dyapp/releases/tag/v2.1.0"
    );
}

#[test]
fn missing_current_other_target_and_dismissed_offers_have_no_plan() {
    let fresh = test_dir().unwrap();
    let (installed, current) = versions();
    assert!(
        claim_pending_notification(&fresh, Platform::Android, installed)
            .unwrap()
            .is_none()
    );
    let root = test_dir().unwrap();
    let owner = seed(&root);
    assert!(
        claim_pending_notification(&root, Platform::WindowsX86_64, installed)
            .unwrap()
            .is_none()
    );
    assert!(
        claim_pending_notification(&root, Platform::Android, current)
            .unwrap()
            .is_none()
    );
    assert!(
        dismiss(&root, Platform::Android, installed, current)
            .unwrap()
            .1
    );
    assert!(
        claim_pending_notification(&root, Platform::Android, installed)
            .unwrap()
            .is_none()
    );
    drop(owner);
}

#[test]
fn an_unpinned_store_target_never_yields_a_background_plan() {
    let root = test_dir().unwrap();
    let mut owner = seed(&root);
    let (installed, _) = versions();
    let bytes = br#"{"schema":1,"channel":"stable","targets":{"android":{"version":"2.0.0","tag":"v2.0.0","method":"store"}}}"#;
    let mut current = owner.load().unwrap();
    current
        .admit(Cache::restore(bytes.to_vec(), None).unwrap())
        .unwrap();
    owner.save(&current).unwrap();
    assert!(
        claim_pending_notification(&root, Platform::Android, installed)
            .unwrap()
            .is_none()
    );
}

#[test]
fn an_aborted_commit_never_grants_notification_permission() {
    let root = test_dir().unwrap();
    let owner = seed(&root);
    let (installed, expected) = versions();
    let competing = Connection::open(root.join("updates.sqlite")).unwrap();
    competing.execute_batch("CREATE TRIGGER `abort_claim` BEFORE UPDATE ON `state` BEGIN SELECT RAISE(ABORT, 'controlled claim failure'); END;").unwrap();
    let before = owner.load().unwrap().encode().unwrap();
    assert!(claim_notification(&root, Platform::Android, installed, expected).is_err());
    assert!(claim_pending_notification(&root, Platform::Android, installed).is_err());
    assert_eq!(owner.load().unwrap().encode().unwrap(), before);
    drop(owner);
    assert!(crate::updates::checker::read_state(&root)
        .unwrap()
        .notice(Platform::Android, installed)
        .is_some());
}
