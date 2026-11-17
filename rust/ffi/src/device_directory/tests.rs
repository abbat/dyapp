mod wire;

use super::*;
use crate::slot_registry_tests::{root, Credentials, Wrapping};
use crate::{MobileSlotLayout, MobileSlotSelection};
use dyapp_identity::Identity;
use dyapp_profile::Profile;
use std::path::PathBuf;

pub(super) struct Fixture {
    pub(super) registry: Arc<MobileIdentityRegistry>,
    pub(super) slot: MobileSlotSelection,
    pub(super) worker: Arc<MobileDeviceDirectory>,
    pub(super) keys: Arc<Credentials>,
    cached: Profile,
}

impl Fixture {
    pub(super) fn new() -> Self {
        let keys = Arc::new(Credentials::default());
        let registry = MobileIdentityRegistry::new(
            root().to_str().unwrap().into(),
            MobileSlotLayout::Android,
            keys.clone(),
        );
        let slot = registry.create().unwrap();
        drop(
            registry
                .open_chats(slot.slot.id.clone(), slot.key.peer_id(), Arc::new(Wrapping))
                .unwrap(),
        );
        let worker = MobileDeviceDirectory::new(
            registry.clone(),
            slot.slot.id.clone(),
            slot.key.peer_id(),
            Arc::new(Wrapping),
        )
        .unwrap();
        let admitted = worker.open().unwrap();
        let cached = Profile {
            version: 5,
            name: "pending editor fields".into(),
            devices: vec![admitted.session.certificate().unwrap()],
            device_owners: vec![dyapp_client::owner_binding(
                admitted.operation.device(),
                &admitted.operation.identity().public_key(),
            )],
            ..Default::default()
        };
        admitted
            .session
            .store()
            .save_profile_draft(&cached)
            .unwrap();
        drop(admitted);
        Self {
            registry,
            slot,
            worker,
            keys,
            cached,
        }
    }

    fn store(&self) -> Store {
        Store::open_existing(
            &PathBuf::from(&self.slot.slot.directory).join("client.db"),
            &self.slot.key.identity,
        )
        .unwrap()
    }
}

#[test]
fn offline_reads_keep_cached_rows_and_an_idle_handle_does_not_block_profile_changes() {
    let f = Fixture::new();
    let before = f.store().profile_draft().unwrap();
    let offline = f.worker.devices(false).unwrap();
    assert_eq!(offline.len(), 1);
    assert!(offline[0].current);
    assert_eq!(offline[0].key.len(), 32);
    assert_eq!(f.worker.devices(true).unwrap(), offline);
    assert_eq!(f.store().profile_draft().unwrap(), before);
    // A handle is idle between calls; a registry mutation must not wait for its disposal.
    let registry = f.registry.clone();
    let (completed, result) = std::sync::mpsc::channel();
    let mutation = std::thread::spawn(move || completed.send(registry.create()).unwrap());
    let other = result
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap()
        .unwrap();
    mutation.join().unwrap();
    assert_ne!(other.slot.id, f.slot.slot.id);
    assert_eq!(f.worker.devices(false).unwrap(), offline);
    assert_eq!(f.store().profile_draft().unwrap(), Some(f.cached));
}

#[test]
fn native_directory_preserves_core_current_first_order_and_saved_draft() {
    use dyapp_identity::Domain;
    use dyapp_profile::DeviceCertificate;
    use prost::Message as _;

    let f = Fixture::new();
    let other = Identity::try_generate().unwrap();
    let mut cached = f.cached.clone();
    cached.devices.insert(
        0,
        f.slot.key.identity.sign(
            Domain::DeviceCertificate,
            DeviceCertificate {
                device_key: other.public_key().to_vec(),
                mls_signature_key: Identity::try_generate().unwrap().public_key().to_vec(),
                generation: 1,
            }
            .encode_to_vec(),
        ),
    );
    f.store().save_profile_draft(&cached).unwrap();
    let admitted = f.worker.open().unwrap();
    let current = admitted.operation.device().public_key().to_vec();
    drop(admitted);
    assert_eq!(
        f.worker.devices(false).unwrap(),
        vec![
            MobileOwnDevice {
                key: current,
                current: true
            },
            MobileOwnDevice {
                key: other.public_key().to_vec(),
                current: false
            },
        ]
    );
    assert_eq!(f.store().profile_draft().unwrap(), Some(cached));
}

#[test]
fn consistent_replacement_device_cannot_reuse_the_original_directory_handle() {
    let f = Fixture::new();
    let admitted = f.worker.open().unwrap();
    let device = Identity::try_generate().unwrap();
    let sealed =
        dyapp_messaging::mls::Device::new(admitted.operation.identity(), device.public_key(), 1)
            .unwrap()
            .seal(admitted.operation.state_key())
            .unwrap();
    let wrapped: Vec<_> = device.secret().iter().map(|byte| byte ^ 0x5a).collect();
    std::fs::write(
        PathBuf::from(&f.slot.slot.directory).join("device.key"),
        wrapped,
    )
    .unwrap();
    f.store().stage_outbound(&sealed, &[]).unwrap();
    drop(admitted);
    assert_eq!(
        f.worker.devices(false).unwrap_err().to_string(),
        "Device context changed"
    );
    assert_eq!(
        f.worker.revoke(vec![1; 32]).unwrap_err().to_string(),
        "Device context changed"
    );
    assert_eq!(
        f.worker.revoked_device().err().unwrap().to_string(),
        "Device context changed"
    );
    assert_eq!(f.store().profile_draft().unwrap(), Some(f.cached));
}

#[test]
fn malformed_targets_and_missing_network_leave_the_original_directory_unchanged() {
    let f = Fixture::new();
    for target in [vec![], vec![1; 31], vec![1; 33]] {
        assert_eq!(
            f.worker.revoke(target).unwrap_err().to_string(),
            "invalidDevice"
        );
    }
    assert_eq!(
        f.worker
            .revoke(f.worker.devices(false).unwrap()[0].key.clone())
            .unwrap_err()
            .to_string(),
        "notConnected"
    );
    assert_eq!(f.store().profile_draft().unwrap(), Some(f.cached));
}

#[test]
fn constructor_refuses_an_uninitialized_slot_without_bootstrapping_chat_keys() {
    let registry = MobileIdentityRegistry::new(
        root().to_str().unwrap().into(),
        MobileSlotLayout::Android,
        Arc::new(Credentials::default()),
    );
    let slot = registry.create().unwrap();
    assert!(MobileDeviceDirectory::new(
        registry,
        slot.slot.id,
        slot.key.peer_id(),
        Arc::new(Wrapping),
    )
    .is_err());
    let directory = PathBuf::from(slot.slot.directory);
    assert!(!directory.join("client.db").exists());
    assert!(!directory.join("device.key").exists());
    assert!(!directory.join("chat-state.key").exists());
}
