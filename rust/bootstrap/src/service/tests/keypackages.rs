use super::*;
use dyapp_profile::DeviceCertificate;

fn setup() -> (Service, Identity, Identity, Vec<u8>) {
    let service = service();
    let owner = Identity::generate();
    let device = Identity::generate();
    let mls = Identity::generate();
    let certificate = owner.sign(
        Domain::DeviceCertificate,
        DeviceCertificate {
            device_key: device.public_key().to_vec(),
            mls_signature_key: mls.public_key().to_vec(),
            generation: 1,
        }
        .encode_to_vec(),
    );
    let profile = Profile {
        version: 1,
        devices: vec![certificate],
        ..Default::default()
    };
    assert_eq!(
        status_of(service.store_profile(&profile.sign(&owner)).unwrap().status),
        Status::Ok
    );
    let mailbox = dyapp_identity::key_hash(&device.public_key()).to_vec();
    (service, owner, device, mailbox)
}

fn package(
    owner: &Identity,
    device: &Identity,
    mailbox: &[u8],
    suffix: u8,
    last_resort: bool,
) -> SignedRecord {
    let mut bytes = vec![0, 1, 0, 1, suffix];
    bytes.extend_from_slice(&[suffix; 8]);
    device.sign(
        Domain::KeyPackage,
        proto::KeyPackageRecord {
            owner: dyapp_identity::key_hash(&owner.public_key()).to_vec(),
            mailbox: mailbox.to_vec(),
            package: bytes,
            expires: chrono::Utc::now().timestamp().unsigned_abs() + 3600,
            last_resort,
            revision: u32::from(last_resort),
        }
        .encode_to_vec(),
    )
}

fn fetch(owner: &Identity, mailbox: &[u8], slot: u8) -> proto::KeyPackageRequest {
    proto::KeyPackageRequest {
        request: Some(proto::key_package_request::Request::Fetch(
            proto::GetKeyPackage {
                owner: dyapp_identity::key_hash(&owner.public_key()).to_vec(),
                mailbox: mailbox.to_vec(),
                profile_version: 1,
                slot: u32::from(slot),
            },
        )),
    }
}

#[test]
fn publication_requires_current_certificate_and_device_signature() {
    let (service, owner, device, mailbox) = setup();
    let good = package(&owner, &device, &mailbox, 1, false);
    assert_eq!(
        status_of(service.store_keypackage(&good).unwrap().status),
        Status::Ok
    );
    let mut forged = good.clone();
    forged.signature[0] ^= 1;
    assert_eq!(
        status_of(service.store_keypackage(&forged).unwrap().status),
        Status::Denied
    );
    let stranger = Identity::generate();
    let unlisted = package(
        &owner,
        &stranger,
        &dyapp_identity::key_hash(&stranger.public_key()),
        2,
        false,
    );
    assert_eq!(
        status_of(service.store_keypackage(&unlisted).unwrap().status),
        Status::Denied
    );
    let mut expired = proto::KeyPackageRecord::decode(good.payload.as_slice()).unwrap();
    expired.expires = 1;
    let expired = device.sign(Domain::KeyPackage, expired.encode_to_vec());
    assert_eq!(
        status_of(service.store_keypackage(&expired).unwrap().status),
        Status::Invalid
    );
}

#[test]
fn ordinary_package_is_consumed_once_and_last_resort_is_reused() {
    let (service, owner, device, mailbox) = setup();
    let ordinary = package(&owner, &device, &mailbox, 1, false);
    let bytes = proto::KeyPackageRecord::decode(ordinary.payload.as_slice())
        .unwrap()
        .package;
    let hash = dyapp_identity::sha256(&bytes);
    let slot = hash[0] % dyapp_p2p_net::REPLICAS;
    service.store_keypackage(&ordinary).unwrap();
    let first = service
        .keypackage(&peer("one"), fetch(&owner, &mailbox, slot))
        .unwrap();
    assert_eq!(status_of(first.status), Status::Ok);
    assert_eq!(first.record, Some(ordinary.clone()));
    let second = service
        .keypackage(&peer("two"), fetch(&owner, &mailbox, slot))
        .unwrap();
    assert_eq!(status_of(second.status), Status::NotFound);
    // Replaying the publication does not reset the consumed marker.
    service.store_keypackage(&ordinary).unwrap();
    let third = service
        .keypackage(&peer("three"), fetch(&owner, &mailbox, slot))
        .unwrap();
    assert_eq!(status_of(third.status), Status::NotFound);
    let fallback = package(&owner, &device, &mailbox, 2, true);
    service.store_keypackage(&fallback).unwrap();
    for id in ["four", "five"] {
        let answer = service
            .keypackage(&peer(id), fetch(&owner, &mailbox, slot))
            .unwrap();
        assert_eq!(answer.record, Some(fallback.clone()));
    }
}

#[test]
fn stock_is_bounded_and_removed_device_cannot_publish_or_fetch() {
    let (service, owner, device, mailbox) = setup();
    for suffix in 0..20 {
        assert_eq!(
            status_of(
                service
                    .store_keypackage(&package(&owner, &device, &mailbox, suffix, false))
                    .unwrap()
                    .status
            ),
            Status::Ok
        );
    }
    assert_eq!(
        status_of(
            service
                .store_keypackage(&package(&owner, &device, &mailbox, 21, false))
                .unwrap()
                .status
        ),
        Status::Full
    );
    let removed = Profile {
        version: 2,
        ..Default::default()
    }
    .sign(&owner);
    assert_eq!(
        status_of(service.store_profile(&removed).unwrap().status),
        Status::Ok
    );
    assert_eq!(
        status_of(
            service
                .store_keypackage(&package(&owner, &device, &mailbox, 22, false))
                .unwrap()
                .status
        ),
        Status::Denied
    );
    assert_eq!(
        status_of(
            service
                .keypackage(&peer("removed"), fetch(&owner, &mailbox, 0))
                .unwrap()
                .status
        ),
        Status::NotFound
    );
}
