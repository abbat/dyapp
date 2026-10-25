//! Fixed v1 contracts; generated round-trips alone cannot detect format changes.
use dyapp_bootstrap::NodeConfig;
use dyapp_identity::{Domain, SignedRecord};
use dyapp_messaging::{Message, MessageStatus};
use dyapp_p2p_net::proto;
use prost::Message as _;

#[derive(Clone, PartialEq, prost::Message)]
struct LegacyStatus {
    #[prost(enumeration = "proto::Status", tag = "1")]
    status: i32,
}

#[derive(Clone, PartialEq, prost::Message)]
struct LegacyInfo {
    #[prost(uint64, tag = "2")]
    max_profile_bytes: u64,
}

#[test]
fn server_clock_hints_keep_fixed_tags_and_legacy_decoding() {
    let legacy = [0x08, 0x09]; // INVALID before clock hints.
    let profile = proto::ProfileResponse::decode(legacy.as_slice()).unwrap();
    let media = proto::MediaResponse::decode(legacy.as_slice()).unwrap();
    let search = proto::SearchResponse::decode(legacy.as_slice()).unwrap();
    assert_eq!(
        (profile.server_time, media.server_time, search.server_time),
        (0, 0, 0)
    );
    for encoded in [
        profile.encode_to_vec(),
        media.encode_to_vec(),
        search.encode_to_vec(),
    ] {
        assert_eq!(encoded, legacy);
    }
    let legacy_info = [0x10, 0x07]; // max_profile_bytes = 7.
    let mut info = proto::NodeInfo::decode(legacy_info.as_slice()).unwrap();
    assert_eq!(info.server_time, 0);
    assert_eq!(info.encode_to_vec(), legacy_info);
    info.server_time = 123;
    assert_eq!(info.encode_to_vec(), [0x10, 0x07, 0x50, 0x7b]);
    assert_eq!(
        LegacyInfo::decode(info.encode_to_vec().as_slice())
            .unwrap()
            .max_profile_bytes,
        7
    );

    let profile = proto::ProfileResponse {
        max_bytes: 100,
        server_time: 123,
        ..profile
    };
    let media = proto::MediaResponse {
        max_bytes: 100,
        server_time: 123,
        ..media
    };
    let search = proto::SearchResponse {
        max_bytes: 100,
        required_pow_bits: 11,
        server_time: 123,
        ..search
    };
    for (encoded, expected) in [
        (
            profile.encode_to_vec(),
            vec![0x08, 0x09, 0x20, 0x64, 0x28, 0x7b],
        ),
        (
            media.encode_to_vec(),
            vec![0x08, 0x09, 0x28, 0x64, 0x30, 0x7b],
        ),
        (
            search.encode_to_vec(),
            vec![0x08, 0x09, 0x40, 0x64, 0x48, 0x0b, 0x50, 0x7b],
        ),
    ] {
        assert_eq!(encoded, expected);
        assert_eq!(
            LegacyStatus::decode(encoded.as_slice()).unwrap().status,
            proto::Status::Invalid as i32
        );
    }
}

#[test]
fn retired_age_tag_is_ignored() {
    let hex = include_str!("fixtures/signed-profile-v1.hex").trim();
    assert_eq!(hex.len() % 2, 0);
    let bytes: Vec<u8> = hex
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect();
    let record = SignedRecord::decode(bytes.as_slice()).unwrap();
    let verified = dyapp_profile::verify(&record).unwrap();
    assert_eq!(verified.profile.version, 1);
    assert_eq!(verified.profile.birth_year, None);
    assert_eq!(verified.profile.birth_month, None);
    assert_eq!(verified.profile.birth_day, None);
    assert!(record.verify(Domain::MediaKeep).is_err());
    // Re-encoding drops retired tag 3; its former value cannot become a birth date.
    assert_ne!(verified.profile.encode_to_vec(), record.payload);
}

#[test]
fn message_v1_json_remains_compatible() {
    let fixture = include_str!("fixtures/message-v1.json").trim();
    let decoded: Message = serde_json::from_str(fixture).unwrap();
    let expected = Message {
        id: "fixture-v1".into(),
        sender_id: "sender".into(),
        recipient_id: "recipient".into(),
        text: "hello\nπ".into(),
        timestamp: u64::MAX.into(),
        created_at: 1700000000000,
        status: MessageStatus::Delivered,
        encrypted_payload: Some(vec![0, 255]),
        signature: Some(vec![1, 2, 3]),
    };
    assert_eq!(decoded.id, expected.id);
    assert_eq!(decoded.sender_id, expected.sender_id);
    assert_eq!(decoded.recipient_id, expected.recipient_id);
    assert_eq!(decoded.text, expected.text);
    assert_eq!(decoded.timestamp, expected.timestamp);
    assert_eq!(decoded.created_at, expected.created_at);
    assert_eq!(decoded.status, expected.status);
    assert_eq!(decoded.encrypted_payload, expected.encrypted_payload);
    assert_eq!(decoded.signature, expected.signature);
    assert_eq!(serde_json::to_string(&expected).unwrap(), fixture);
}

#[test]
fn named_keygen_binds_storage_and_a_public_start_is_refused() {
    let dir = format!("/tmp/ai/test-network-named-{}", uuid::Uuid::new_v4());
    let dyappd = |name: &str, args: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_dyappd"))
            .env_clear()
            .envs(std::env::var_os("LLVM_PROFILE_FILE").map(|path| ("LLVM_PROFILE_FILE", path)))
            .args(["--storage.dir", &dir, "--network.name", name])
            .args(["--network.id-pow-bits", "0"])
            .args(args)
            .output()
            .unwrap()
    };
    let output = dyappd("city", &["keygen"]);
    assert!(output.status.success(), "{output:?}");
    let marker = std::path::Path::new(&dir).join("network");
    assert_eq!(std::fs::read_to_string(marker).unwrap(), "city");
    let output = dyappd("", &[]);
    assert!(!output.status.success());
    assert!(String::from_utf8(output.stderr)
        .unwrap()
        .contains("storage belongs to network \"city\""));
}

#[test]
fn documented_config_and_deny_diagnostics_remain_stable() {
    let config_error = NodeConfig::load(
        None,
        Vec::<(String, String)>::new(),
        &[("missing.option".into(), "1".into())],
    )
    .unwrap_err()
    .to_string();
    let dir = format!("/tmp/ai/test-deny-diagnostic-{}", uuid::Uuid::new_v4());
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_dyappd"))
        .env_clear()
        .envs(std::env::var_os("LLVM_PROFILE_FILE").map(|path| ("LLVM_PROFILE_FILE", path)))
        .args(["deny", "add", "not-an-entry", "--storage.dir", &dir])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let deny_error = String::from_utf8(output.stderr).unwrap();
    let actual = format!("config: {config_error}\ndeny: {}\n", deny_error.trim());
    assert_eq!(actual, include_str!("fixtures/diagnostics-v1.txt"));
}
