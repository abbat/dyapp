use super::*;
use serde_json::{json, Value};

fn release(version: &str, tag: &str, method: &Value) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "schema": 1, "channel": "stable",
        "targets": {"android": {"version": version, "tag": tag, "method": method}}
    }))
    .unwrap()
}

#[test]
fn known_targets_are_typed_and_release_pages_are_constructed() {
    let body = br#"{"schema":1,"channel":"stable","targets":{
        "linux-x86_64":{"version":"0.0.1","tag":"v0.0.1","method":"package"},
        "windows-x86_64":{"version":"0.0.2","tag":"v0.0.2","method":"package"},
        "macos-universal":{"version":"0.0.3","tag":"v0.0.3","method":"build-from-source"},
        "android":{"version":"0.0.4","tag":"v0.0.4","method":"store"},
        "ios":{"version":"0.0.5","tag":"v0.0.5","method":"build-from-source"}
    }}"#;
    let feed = Feed::parse(body).unwrap();
    for (platform, version, method) in [
        (Platform::LinuxX86_64, "0.0.1", Method::Package),
        (Platform::WindowsX86_64, "0.0.2", Method::Package),
        (Platform::MacosUniversal, "0.0.3", Method::BuildFromSource),
        (Platform::Android, "0.0.4", Method::Store),
        (Platform::Ios, "0.0.5", Method::BuildFromSource),
    ] {
        let target = feed.target(platform).unwrap();
        assert_eq!(target.version(), Version::parse(version).unwrap());
        assert_eq!(target.method(), method);
        assert_eq!(target.tag(), format!("v{version}"));
        assert_eq!(
            target.release_page(),
            format!("https://github.com/abbat/dyapp/releases/tag/v{version}")
        );
    }
}

#[test]
fn unknown_targets_are_ignored_and_missing_targets_are_not_current() {
    let feed = Feed::parse(
        br#"{"schema":1,"channel":"stable","targets":{
        "future-platform":{"method":"future","url":"https://invalid.example/","requires":true},
        "another-platform":null
    }}"#,
    )
    .unwrap();
    assert!(feed.target(Platform::Android).is_none());
    assert!(feed.target(Platform::LinuxX86_64).is_none());
    let empty = Feed::parse(br#"{"schema":1,"channel":"stable","targets":{}}"#).unwrap();
    assert!(empty.target(Platform::Android).is_none());
}

#[test]
fn versions_and_tags_must_be_canonical_and_match() {
    for invalid in [
        "01.0.0",
        "v0.0.1",
        "0.0.1-beta",
        "1.2.3.4",
        "4294967296.0.0",
        "\u{0661}.0.0",
        "1.0.0/?evil",
    ] {
        assert_eq!(
            Feed::parse(&release(invalid, &format!("v{invalid}"), &json!("package"))).err(),
            Some(Error::Invalid)
        );
    }
    for tag in [
        "v0.0.2",
        "0.0.1",
        "v0.0.01",
        "v0.0.1/",
        "https://invalid.example/",
    ] {
        assert_eq!(
            Feed::parse(&release("0.0.1", tag, &json!("package"))).err(),
            Some(Error::Invalid)
        );
    }
    assert!(Feed::parse(&release(
        "4294967295.0.0",
        "v4294967295.0.0",
        &json!("package")
    ))
    .is_ok());
}

#[test]
fn methods_require_exact_supported_strings() {
    for method in [
        json!("future"),
        json!("PACKAGE"),
        json!({"package":null}),
        Value::Null,
        json!(1),
    ] {
        assert_eq!(
            Feed::parse(&release("0.0.1", "v0.0.1", &method)).err(),
            Some(Error::Invalid)
        );
    }
}

#[test]
fn duplicate_root_target_and_release_fields_fail_closed() {
    for body in [
        r#"{"schema":1,"schema":1,"channel":"stable","targets":{}}"#,
        r#"{"schema":1,"channel":"stable","targets":{},"targets":{}}"#,
        r#"{"schema":1,"channel":"stable","targets":{"android":{"version":"0.0.1","tag":"v0.0.1","method":"package"},"android":{"version":"0.0.2","tag":"v0.0.2","method":"package"}}}"#,
        r#"{"schema":1,"channel":"stable","targets":{"future":null,"future":null}}"#,
        r#"{"schema":1,"channel":"stable","targets":{"android":{"version":"0.0.1","version":"0.0.2","tag":"v0.0.2","method":"package"}}}"#,
    ] {
        assert_eq!(Feed::parse(body.as_bytes()).err(), Some(Error::Invalid));
    }
}

#[test]
fn unknown_root_fields_and_known_target_requirements_or_urls_are_rejected() {
    for body in [
        json!({"schema":1,"channel":"stable","targets":{},"url":"https://invalid.example/"}),
        json!({"schema":1,"channel":"stable","targets":{},"requires_protocol":2}),
        json!({"schema":1,"channel":"stable","targets":{"android":{"version":"0.0.1","tag":"v0.0.1","method":"package","url":"https://invalid.example/"}}}),
        json!({"schema":1,"channel":"stable","targets":{"android":{"version":"0.0.1","tag":"v0.0.1","method":"package","requires_protocol":2}}}),
    ] {
        assert_eq!(
            Feed::parse(&serde_json::to_vec(&body).unwrap()).err(),
            Some(Error::Invalid)
        );
    }
}

#[test]
fn malformed_shapes_schema_and_channel_never_produce_a_feed() {
    for body in [
        "",
        "[]",
        "null",
        "{}",
        r#"{"schema":2,"channel":"stable","targets":{}}"#,
        r#"{"schema":1.0,"channel":"stable","targets":{}}"#,
        r#"{"schema":1,"channel":"beta","targets":{}}"#,
        r#"{"schema":1,"channel":"stable","targets":[]}"#,
        r#"{"schema":1,"channel":"stable","targets":{"android":null}}"#,
        r#"{"schema":1,"channel":"stable","targets":{"android":{"version":"0.0.1","tag":"v0.0.1"}}}"#,
        r#"{"schema":1,"channel":"stable","targets":{}} false"#,
    ] {
        assert_eq!(Feed::parse(body.as_bytes()).err(), Some(Error::Invalid));
    }
    assert_eq!(Feed::parse(&[0xFF]).err(), Some(Error::Invalid));
}

#[test]
fn complete_response_size_is_bounded_before_parsing() {
    let mut body = br#"{"schema":1,"channel":"stable","targets":{}}"#.to_vec();
    body.resize(MAX_BYTES, b' ');
    assert!(Feed::parse(&body).is_ok());
    body.push(b' ');
    assert_eq!(Feed::parse(&body).err(), Some(Error::TooLarge));
}
