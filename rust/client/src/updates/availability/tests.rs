use super::*;
use crate::updates::{coordinator::Checkpoint, transport::Cache, CheckState};
use serde_json::json;

fn version(text: &str) -> Version {
    Version::parse(text).unwrap()
}

fn snapshot(targets: &serde_json::Value, success: bool) -> Snapshot {
    let body = json!({"schema":1,"channel":"stable","targets":targets});
    let cache = Cache::restore(serde_json::to_vec(&body).unwrap(), None).unwrap();
    let mut snapshot = Snapshot::default();
    snapshot.admit(cache).unwrap();
    if success {
        snapshot.set_checkpoint(Checkpoint {
            state: CheckState {
                last_attempt: Some(1),
                last_success: Some(1),
                consecutive_failures: 0,
            },
            ..Checkpoint::default()
        });
    }
    snapshot
}

fn release(version: &str, method: &str) -> serde_json::Value {
    json!({"version":version,"tag":format!("v{version}"),"method":method})
}

#[test]
fn no_cache_unconfirmed_cache_or_server_release_never_claims_current() {
    let installed = version("1.0.0");
    assert_eq!(
        project(&Snapshot::default(), Platform::Android, installed),
        Availability::NeverChecked
    );
    let unconfirmed = snapshot(&json!({"android":release("1.0.0","package")}), false);
    assert_eq!(
        project(&unconfirmed, Platform::Android, installed),
        Availability::NeverChecked
    );
    let server = snapshot(&json!({"dyappd":{"version":"99.0.0"}}), true);
    assert_eq!(
        project(&server, Platform::Android, installed),
        Availability::NotPublished
    );
}

#[test]
fn every_platform_uses_its_own_numeric_version_and_fixed_release_page() {
    for (platform, key) in [
        (Platform::LinuxX86_64, "linux-x86_64"),
        (Platform::WindowsX86_64, "windows-x86_64"),
        (Platform::MacosUniversal, "macos-universal"),
        (Platform::Android, "android"),
        (Platform::Ios, "ios"),
    ] {
        let mut targets = serde_json::Map::new();
        for other in Platform::ALL {
            targets.insert(other.key().into(), release("99.0.0", "package"));
        }
        targets.insert(key.into(), release("1.10.0", "package"));
        let state = snapshot(&serde_json::Value::Object(targets), true);
        let Availability::Available(offer) = project(&state, platform, version("1.9.0")) else {
            panic!("newer installed target")
        };
        assert_eq!(offer.version(), version("1.10.0"));
        assert_eq!(offer.method(), Method::Package);
        assert_eq!(
            offer.release_page(),
            "https://github.com/abbat/dyapp/releases/tag/v1.10.0"
        );
        assert_eq!(
            project(&state, platform, version("1.10.0")),
            Availability::Current
        );
        assert_eq!(
            project(&state, platform, version("2.0.0")),
            Availability::Current
        );
    }
}

#[test]
fn source_build_semantics_are_retained_and_unknown_store_destinations_stay_unavailable() {
    let source = snapshot(
        &json!({"android":release("2.0.0","build-from-source")}),
        true,
    );
    let Availability::Available(offer) = project(&source, Platform::Android, version("1.0.0"))
    else {
        panic!("source offer")
    };
    assert_eq!(offer.method(), Method::BuildFromSource);
    assert_eq!(
        offer.release_page(),
        "https://github.com/abbat/dyapp/releases/tag/v2.0.0"
    );
    let store = snapshot(&json!({"android":release("2.0.0","store")}), true);
    for installed in ["1.0.0", "2.0.0", "3.0.0"] {
        assert_eq!(
            project(&store, Platform::Android, version(installed)),
            Availability::DestinationUnavailable
        );
    }
}
