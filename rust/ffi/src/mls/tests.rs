use super::*;
use crate::WrapError;
use dyapp_profile::Profile;

/// XOR with a fixed byte: stands in for the platform keystore.
struct TestStore;

impl KeyStore for TestStore {
    fn wrap(&self, secret: Vec<u8>) -> Result<Vec<u8>, WrapError> {
        Ok(secret.iter().map(|byte| byte ^ 0x5a).collect())
    }

    fn unwrap(&self, wrapped: Vec<u8>) -> Result<Vec<u8>, WrapError> {
        self.wrap(wrapped)
    }
}

fn device() -> (Arc<IdentityKey>, Arc<MlsDevice>) {
    let identity = IdentityKey::generate();
    let device = MlsDevice::new(identity.clone(), IdentityKey::generate(), 1).unwrap();
    (identity, device)
}

// The owner's signed profile listing `device`.
fn profile(owner: &IdentityKey, device: &MlsDevice) -> Vec<u8> {
    Profile {
        version: 1,
        devices: vec![device.device.certificate().clone()],
        ..Profile::default()
    }
    .sign(&owner.identity)
    .encode_to_vec()
}

#[test]
fn two_devices_exchange_messages_across_a_restart() {
    let (alice, alice_device) = device();
    let (bob, bob_device) = device();
    let profiles = vec![profile(&alice, &alice_device), profile(&bob, &bob_device)];
    let package = bob_device.key_package(false).unwrap();
    let created = alice_device
        .create_group(vec![package], profiles.clone())
        .unwrap();
    let bob_group = bob_device.join(created.welcome, profiles.clone()).unwrap();
    assert_eq!(bob_group.id().unwrap(), created.group.id().unwrap());

    let store: Arc<dyn KeyStore> = Arc::new(TestStore);
    let wrapped = bob_device.wrap_state_key(store.clone()).unwrap();
    let sealed = bob_device.seal().unwrap();
    let bob_device = MlsDevice::open(store.clone(), wrapped.clone(), sealed.clone()).unwrap();
    assert_eq!(bob_device.certificate(), profile_device(&profiles[1]));
    let bob_group = bob_device
        .load_group(created.group.id().unwrap())
        .unwrap()
        .unwrap();
    assert!(bob_device.load_group(b"none".to_vec()).unwrap().is_none());

    let message = created
        .group
        .encrypt(alice_device.clone(), b"hi".to_vec())
        .unwrap();
    assert!(!message.windows(2).any(|w| w == b"hi"));
    assert_eq!(
        bob_group
            .process(bob_device.clone(), message, profiles.clone())
            .unwrap(),
        MlsReceived::Message {
            sender: alice.peer_id(),
            plaintext: b"hi".to_vec()
        }
    );
    let mut members = bob_group.members().unwrap();
    members.sort();
    let mut expected = vec![alice.peer_id(), bob.peer_id()];
    expected.sort();
    assert_eq!(members, expected);
    let secret = bob_group
        .export_secret(bob_device.clone(), "call".to_string(), vec![], 16)
        .unwrap();
    assert_eq!(secret.len(), 16);
    assert_eq!(
        secret,
        created
            .group
            .export_secret(alice_device.clone(), "call".to_string(), vec![], 16)
            .unwrap()
    );
    assert!(bob_group
        .remove_revoked(bob_device, profiles.clone())
        .unwrap()
        .is_none());

    // A bad profile, a wrong-sized key and a different state key all fail.
    assert!(created
        .group
        .remove_revoked(alice_device, vec![vec![1, 2, 3]])
        .is_err());
    assert!(MlsDevice::open(store.clone(), vec![0; 31], sealed.clone()).is_err());
    let other = device().1.wrap_state_key(store.clone()).unwrap();
    assert!(MlsDevice::open(store, other, sealed).is_err());
}

// The certificate a one-device profile lists.
fn profile_device(record: &[u8]) -> Vec<u8> {
    let record = SignedRecord::decode(record).unwrap();
    Profile::decode(record.payload.as_slice()).unwrap().devices[0].encode_to_vec()
}
