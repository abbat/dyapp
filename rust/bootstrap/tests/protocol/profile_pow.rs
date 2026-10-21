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
        let owner = Identity::generate();
        let key = owner.public_key();
        let owner_id = owner.peer_id();
        let weak = Profile {
            version: 1,
            ..Default::default()
        }
        .sign(&owner);
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
        assert!(service.store.get_profile(&owner_id).unwrap().is_none());
        assert_eq!(service.pow.totals(), (0, 0));

        let bad_nonce = (0..)
            .find(|nonce| dyapp_profile::pow_leading_bits(&key, *nonce) < 2)
            .unwrap();
        let forged = Profile {
            version: 3,
            pow_bits: 2,
            pow_nonce: bad_nonce,
            ..Default::default()
        }
        .sign(&owner);
        assert_eq!(
            status(
                client
                    .profile(profile_request::Request::ReplicaPut(forged.clone()))
                    .await
                    .status
            ),
            Status::Denied
        );
        assert!(service.store.get_profile(&owner_id).unwrap().is_none());
        assert_eq!(service.pow.totals(), (1, 1));

        let first_nonce = dyapp_profile::solve(&key, 2, 0, 100_000).unwrap();
        let first = Profile {
            version: 1,
            pow_bits: 2,
            pow_nonce: first_nonce,
            ..Default::default()
        }
        .sign(&owner);
        assert_eq!(
            status(
                client
                    .profile(profile_request::Request::Publish(first.clone()))
                    .await
                    .status
            ),
            Status::Ok
        );
        assert_eq!(
            service.store.get_profile(&owner_id).unwrap(),
            Some(first.clone())
        );
        assert_eq!(service.pow.totals(), (2, 1));

        assert_eq!(
            status(
                client
                    .profile(profile_request::Request::Publish(first))
                    .await
                    .status
            ),
            Status::Stale
        );
        assert_eq!(service.pow.totals(), (2, 1));

        let second_nonce = dyapp_profile::solve(&key, 2, first_nonce + 1, 100_000).unwrap();
        let second = Profile {
            version: 2,
            pow_bits: 2,
            pow_nonce: second_nonce,
            ..Default::default()
        }
        .sign(&owner);
        assert_eq!(
            status(
                client
                    .profile(profile_request::Request::ReplicaPut(second.clone()))
                    .await
                    .status
            ),
            Status::Ok
        );
        assert_eq!(service.store.get_profile(&owner_id).unwrap(), Some(second));
        assert_eq!(service.pow.totals(), (3, 1));

        assert_eq!(
            status(
                client
                    .profile(profile_request::Request::ReplicaPut(forged))
                    .await
                    .status
            ),
            Status::Denied
        );
        assert_eq!(service.pow.totals(), (4, 2));
        assert_eq!(status(client.info().await.status), Status::Ok);
    }
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
    .sign(&Identity::generate());
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
