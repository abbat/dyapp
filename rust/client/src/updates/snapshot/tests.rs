use super::*;
use crate::updates::{CheckState, Interval};
use serde_json::json;

fn cache(targets: serde_json::Value) -> Cache {
    let bytes =
        serde_json::to_vec(&json!({"schema":1,"channel":"stable","targets":targets})).unwrap();
    Cache::restore(bytes, Some("\"saved\"")).unwrap()
}

fn release(version: &str) -> serde_json::Value {
    json!({"version":version,"tag":format!("v{version}"),"method":"package"})
}

#[test]
fn checkpoint_cache_and_high_water_round_trip_without_io_or_identity() {
    let mut snapshot = Snapshot::default();
    snapshot
        .admit(cache(json!({"android":release("1.2.3")})))
        .unwrap();
    let checkpoint = Checkpoint {
        interval: Interval::Week,
        state: CheckState {
            last_attempt: Some(u64::MAX),
            last_success: Some(900),
            consecutive_failures: 3,
        },
        in_flight: true,
    };
    snapshot.set_checkpoint(checkpoint);
    let restored = Snapshot::decode(&snapshot.encode().unwrap()).unwrap();
    assert_eq!(restored.checkpoint(), checkpoint);
    assert_eq!(
        restored.cache().unwrap().bytes(),
        snapshot.cache().unwrap().bytes()
    );
    assert_eq!(restored.cache().unwrap().etag(), Some("\"saved\""));
    assert_eq!(restored.highest(Platform::Android), Version::parse("1.2.3"));
    assert_eq!(
        Snapshot::decode(&restored.encode().unwrap())
            .unwrap()
            .checkpoint(),
        checkpoint
    );
}

#[test]
fn omitted_targets_keep_independent_high_water_after_restart() {
    let mut snapshot = Snapshot::default();
    snapshot
        .admit(cache(
            json!({"android":release("2.0.0"),"linux-x86_64":release("8.0.0")}),
        ))
        .unwrap();
    snapshot
        .admit(cache(json!({"android":release("2.1.0")})))
        .unwrap();
    let mut restored = Snapshot::decode(&snapshot.encode().unwrap()).unwrap();
    assert_eq!(
        restored.highest(Platform::LinuxX86_64),
        Version::parse("8.0.0")
    );
    let before = restored.encode().unwrap();
    let candidate = cache(json!({"android":release("3.0.0"),"linux-x86_64":release("7.9.9")}));
    assert!(matches!(restored.admit(candidate), Err(Error::Rollback)));
    assert_eq!(restored.encode().unwrap(), before);
    restored
        .admit(cache(
            json!({"android":release("2.1.0"),"linux-x86_64":release("8.0.0")}),
        ))
        .unwrap();
}

#[test]
fn malformed_or_inconsistent_records_never_become_fresh_state() {
    let fresh = Snapshot::default().encode().unwrap();
    assert_eq!(Snapshot::decode(&fresh).unwrap().encode().unwrap(), fresh);
    let value: serde_json::Value = serde_json::from_slice(&fresh).unwrap();
    for (field, invalid) in [
        ("schema", json!(2)),
        ("unexpected", json!(true)),
        ("interval_seconds", json!(900)),
        ("last_success", json!(1)),
        ("in_flight", json!(true)),
        ("etag", json!("\"orphan\"")),
        ("highest", json!([{"platform":"android","version":"1.0.0"}])),
    ] {
        let mut changed = value.clone();
        changed[field] = invalid;
        assert!(
            Snapshot::decode(&serde_json::to_vec(&changed).unwrap()).is_err(),
            "{field}"
        );
    }
    assert!(Snapshot::decode(&vec![b' '; MAX_BYTES + 1]).is_err());
    assert!(Snapshot::decode(b"{}").is_err());
    assert!(Snapshot::decode(b"{broken").is_err());
    let duplicate =
        String::from_utf8(fresh)
            .unwrap()
            .replacen("\"schema\":1", "\"schema\":1,\"schema\":1", 1);
    assert!(Snapshot::decode(duplicate.as_bytes()).is_err());
}

#[test]
fn saved_cache_cannot_bypass_high_water_or_etag_validation() {
    let mut snapshot = Snapshot::default();
    snapshot
        .admit(cache(json!({"android":release("2.0.0")})))
        .unwrap();
    let original: serde_json::Value = serde_json::from_slice(&snapshot.encode().unwrap()).unwrap();
    for high in [
        json!([]),
        json!([{"platform":"android","version":"1.0.0"}]),
        json!([{"platform":"android","version":"3.0.0"}]),
        json!([{"platform":"unknown","version":"2.0.0"}]),
        json!([{"platform":"android","version":"02.0.0"}]),
        json!([{"platform":"android","version":"2.0.0"},{"platform":"android","version":"2.0.0"}]),
    ] {
        let mut changed = original.clone();
        changed["highest"] = high;
        assert!(Snapshot::decode(&serde_json::to_vec(&changed).unwrap()).is_err());
    }
    for (field, invalid) in [
        ("etag", json!("header\r\ninjection")),
        ("feed", json!("{}")),
        ("feed", json!(null)),
    ] {
        let mut changed = original.clone();
        changed[field] = invalid;
        assert!(Snapshot::decode(&serde_json::to_vec(&changed).unwrap()).is_err());
    }
}
