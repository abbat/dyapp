use dyapp_profile::Profile;

use super::*;

fn device() -> (Identity, Device) {
    let identity = Identity::generate();
    let device = Device::new(&identity, [7; 32], 1).unwrap();
    (identity, device)
}

// The owner's newest profile, listing `devices`.
fn profile(owner: &Identity, devices: &[&Device]) -> VerifiedProfile {
    VerifiedProfile {
        peer_id: owner.peer_id(),
        profile: Profile {
            devices: devices.iter().map(|d| d.certificate().clone()).collect(),
            ..Profile::default()
        },
    }
}

// A device whose credential carries `certificate` instead of its own.
fn with_certificate(device: &mut Device, certificate: &SignedRecord) {
    device.credential.credential = BasicCredential::new(certificate.encode_to_vec()).into();
    device.certificate = certificate.clone();
}

struct Pair {
    alice: Identity,
    alice_device: Device,
    alice_group: Group,
    bob: Identity,
    bob_device: Device,
    bob_group: Group,
    profiles: Vec<VerifiedProfile>,
}

fn pair() -> Pair {
    let (alice, alice_device) = device();
    let (bob, bob_device) = device();
    let profiles = vec![
        profile(&alice, &[&alice_device]),
        profile(&bob, &[&bob_device]),
    ];
    let (alice_group, welcome) = alice_device
        .create_group(&[bob_device.key_package(false).unwrap()], &profiles)
        .unwrap();
    let bob_group = bob_device.join(&welcome, &profiles).unwrap();
    Pair {
        alice,
        alice_device,
        alice_group,
        bob,
        bob_device,
        bob_group,
        profiles,
    }
}

#[test]
fn members_exchange_messages_both_ways() {
    let mut p = pair();
    assert_eq!(p.alice_group.id(), p.bob_group.id());
    assert_eq!(
        p.bob_group.members().unwrap(),
        [p.alice.peer_id(), p.bob.peer_id()]
    );

    let sealed = p.alice_group.encrypt(&p.alice_device, b"hello").unwrap();
    assert!(!sealed.windows(5).any(|w| w == b"hello"));
    assert_eq!(
        p.bob_group
            .process(&p.bob_device, &sealed, &p.profiles)
            .unwrap(),
        Received::Message {
            sender: p.alice.peer_id(),
            plaintext: b"hello".to_vec()
        }
    );
    let reply = p.bob_group.encrypt(&p.bob_device, b"hi").unwrap();
    assert_eq!(
        p.alice_group
            .process(&p.alice_device, &reply, &p.profiles)
            .unwrap(),
        Received::Message {
            sender: p.bob.peer_id(),
            plaintext: b"hi".to_vec()
        }
    );
}

#[test]
fn tampered_or_replayed_message_is_rejected() {
    let mut p = pair();
    let mut tampered = p.alice_group.encrypt(&p.alice_device, b"hello").unwrap();
    *tampered.last_mut().unwrap() ^= 1;
    assert!(p
        .bob_group
        .process(&p.bob_device, &tampered, &p.profiles)
        .is_err());
    let sealed = p.alice_group.encrypt(&p.alice_device, b"hello").unwrap();
    assert!(p
        .bob_group
        .process(&p.bob_device, &sealed, &p.profiles)
        .is_ok());
    assert!(p
        .bob_group
        .process(&p.bob_device, &sealed, &p.profiles)
        .is_err());
}

#[test]
fn another_group_cannot_read_a_message() {
    let mut p = pair();
    let mut q = pair();
    assert_ne!(p.alice_group.id(), q.alice_group.id());
    let sealed = p.alice_group.encrypt(&p.alice_device, b"hello").unwrap();
    let profiles = [p.profiles, q.profiles].concat();
    assert!(q
        .alice_group
        .process(&q.alice_device, &sealed, &profiles)
        .is_err());
}

#[test]
fn welcome_works_once_for_a_single_use_key_package() {
    let (alice, alice_device) = device();
    let (bob, bob_device) = device();
    let profiles = [
        profile(&alice, &[&alice_device]),
        profile(&bob, &[&bob_device]),
    ];
    let (_, welcome) = alice_device
        .create_group(&[bob_device.key_package(false).unwrap()], &profiles)
        .unwrap();
    bob_device.join(&welcome, &profiles).unwrap();
    assert!(bob_device.join(&welcome, &profiles).is_err());

    let last_resort = bob_device.key_package(true).unwrap();
    for _ in 0..2 {
        let (_, welcome) = alice_device
            .create_group(std::slice::from_ref(&last_resort), &profiles)
            .unwrap();
        bob_device.join(&welcome, &profiles).unwrap();
    }
}

