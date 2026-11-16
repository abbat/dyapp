use super::*;
use crate::slot_registry_tests::{root, Credentials};
use crate::{MobileLinkReceiver, MobileSlotLayout};

#[test]
fn code_status_preserves_malformed_expired_and_valid_input_without_transferring() {
    assert_eq!(
        device_link_code_status("bad".into()),
        MobileLinkCodeStatus::Malformed
    );
    let expired = dyapp_client::link::Code {
        public: [1; 32],
        mailbox: [2; 32],
        expires_at: 1,
    };
    assert_eq!(
        device_link_code_status(expired.text()),
        MobileLinkCodeStatus::Expired
    );
    let invalid = dyapp_client::link::Code {
        expires_at: 0,
        ..expired
    };
    assert_eq!(
        device_link_code_status(invalid.text()),
        MobileLinkCodeStatus::Malformed
    );
    let valid = dyapp_client::link::Code {
        expires_at: u64::MAX,
        ..expired
    };
    assert_eq!(
        device_link_code_status(format!(" \n{}\t", valid.text())),
        MobileLinkCodeStatus::Valid
    );
}

#[test]
fn prepared_native_approval_is_idle_and_profile_aba_refuses_before_private_snapshot() {
    let root = root();
    let registry = MobileIdentityRegistry::new(
        root.to_str().unwrap().into(),
        MobileSlotLayout::Android,
        Arc::new(Credentials::default()),
    );
    let source = registry.create().unwrap();
    let target = MobileLinkReceiver::start(
        registry.clone(),
        source.slot.id.clone(),
        source.key.peer_id(),
        false,
    )
    .unwrap();
    let approval = MobileLinkApproval::prepare(
        registry.clone(),
        source.slot.id.clone(),
        source.key.peer_id(),
        target.code(),
    )
    .unwrap();
    assert_eq!(approval.code(), target.code());
    assert_eq!(approval.expires_at(), target.expires_at());
    let path = std::path::Path::new(&source.slot.directory).join("client.db");
    assert!(!path.exists());
    registry.create().unwrap();
    registry.select(source.slot.id).unwrap();
    assert!(
        matches!(approval.confirm_and_send(), Err(CoreError::Failed(error)) if error == "Profile view changed")
    );
    assert!(!path.exists());
    assert_eq!(registry.list().unwrap().len(), 2);
}
