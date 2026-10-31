use super::*;

#[tokio::test]
async fn keypackage_publish_and_consume_over_tcp() {
    dyapp_p2p_net::set_id_pow_bits(0);
    let address = start_config(|config| {
        config.roles = vec![Role::Store, Role::Search];
        config.limits.requests_per_second = 1000;
        config.limits.min_free_mb = 0;
        config.search.min_profile_pow_bits = 0;
    })
    .await;
    let mut client = Client::connect(&address).await;
    let owner = Identity::generate();
    let device = Identity::generate();
    let mls = Identity::generate();
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
    let signed = device.sign(
        Domain::KeyPackage,
        proto::KeyPackageRecord {
            owner: owner_hash.clone(),
            mailbox: mailbox.clone(),
            package,
            expires: chrono::Utc::now().timestamp().unsigned_abs() + 3600,
            ..Default::default()
        }
        .encode_to_vec(),
    );
    // Rejected without the profile, and answered at once rather than replicated.
    assert_eq!(
        status(
            client
                .keypackage(proto::key_package_request::Request::Publish(signed.clone()))
                .await
                .status
        ),
        Status::Denied
    );
    assert_eq!(
        status(
            client
                .profile(profile_request::Request::Publish(profile.clone()))
                .await
                .status
        ),
        Status::Ok
    );
    // Search-only withdrawal leaves the store's signed device directory intact.
    let tombstone = Profile::tombstone(2, 0, 0).sign(&owner);
    assert_eq!(
        status(
            client
                .search(proto::search_request::Request::Publish(profile))
                .await
                .status
        ),
        Status::Ok
    );
    assert_eq!(
        status(
            client
                .search(proto::search_request::Request::Publish(tombstone.clone()))
                .await
                .status
        ),
        Status::Ok
    );
    assert_eq!(
        status(
            client
                .keypackage(proto::key_package_request::Request::Publish(signed.clone()))
                .await
                .status
        ),
        Status::Ok
    );
    let fetch = proto::key_package_request::Request::Fetch(proto::GetKeyPackage {
        owner: owner_hash.clone(),
        mailbox: mailbox.clone(),
        profile_version: 1,
        slot,
    });
    assert_eq!(client.keypackage(fetch.clone()).await.record, Some(signed));
    assert_eq!(
        status(client.keypackage(fetch.clone()).await.status),
        Status::NotFound
    );
    let fallback = device.sign(
        Domain::KeyPackage,
        proto::KeyPackageRecord {
            owner: owner_hash,
            mailbox,
            package: vec![0, 1, 0, 1, 8],
            expires: chrono::Utc::now().timestamp().unsigned_abs() + 3600,
            last_resort: true,
            revision: 1,
        }
        .encode_to_vec(),
    );
    assert_eq!(
        status(
            client
                .keypackage(proto::key_package_request::Request::Publish(
                    fallback.clone()
                ))
                .await
                .status
        ),
        Status::Ok
    );
    for _ in 0..2 {
        assert_eq!(
            client.keypackage(fetch.clone()).await.record,
            Some(fallback.clone())
        );
    }
    // Final withdrawal reaches profile storage; known-peer KeyPackage lookup now stops.
    assert_eq!(
        status(
            client
                .profile(profile_request::Request::Publish(tombstone))
                .await
                .status
        ),
        Status::Ok
    );
    assert_eq!(
        status(client.keypackage(fetch).await.status),
        Status::NotFound
    );
    // A final profile-store tombstone cannot be revived while the node retains it.
    let revived = Profile {
        version: 3,
        ..Default::default()
    }
    .sign(&owner);
    assert_eq!(
        status(
            client
                .profile(profile_request::Request::Publish(revived))
                .await
                .status
        ),
        Status::Invalid
    );
}
