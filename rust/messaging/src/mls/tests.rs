use super::*;

fn device() -> (Identity, Device) {
    let identity = Identity::generate();
    let device = Device::new(&identity, [7; 32], 1).unwrap();
    (identity, device)
}

// A device whose credential carries `certificate` instead of its own.
fn with_certificate(device: &mut Device, certificate: &SignedRecord) {
    device.credential.credential = BasicCredential::new(certificate.encode_to_vec()).into();
}

fn pair() -> (Identity, Device, Group, Identity, Device, Group) {
    let (alice, alice_device) = device();
    let (bob, bob_device) = device();
    let (alice_group, welcome) = alice_device
        .create_group(&[bob_device.key_package(false).unwrap()])
        .unwrap();
    let bob_group = bob_device.join(&welcome).unwrap();
    (alice, alice_device, alice_group, bob, bob_device, bob_group)
}

#[test]
fn members_exchange_messages_both_ways() {
    let (alice, alice_device, mut alice_group, bob, bob_device, mut bob_group) = pair();
    assert_eq!(alice_group.id(), bob_group.id());
    assert_eq!(
        bob_group.members().unwrap(),
        [alice.peer_id(), bob.peer_id()]
    );

    let sealed = alice_group.encrypt(&alice_device, b"hello").unwrap();
    assert!(!sealed.windows(5).any(|w| w == b"hello"));
    assert_eq!(
        bob_group.process(&bob_device, &sealed).unwrap(),
        Received::Message {
            sender: alice.peer_id(),
            plaintext: b"hello".to_vec()
        }
    );
    let reply = bob_group.encrypt(&bob_device, b"hi").unwrap();
    assert_eq!(
        alice_group.process(&alice_device, &reply).unwrap(),
        Received::Message {
            sender: bob.peer_id(),
            plaintext: b"hi".to_vec()
        }
    );
}

#[test]
fn tampered_or_replayed_message_is_rejected() {
    let (_, alice_device, mut alice_group, _, bob_device, mut bob_group) = pair();
    let mut tampered = alice_group.encrypt(&alice_device, b"hello").unwrap();
    *tampered.last_mut().unwrap() ^= 1;
    assert!(bob_group.process(&bob_device, &tampered).is_err());
    let sealed = alice_group.encrypt(&alice_device, b"hello").unwrap();
    assert!(bob_group.process(&bob_device, &sealed).is_ok());
    assert!(bob_group.process(&bob_device, &sealed).is_err());
}

#[test]
fn another_group_cannot_read_a_message() {
    let (_, alice_device, mut alice_group, _, _, _) = pair();
    let (_, carol_device, mut carol_group, _, _, _) = pair();
    let sealed = alice_group.encrypt(&alice_device, b"hello").unwrap();
    assert!(carol_group.process(&carol_device, &sealed).is_err());
}

#[test]
fn welcome_works_once_for_a_single_use_key_package() {
    let (_, alice_device) = device();
    let (_, bob_device) = device();
    let (_, welcome) = alice_device
        .create_group(&[bob_device.key_package(false).unwrap()])
        .unwrap();
    bob_device.join(&welcome).unwrap();
    assert!(bob_device.join(&welcome).is_err());

    let last_resort = bob_device.key_package(true).unwrap();
    for _ in 0..2 {
        let (_, welcome) = alice_device
            .create_group(std::slice::from_ref(&last_resort))
            .unwrap();
        bob_device.join(&welcome).unwrap();
    }
}

#[test]
fn key_package_without_a_valid_certificate_is_rejected() {
    let (_, alice_device) = device();
    let (bob, mut bob_device) = device();
    let certificate = |domain, mls_signature_key: Vec<u8>| {
        bob.sign(
            domain,
            DeviceCertificate {
                device_key: vec![7; 32],
                mls_signature_key,
                generation: 1,
            }
            .encode_to_vec(),
        )
    };
    let own_key = bob_device.signer.public().to_vec();
    for forged in [
        certificate(Domain::KeyPackage, own_key),
        certificate(Domain::DeviceCertificate, vec![9; 32]),
    ] {
        with_certificate(&mut bob_device, &forged);
        let package = bob_device.key_package(false).unwrap();
        assert!(matches!(
            alice_device.create_group(&[package]),
            Err(Error::Credential)
        ));
    }
}