#[test]
fn key_package_without_a_valid_certificate_is_rejected() {
    let (alice, alice_device) = device();
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
        let profiles = [
            profile(&alice, &[&alice_device]),
            profile(&bob, &[&bob_device]),
        ];
        let package = bob_device.key_package(false).unwrap();
        assert!(matches!(
            alice_device.create_group(&[package], &profiles),
            Err(Error::Credential)
        ));
    }
}

#[test]
fn key_package_of_an_unlisted_device_is_rejected() {
    let (alice, alice_device) = device();
    let (bob, bob_device) = device();
    let other = Device::new(&bob, [8; 32], 2).unwrap();
    let package = bob_device.key_package(false).unwrap();
    for profiles in [
        vec![profile(&alice, &[&alice_device])],
        vec![profile(&alice, &[&alice_device]), profile(&bob, &[&other])],
    ] {
        assert!(matches!(
            alice_device.create_group(std::slice::from_ref(&package), &profiles),
            Err(Error::Revoked)
        ));
    }
}

#[test]
fn key_package_lasts_thirty_days() {
    let (_, device) = device();
    let package = KeyPackageIn::tls_deserialize_exact(device.key_package(false).unwrap())
        .unwrap()
        .validate(device.provider.crypto(), ProtocolVersion::Mls10)
        .unwrap();
    let lifetime = package.life_time();
    // openmls starts the lifetime an hour early.
    assert_eq!(
        lifetime.not_after() - lifetime.not_before(),
        30 * 24 * 3600 + 3600
    );
}

#[test]
fn key_package_with_another_suite_or_long_lifetime_is_rejected() {
    let (alice, alice_device) = device();
    let (bob, bob_device) = device();
    let profiles = [
        profile(&alice, &[&alice_device]),
        profile(&bob, &[&bob_device]),
    ];
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
        alice_device.create_group(&[package], &profiles),
        Err(Error::Ciphersuite)
    ));

    let package = bob_device
        .key_package_for(Lifetime::new(KEY_PACKAGE_LIFETIME_SECONDS + 1), false)
        .unwrap();
    assert!(matches!(
        alice_device.create_group(&[package], &profiles),
        Err(Error::Lifetime)
    ));
}

#[test]
fn commit_adding_an_uncertified_member_is_refused() {
    let mut p = pair();
    let (identity, mut mallory) = device();
    with_certificate(
        &mut mallory,
        &identity.sign(Domain::KeyPackage, b"not a certificate".to_vec()),
    );
    let package = KeyPackageIn::tls_deserialize_exact(mallory.key_package(false).unwrap())
        .unwrap()
        .validate(p.alice_device.provider.crypto(), ProtocolVersion::Mls10)
        .unwrap();
    let (commit, _, _) = p
        .alice_group
        .group
        .add_members(&p.alice_device.provider, &p.alice_device.signer, &[package])
        .unwrap();
    let epoch = p.bob_group.group.epoch();
    assert!(matches!(
        p.bob_group
            .process(&p.bob_device, &commit.to_bytes().unwrap(), &p.profiles),
        Err(Error::Credential)
    ));
    assert_eq!(p.bob_group.group.epoch(), epoch);
}

#[test]
fn certified_commit_advances_the_epoch() {
    let mut p = pair();
    let (carol, carol_device) = device();
    p.profiles.push(profile(&carol, &[&carol_device]));
    let package = p
        .alice_device
        .verify_key_package(&carol_device.key_package(false).unwrap(), &p.profiles)
        .unwrap();
    let (commit, welcome, _) = p
        .alice_group
        .group
        .add_members(&p.alice_device.provider, &p.alice_device.signer, &[package])
        .unwrap();
    p.alice_group
        .group
        .merge_pending_commit(&p.alice_device.provider)
        .unwrap();
    assert_eq!(
        p.bob_group
            .process(&p.bob_device, &commit.to_bytes().unwrap(), &p.profiles)
            .unwrap(),
        Received::Commit
    );
    let mut carol_group = carol_device
        .join(&welcome.to_bytes().unwrap(), &p.profiles)
        .unwrap();
    assert_eq!(carol_group.members().unwrap().len(), 3);

    let sealed = carol_group.encrypt(&carol_device, b"hey").unwrap();
    assert!(p
        .bob_group
        .process(&p.bob_device, &sealed, &p.profiles)
        .is_ok());
}

