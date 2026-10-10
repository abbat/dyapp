//! Fixed v1 contracts; generated round-trips alone cannot detect format changes.
use dyapp_bootstrap::NodeConfig;
use dyapp_identity::{Domain, Identity, SignedRecord};
use dyapp_messaging::{Message, MessageStatus};
use dyapp_profile::Profile;
use prost::Message as _;

#[test]
fn signed_profile_v1_bytes_remain_compatible() {
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
    assert_eq!(verified.profile.age, 26);
    assert!(record.verify(Domain::MediaKeep).is_err());
    let current = Profile {
        version: 1,
        age: 26,
        ..Default::default()
    }
    .sign(&Identity::from_secret(&[0; 32]));
    assert_eq!(current, record);
    assert_eq!(current.encode_to_vec(), bytes);
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
        lamport_timestamp: u64::MAX,
        created_at: 1700000000000,
        status: MessageStatus::Delivered,
        encrypted_payload: Some(vec![0, 255]),
        signature: Some(vec![1, 2, 3]),
    };
    assert_eq!(decoded.id, expected.id);
    assert_eq!(decoded.sender_id, expected.sender_id);
    assert_eq!(decoded.recipient_id, expected.recipient_id);
    assert_eq!(decoded.text, expected.text);
    assert_eq!(decoded.lamport_timestamp, expected.lamport_timestamp);
    assert_eq!(decoded.created_at, expected.created_at);
    assert_eq!(decoded.status, expected.status);
    assert_eq!(decoded.encrypted_payload, expected.encrypted_payload);
    assert_eq!(decoded.signature, expected.signature);
    assert_eq!(serde_json::to_string(&expected).unwrap(), fixture);
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
        .args(["deny", "add", "not-an-entry", "--storage.dir", &dir])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let deny_error = String::from_utf8(output.stderr).unwrap();
    let actual = format!("config: {config_error}\ndeny: {}\n", deny_error.trim());
    assert_eq!(actual, include_str!("fixtures/diagnostics-v1.txt"));
}