#[test]
fn key_package_with_another_suite_or_long_lifetime_is_rejected() {
    let (_, alice_device) = device();
    let (_, bob_device) = device();
    let p256 = Ciphersuite::MLS_128_DHKEMP256_AES128GCM_SHA256_P256;
    let signer = SignatureKeyPair::new(p256.signature_algorithm()).unwrap();
    let package = KeyPackage::builder()
        .build(
            p256,
            &bob_device.provider,
            &signer,
            CredentialWithKey {
                credential: bob_device.credential.credential.clone(),
                signature_key: signer.public().into(),
            },
        )
        .unwrap()
        .key_package()
        .tls_serialize_detached()
        .unwrap();
    assert!(matches!(
        alice_device.create_group(&[package]),
        Err(Error::Ciphersuite)
    ));

    let package = bob_device
        .key_package_for(Lifetime::new(2 * KEY_PACKAGE_LIFETIME_SECONDS), false)
        .unwrap();
    assert!(matches!(
        alice_device.create_group(&[package]),
        Err(Error::Lifetime)
    ));
}

#[test]
fn commit_adding_an_uncertified_member_is_refused() {
    let (_, alice_device, mut alice_group, _, bob_device, mut bob_group) = pair();
    let (identity, mut mallory) = device();
    with_certificate(
        &mut mallory,
        &identity.sign(Domain::KeyPackage, b"not a certificate".to_vec()),
    );
    let package = KeyPackageIn::tls_deserialize_exact(mallory.key_package(false).unwrap())
        .unwrap()
        .validate(alice_device.provider.crypto(), ProtocolVersion::Mls10)
        .unwrap();
    let (commit, _, _) = alice_group
        .group
        .add_members(&alice_device.provider, &alice_device.signer, &[package])
        .unwrap();
    let epoch = bob_group.group.epoch();
    assert!(matches!(
        bob_group.process(&bob_device, &commit.to_bytes().unwrap()),
        Err(Error::Credential)
    ));
    assert_eq!(bob_group.group.epoch(), epoch);
}

#[test]
fn certified_commit_advances_the_epoch() {
    let (_, alice_device, mut alice_group, _, bob_device, mut bob_group) = pair();
    let (_, carol_device) = device();
    let package = alice_device
        .verify_key_package(&carol_device.key_package(false).unwrap())
        .unwrap();
    let (commit, welcome, _) = alice_group
        .group
        .add_members(&alice_device.provider, &alice_device.signer, &[package])
        .unwrap();
    alice_group
        .group
        .merge_pending_commit(&alice_device.provider)
        .unwrap();
    assert_eq!(
        bob_group
            .process(&bob_device, &commit.to_bytes().unwrap())
            .unwrap(),
        Received::Commit
    );
    let mut carol_group = carol_device.join(&welcome.to_bytes().unwrap()).unwrap();
    assert_eq!(carol_group.members().unwrap().len(), 3);

    let sealed = carol_group.encrypt(&carol_device, b"hey").unwrap();
    assert!(bob_group.process(&bob_device, &sealed).is_ok());
}

#[test]
fn exported_secret_matches_across_members() {
    let (_, alice_device, alice_group, _, bob_device, bob_group) = pair();
    let ours = alice_group
        .export_secret(&alice_device, "dyapp/test", b"call", 32)
        .unwrap();
    assert_eq!(ours.len(), 32);
    assert_eq!(
        ours,
        bob_group
            .export_secret(&bob_device, "dyapp/test", b"call", 32)
            .unwrap()
    );
    assert_ne!(
        ours,
        bob_group
            .export_secret(&bob_device, "dyapp/test", b"other", 32)
            .unwrap()
    );
}
