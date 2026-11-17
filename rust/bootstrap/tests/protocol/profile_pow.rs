use super::*;

#[tokio::test]
async fn store_and_mixed_nodes_verify_profile_proofs_before_holding_copies() {
    for roles in [vec![Role::Store], vec![Role::Store, Role::Search]] {
        let (address, _, service) = start_config_with_service(move |config| {
            config.roles = roles;
            config.search.min_profile_pow_bits = 2;
            config.limits.requests_per_second = 1000;
            config.limits.min_free_mb = 0;
        })
        .await;
        let mut client = Client::connect(&address).await;
        assert_eq!(client.info().await.info.unwrap().min_profile_pow, 2);
        let owner = Identity::try_generate().unwrap();
        let key = owner.public_key();
        let owner_id = owner.peer_id();
        assert_weak_rejected(&mut client, &service, &owner, &owner_id).await;
        let forged = assert_bad_nonce(&mut client, &service, &owner, &key, &owner_id).await;
        let (first, first_nonce) =
            publish_first_valid(&mut client, &service, &owner, &key, &owner_id).await;
        assert_stale_replay(&mut client, &service, first).await;
        publish_second_valid(&mut client, &service, &owner, &key, &owner_id, first_nonce).await;
        assert_eq!(
            profile_status(&mut client, profile_request::Request::ReplicaPut(forged)).await,
            Status::Denied
        );
        assert_eq!(service.pow.totals(), (4, 2));
        assert_eq!(status(client.info().await.status), Status::Ok);
    }
}

async fn profile_status(client: &mut Client, request: profile_request::Request) -> Status {
    status(client.profile(request).await.status)
}

fn pow_profile(owner: &Identity, version: u64, nonce: u64) -> SignedRecord {
    Profile {
        version,
        pow_bits: 2,
        pow_nonce: nonce,
        ..Default::default()
    }
    .sign(owner)
}

async fn assert_weak_rejected(
    client: &mut Client,
    service: &Service,
    owner: &Identity,
    owner_id: &str,
) {
    let weak = Profile {
        version: 1,
        ..Default::default()
    }
    .sign(owner);
    for request in [
        profile_request::Request::Publish(weak.clone()),
        profile_request::Request::ReplicaPut(weak),
    ] {
        let reply = client.profile(request).await;
        assert_eq!(
            (status(reply.status), reply.required_pow_bits),
            (Status::Invalid, 2)
        );
    }
    assert!(service.store.get_profile(owner_id).unwrap().is_none());
    assert_eq!(service.pow.totals(), (0, 0));
}

async fn assert_bad_nonce(
    client: &mut Client,
    service: &Service,
    owner: &Identity,
    key: &[u8; 32],
    owner_id: &str,
) -> SignedRecord {
    let bad_nonce = (0..1_000)
        .find(|nonce| dyapp_profile::pow_leading_bits(key, *nonce) < 2)
        .unwrap();
    let forged = pow_profile(owner, 3, bad_nonce);
    assert_eq!(
        profile_status(client, profile_request::Request::ReplicaPut(forged.clone())).await,
        Status::Denied
    );
    assert!(service.store.get_profile(owner_id).unwrap().is_none());
    assert_eq!(service.pow.totals(), (1, 1));
    forged
}

async fn publish_first_valid(
    client: &mut Client,
    service: &Service,
    owner: &Identity,
    key: &[u8; 32],
    owner_id: &str,
) -> (SignedRecord, u64) {
    let nonce = dyapp_profile::solve(key, 2, 0, 100_000).unwrap();
    let first = pow_profile(owner, 1, nonce);
    assert_eq!(
        profile_status(client, profile_request::Request::Publish(first.clone())).await,
        Status::Ok
    );
    assert_eq!(
        service.store.get_profile(owner_id).unwrap(),
        Some(first.clone())
    );
    assert_eq!(service.pow.totals(), (2, 1));
    (first, nonce)
}

async fn assert_stale_replay(client: &mut Client, service: &Service, first: SignedRecord) {
    assert_eq!(
        profile_status(client, profile_request::Request::Publish(first)).await,
        Status::Stale
    );
    assert_eq!(service.pow.totals(), (2, 1));
}

async fn publish_second_valid(
    client: &mut Client,
    service: &Service,
    owner: &Identity,
    key: &[u8; 32],
    owner_id: &str,
    first_nonce: u64,
) {
    let nonce = dyapp_profile::solve(key, 2, first_nonce + 1, 100_000).unwrap();
    let second = pow_profile(owner, 2, nonce);
    assert_eq!(
        profile_status(client, profile_request::Request::ReplicaPut(second.clone())).await,
        Status::Ok
    );
    assert_eq!(service.store.get_profile(owner_id).unwrap(), Some(second));
    assert_eq!(service.pow.totals(), (3, 1));
}

#[tokio::test]
async fn refused_profile_flood_keeps_the_tcp_connection_usable() {
    let address = start_config(|config| {
        config.roles = vec![Role::Store];
        config.search.min_profile_pow_bits = 2;
        config.limits.requests_per_second = 5;
        config.limits.min_free_mb = 0;
    })
    .await;
    let mut client = Client::connect(&address).await;
    let weak = Profile {
        version: 1,
        ..Default::default()
    }
    .sign(&Identity::try_generate().unwrap());
    assert_eq!(
        status(
            client
                .profile(profile_request::Request::Publish(weak.clone()))
                .await
                .status
        ),
        Status::Invalid
    );
    assert_eq!(
        status(
            client
                .profile(profile_request::Request::Publish(weak))
                .await
                .status
        ),
        Status::RateLimited
    );
    assert_eq!(status(client.info().await.status), Status::Ok);
}