#[test]
fn revoked_device_is_refused_and_removed() {
    let (alice, alice_device) = device();
    let (bob, bob_phone) = device();
    let bob_laptop = Device::new(&bob, [8; 32], 1).unwrap();
    let both = [
        profile(&alice, &[&alice_device]),
        profile(&bob, &[&bob_phone, &bob_laptop]),
    ];
    let (mut alice_group, welcome) = alice_device
        .create_group(
            &[
                bob_phone.key_package(false).unwrap(),
                bob_laptop.key_package(false).unwrap(),
            ],
            &both,
        )
        .unwrap();
    let mut phone_group = bob_phone.join(&welcome, &both).unwrap();
    let mut laptop_group = bob_laptop.join(&welcome, &both).unwrap();
    assert_eq!(
        alice_group.remove_revoked(&alice_device, &both).unwrap(),
        None
    );

    // Bob loses the laptop and publishes a profile without it.
    let revoked = [
        profile(&alice, &[&alice_device]),
        profile(&bob, &[&bob_phone]),
    ];
    let sealed = laptop_group.encrypt(&bob_laptop, b"from a thief").unwrap();
    assert!(matches!(
        alice_group.process(&alice_device, &sealed, &revoked),
        Err(Error::Revoked)
    ));
    let commit = alice_group
        .remove_revoked(&alice_device, &revoked)
        .unwrap()
        .unwrap();
    assert_eq!(
        phone_group.process(&bob_phone, &commit, &revoked).unwrap(),
        Received::Commit
    );
    assert_eq!(phone_group.members().unwrap().len(), 2);
    let sealed = alice_group.encrypt(&alice_device, b"after").unwrap();
    assert!(laptop_group.process(&bob_laptop, &sealed, &both).is_err());
}

#[test]
fn device_restarted_between_messages_still_decrypts() {
    let mut p = pair();
    let key = [3; 32];
    let first = p.alice_group.encrypt(&p.alice_device, b"one").unwrap();
    p.bob_group
        .process(&p.bob_device, &first, &p.profiles)
        .unwrap();

    let sealed = p.bob_device.seal(&key).unwrap();
    let id = p.bob_group.id().to_vec();
    let bob_device = Device::open(&sealed, &key).unwrap();
    assert_eq!(bob_device.certificate(), p.bob_device.certificate());
    let mut bob_group = Group::load(&bob_device, &id).unwrap().unwrap();
    assert!(Group::load(&bob_device, b"no such group")
        .unwrap()
        .is_none());

    let second = p.alice_group.encrypt(&p.alice_device, b"two").unwrap();
    assert_eq!(
        bob_group
            .process(&bob_device, &second, &p.profiles)
            .unwrap(),
        Received::Message {
            sender: p.alice.peer_id(),
            plaintext: b"two".to_vec()
        }
    );
    let reply = bob_group.encrypt(&bob_device, b"back").unwrap();
    assert!(p
        .alice_group
        .process(&p.alice_device, &reply, &p.profiles)
        .is_ok());
}

#[test]
fn key_package_published_before_restart_can_be_joined() {
    let (alice, alice_device) = device();
    let (bob, bob_device) = device();
    let profiles = [
        profile(&alice, &[&alice_device]),
        profile(&bob, &[&bob_device]),
    ];
    let package = bob_device.key_package(false).unwrap();
    let key = [4; 32];
    let bob_device = Device::open(&bob_device.seal(&key).unwrap(), &key).unwrap();
    let (_, welcome) = alice_device.create_group(&[package], &profiles).unwrap();
    bob_device.join(&welcome, &profiles).unwrap();
}

#[test]
fn sealed_state_is_encrypted_and_authenticated() {
    let (_, device) = device();
    let key = [5; 32];
    let sealed = device.seal(&key).unwrap();
    let public = device.signer.public();
    assert!(!sealed.windows(public.len()).any(|w| w == public));
    assert_ne!(sealed, device.seal(&key).unwrap());

    assert_ne!(Device::state_key().unwrap(), Device::state_key().unwrap());

    let mut tampered = sealed.clone();
    *tampered.last_mut().unwrap() ^= 1;
    for (bytes, key) in [
        (&sealed[..], [6; 32]),
        (&tampered[..], key),
        (&sealed[..11], key),
    ] {
        assert!(matches!(Device::open(bytes, &key), Err(Error::State)));
    }
}

#[test]
fn exported_secret_matches_across_members() {
    let p = pair();
    let ours = p
        .alice_group
        .export_secret(&p.alice_device, "dyapp/test", b"call", 32)
        .unwrap();
    assert_eq!(ours.len(), 32);
    assert_eq!(
        ours,
        p.bob_group
            .export_secret(&p.bob_device, "dyapp/test", b"call", 32)
            .unwrap()
    );
    assert_ne!(
        ours,
        p.bob_group
            .export_secret(&p.bob_device, "dyapp/test", b"other", 32)
            .unwrap()
    );
}
