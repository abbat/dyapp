use super::*;

struct KeyPackageFixture {
    client: Client,
    owner: Identity,
    device: Identity,
    profile: SignedRecord,
    owner_hash: Vec<u8>,
    mailbox: Vec<u8>,
    slot: u32,
}

#[tokio::test]
async fn keypackage_publish_and_consume_over_tcp() {
    let mut fixture = keypackage_fixture().await;
    let (signed, fetch) = regular_package(&fixture);
    publish_hidden_profile(&mut fixture, signed.clone()).await;
    assert_eq!(
        fixture.client.keypackage(fetch.clone()).await.record,
        Some(signed)
    );
    assert_keypackage_status(&mut fixture.client, fetch.clone(), Status::NotFound).await;
    let fallback = publish_fallback(&mut fixture).await;
    for _ in 0..2 {
        assert_eq!(
            fixture.client.keypackage(fetch.clone()).await.record,
            Some(fallback.clone())
        );
    }
    assert_final_withdrawal(&mut fixture, fetch).await;
}

async fn keypackage_fixture() -> KeyPackageFixture {
    dyapp_p2p_net::set_id_pow_bits(0);
    let address = start_config(|config| {
        config.roles = vec![Role::Store, Role::Search];
        config.limits.requests_per_second = 1000;
        config.limits.min_free_mb = 0;
        config.search.min_profile_pow_bits = 0;
    })
    .await;
    let client = Client::connect(&address).await;
    let owner = Identity::try_generate().unwrap();
    let device = Identity::try_generate().unwrap();
    let mls = Identity::try_generate().unwrap();
    let certificate = owner.sign(
        Domain::DeviceCertificate,
        dyapp_profile::DeviceCertificate {
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
    }
    .sign(&owner);
    let owner_hash = dyapp_identity::key_hash(&owner.public_key()).to_vec();
    let mailbox = dyapp_identity::key_hash(&device.public_key()).to_vec();
    let package = vec![0, 1, 0, 1, 7];
    let slot = u32::from(dyapp_identity::sha256(&package)[0] % dyapp_p2p_net::REPLICAS);
    KeyPackageFixture {
        client,
        owner,
        device,
        profile,
        owner_hash,
        mailbox,
        slot,
    }
}

fn signed_package(
    fixture: &KeyPackageFixture,
    package: Vec<u8>,
    last_resort: bool,
    revision: u32,
) -> SignedRecord {
    fixture.device.sign(
        Domain::KeyPackage,
        proto::KeyPackageRecord {
            owner: fixture.owner_hash.clone(),
            mailbox: fixture.mailbox.clone(),
            package,
            expires: dyapp_bootstrap::clock::Clock::system()
                .timestamp()
                .unsigned_abs()
                + 3600,
            last_resort,
            revision,
        }
        .encode_to_vec(),
    )
}

fn regular_package(
    fixture: &KeyPackageFixture,
) -> (SignedRecord, proto::key_package_request::Request) {
    let signed = signed_package(fixture, vec![0, 1, 0, 1, 7], false, 0);
    let fetch = proto::key_package_request::Request::Fetch(proto::GetKeyPackage {
        owner: fixture.owner_hash.clone(),
        mailbox: fixture.mailbox.clone(),
        profile_version: 1,
        slot: fixture.slot,
    });
    (signed, fetch)
}

async fn assert_keypackage_status(
    client: &mut Client,
    request: proto::key_package_request::Request,
    expected: Status,
) {
    assert_eq!(status(client.keypackage(request).await.status), expected);
}

async fn publish_hidden_profile(fixture: &mut KeyPackageFixture, signed: SignedRecord) {
    // Rejected without the profile, and answered at once rather than replicated.
    eprintln!("keypackage test: reject before profile");
    assert_keypackage_status(
        &mut fixture.client,
        proto::key_package_request::Request::Publish(signed.clone()),
        Status::Denied,
    )
    .await;
    eprintln!("keypackage test: publish profile");
    assert_eq!(
        status(
            fixture
                .client
                .profile(profile_request::Request::Publish(fixture.profile.clone()))
                .await
                .status
        ),
        Status::Ok
    );
    // Search-only withdrawal leaves the store's signed device directory intact.
    eprintln!("keypackage test: publish search profile");
    assert_eq!(
        status(
            fixture
                .client
                .search(proto::search_request::Request::Publish(
                    fixture.profile.clone(),
                ))
                .await
                .status
        ),
        Status::Ok
    );
    publish_search_tombstone(fixture).await;
    eprintln!("keypackage test: publish key package while hidden");
    assert_keypackage_status(
        &mut fixture.client,
        proto::key_package_request::Request::Publish(signed),
        Status::Ok,
    )
    .await;
}

async fn publish_search_tombstone(fixture: &mut KeyPackageFixture) {
    eprintln!("keypackage test: publish search tombstone");
    let tombstone = Profile::tombstone(2, 0, 0).sign(&fixture.owner);
    assert_eq!(
        status(
            fixture
                .client
                .search(proto::search_request::Request::Publish(tombstone))
                .await
                .status
        ),
        Status::Ok
    );
}

async fn publish_fallback(fixture: &mut KeyPackageFixture) -> SignedRecord {
    let fallback = signed_package(fixture, vec![0, 1, 0, 1, 8], true, 1);
    assert_keypackage_status(
        &mut fixture.client,
        proto::key_package_request::Request::Publish(fallback.clone()),
        Status::Ok,
    )
    .await;
    fallback
}

async fn assert_final_withdrawal(
    fixture: &mut KeyPackageFixture,
    fetch: proto::key_package_request::Request,
) {
    // Final withdrawal reaches profile storage; known-peer KeyPackage lookup now stops.
    eprintln!("keypackage test: publish final profile tombstone");
    assert_eq!(
        status(
            fixture
                .client
                .profile(deletion(&fixture.owner, &fixture.profile))
                .await
                .status
        ),
        Status::Ok
    );
    assert_keypackage_status(&mut fixture.client, fetch, Status::NotFound).await;
    // A final profile-store tombstone cannot be revived while the node retains it.
    eprintln!("keypackage test: reject revival");
    let revived = Profile {
        version: 3,
        ..Default::default()
    }
    .sign(&fixture.owner);
    assert_eq!(
        status(
            fixture
                .client
                .profile(profile_request::Request::Publish(revived))
                .await
                .status
        ),
        Status::Invalid
    );
}

/// Account deletion needs the owner's condition on the held record; a bare tombstone no longer grants it.
fn deletion(owner: &Identity, held: &SignedRecord) -> profile_request::Request {
    let tombstone = Profile {
        previous: Some(dyapp_profile::ProfileCondition::observed(Some(held)).unwrap()),
        ..Profile::tombstone(2, 0, 0)
    };
    profile_request::Request::ConditionalPublish(tombstone.sign(owner))
}
