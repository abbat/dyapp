use super::{
    MobileClientUpdates, MobileUpdateAvailability, MobileUpdateDelivery, MobileUpdateInterval,
    MobileUpdatePlatform,
};
use dyapp_client::updates::{self as core, coordinator::Checkpoint, transport::Cache, CheckState};
use dyapp_test_support::test_dir;
use std::fs::OpenOptions;

fn client(root: &std::path::Path) -> MobileClientUpdates {
    MobileClientUpdates {
        root: root.to_owned(),
        platform: core::feed::Platform::Android,
        installed: core::Version::parse("1.0.0").unwrap(),
    }
}

fn published(root: &std::path::Path) -> core::storage::LockedState {
    let mut state = core::storage::LockedState::try_open(root).unwrap().unwrap();
    let mut snapshot = state.load().unwrap();
    let body = br#"{"schema":1,"channel":"stable","targets":{"android":{"version":"2.0.0","tag":"v2.0.0","method":"package"}}}"#;
    snapshot
        .admit(Cache::restore(body.to_vec(), None).unwrap())
        .unwrap();
    snapshot.set_checkpoint(Checkpoint {
        state: CheckState {
            last_attempt: Some(200),
            last_success: Some(100),
            consecutive_failures: 0,
        },
        in_flight: true,
        ..Checkpoint::default()
    });
    state.save(&snapshot).unwrap();
    state
}

#[test]
fn native_metadata_is_installation_scoped_and_all_intervals_round_trip() {
    let dir = test_dir().unwrap();
    let profile = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(dir.join("profiles.lock"))
        .unwrap();
    profile.lock().unwrap();
    let updates = MobileClientUpdates::new(
        dir.path().to_string_lossy().into_owned(),
        MobileUpdatePlatform::Android,
    )
    .unwrap();
    let fresh = updates.read_state().unwrap();
    assert_eq!(fresh.installed_version, env!("CARGO_PKG_VERSION"));
    assert_eq!(fresh.availability, MobileUpdateAvailability::NeverChecked);
    assert_eq!(fresh.offer, None);
    for interval in [
        MobileUpdateInterval::SixHours,
        MobileUpdateInterval::TwelveHours,
        MobileUpdateInterval::Day,
        MobileUpdateInterval::Week,
    ] {
        assert_eq!(updates.set_interval(interval).unwrap().interval, interval);
        assert_eq!(client(&dir).read_state().unwrap().interval, interval);
    }
    assert!(MobileClientUpdates::new("relative".into(), MobileUpdatePlatform::Android).is_err());
}

#[test]
fn active_request_metadata_retains_offer_and_accepts_immediate_interval_changes() {
    let dir = test_dir().unwrap();
    let lease = published(&dir);
    let updates = client(&dir);
    let view = updates.read_state().unwrap();
    assert!(view.in_flight);
    assert_eq!(view.last_attempt, Some(200));
    assert_eq!(view.last_success, Some(100));
    let offer = view.offer.unwrap();
    assert_eq!(offer.version, "2.0.0");
    assert_eq!(offer.delivery, MobileUpdateDelivery::Package);
    assert_eq!(
        offer.release_page,
        "https://github.com/abbat/dyapp/releases/tag/v2.0.0"
    );
    assert_eq!(
        updates
            .set_interval(MobileUpdateInterval::Week)
            .unwrap()
            .interval,
        MobileUpdateInterval::Week
    );
    assert_eq!(
        lease.load().unwrap().checkpoint().interval,
        core::Interval::Week
    );
    assert!(core::storage::LockedState::try_open(&dir)
        .unwrap()
        .is_none());
    let other = MobileClientUpdates {
        platform: core::feed::Platform::LinuxX86_64,
        ..client(&dir)
    };
    assert_eq!(
        other.read_state().unwrap().availability,
        MobileUpdateAvailability::NotPublished
    );
}

#[test]
fn malformed_records_remain_errors_instead_of_empty_native_state() {
    let dir = test_dir().unwrap();
    let updates = client(&dir);
    updates.read_state().unwrap();
    let db = rusqlite::Connection::open(dir.join("updates.sqlite")).unwrap();
    db.execute(
        "UPDATE `state` SET `payload` = ?1 WHERE `id` = 1",
        [b"invalid".as_slice()],
    )
    .unwrap();
    assert!(updates.read_state().is_err());
    assert!(updates.set_interval(MobileUpdateInterval::Day).is_err());
    let retained: Vec<u8> = db
        .query_row("SELECT `payload` FROM `state`", [], |row| row.get(0))
        .unwrap();
    assert_eq!(retained, b"invalid".as_slice());
}
