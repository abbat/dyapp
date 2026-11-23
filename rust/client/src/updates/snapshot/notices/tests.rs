use super::*;
use crate::updates::{coordinator::Checkpoint, feed::Method, transport::Cache, CheckState};
use serde_json::json;

fn version(text: &str) -> Version {
    Version::parse(text).unwrap()
}

fn publish(snapshot: &mut Snapshot, platform: Platform, release: &str, method: &str) {
    let body = json!({"schema":1,"channel":"stable","targets":{
        (platform.key()): {"version":release,"tag":format!("v{release}"),"method":method}
    }});
    snapshot
        .admit(Cache::restore(serde_json::to_vec(&body).unwrap(), None).unwrap())
        .unwrap();
    snapshot.set_checkpoint(Checkpoint {
        state: CheckState {
            last_attempt: Some(1),
            last_success: Some(1),
            consecutive_failures: 0,
        },
        ..Checkpoint::default()
    });
}

#[test]
fn later_survives_restart_without_hiding_settings_or_the_next_version() {
    let platform = Platform::Android;
    let installed = version("1.0.0");
    let mut snapshot = Snapshot::default();
    publish(&mut snapshot, platform, "2.0.0", "package");
    assert!(snapshot.dismiss_notice(platform, installed, version("2.0.0")));
    let mut restored = Snapshot::decode(&snapshot.encode().unwrap()).unwrap();
    assert!(restored.notice(platform, installed).is_none());
    assert!(matches!(
        availability::project(&restored, platform, installed),
        Availability::Available(_)
    ));
    assert!(!restored.claim_notification(platform, installed, version("2.0.0")));
    publish(&mut restored, platform, "2.1.0", "package");
    assert!(!restored.dismiss_notice(platform, installed, version("2.0.0")));
    assert!(!restored.claim_notification(platform, installed, version("2.0.0")));
    assert_eq!(
        restored.notice(platform, installed).unwrap().version(),
        version("2.1.0")
    );
    assert!(restored.claim_notification(platform, installed, version("2.1.0")));
}

#[test]
fn os_claim_is_once_per_target_version_and_does_not_consume_the_in_app_notice() {
    let mut snapshot = Snapshot::default();
    for platform in Platform::ALL {
        publish(&mut snapshot, platform, "2.0.0", "package");
        assert!(snapshot.claim_notification(platform, version("1.0.0"), version("2.0.0")));
        assert!(snapshot.notice(platform, version("1.0.0")).is_some());
    }
    let mut restored = Snapshot::decode(&snapshot.encode().unwrap()).unwrap();
    for platform in Platform::ALL {
        publish(&mut restored, platform, "2.0.0", "package");
        assert!(!restored.claim_notification(platform, version("1.0.0"), version("2.0.0")));
        assert!(restored.notice(platform, version("2.0.0")).is_none());
        assert!(!restored.dismiss_notice(platform, version("2.0.0"), version("2.0.0")));
    }
}

#[test]
fn unconfirmed_absent_or_unsupported_targets_cannot_be_claimed() {
    let mut snapshot = Snapshot::default();
    let installed = version("1.0.0");
    let newer = version("2.0.0");
    assert!(!snapshot.claim_notification(Platform::Android, installed, newer));
    publish(&mut snapshot, Platform::LinuxX86_64, "2.0.0", "package");
    assert!(!snapshot.dismiss_notice(Platform::Android, installed, newer));
    publish(&mut snapshot, Platform::Android, "2.0.0", "package");
    let checkpoint = snapshot.checkpoint();
    snapshot.set_checkpoint(Checkpoint::default());
    assert!(!snapshot.claim_notification(Platform::Android, installed, newer));
    snapshot.set_checkpoint(checkpoint);
    publish(&mut snapshot, Platform::Android, "2.0.0", "store");
    assert!(!snapshot.claim_notification(Platform::Android, installed, newer));
    publish(
        &mut snapshot,
        Platform::Android,
        "2.0.0",
        "build-from-source",
    );
    assert_eq!(
        snapshot
            .notice(Platform::Android, installed)
            .unwrap()
            .method(),
        Method::BuildFromSource
    );
}

#[test]
fn missing_legacy_marks_default_but_malformed_or_future_marks_are_rejected() {
    let mut snapshot = Snapshot::default();
    publish(&mut snapshot, Platform::Android, "2.0.0", "package");
    let mut saved: serde_json::Value = serde_json::from_slice(&snapshot.encode().unwrap()).unwrap();
    saved.as_object_mut().unwrap().remove("dismissed");
    saved.as_object_mut().unwrap().remove("notified");
    let legacy = Snapshot::decode(&serde_json::to_vec(&saved).unwrap()).unwrap();
    assert!(legacy.notice(Platform::Android, version("1.0.0")).is_some());
    for field in ["dismissed", "notified"] {
        for invalid in [
            json!([{"platform":"android","version":"3.0.0"}]),
            json!([{"platform":"unknown","version":"2.0.0"}]),
            json!([{"platform":"android","version":"02.0.0"}]),
            json!([{"platform":"android","version":"2.0.0"},{"platform":"android","version":"2.0.0"}]),
            json!(null),
        ] {
            let mut malformed = saved.clone();
            malformed[field] = invalid;
            assert!(
                Snapshot::decode(&serde_json::to_vec(&malformed).unwrap()).is_err(),
                "{field}"
            );
        }
    }
}
