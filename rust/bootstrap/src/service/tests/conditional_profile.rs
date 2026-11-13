//! All storage ingress routes enforce the same owner-signed fence.
use super::*;
use dyapp_profile::ProfileCondition;

fn fixture() -> (Service, Identity) {
    let mut config = NodeConfig::default();
    config.storage.dir =
        std::env::temp_dir().join(format!("profile-condition-{}", uuid::Uuid::new_v4()));
    config.network.id_pow_bits = 0;
    config.search.min_profile_pow_bits = 0;
    config.limits.requests_per_second = 1000;
    config.limits.min_free_mb = 0;
    let store = BootstrapStore::new(config.storage.dir.to_str().unwrap()).unwrap();
    (Service::new(store, config), Identity::from_secret(&[7; 32]))
}

fn record(owner: &Identity, version: u64, name: &str) -> SignedRecord {
    Profile {
        version,
        name: name.into(),
        ..Default::default()
    }
    .sign(owner)
}

fn target(owner: &Identity, source: Option<&SignedRecord>) -> SignedRecord {
    Profile {
        version: 9,
        name: "owner-approved fields".into(),
        previous: Some(ProfileCondition::observed(source).unwrap()),
        ..Default::default()
    }
    .sign(owner)
}

fn request(request: profile_request::Request) -> ProfileRequest {
    ProfileRequest {
        request: Some(request),
        age_seconds: 0,
    }
}

#[test]
fn newer_revocation_survives_conditional_legacy_and_replica_replays() {
    for route in 0..3 {
        let (service, owner) = fixture();
        let source = record(&owner, 2, "reviewed");
        let target = target(&owner, Some(&source));
        service.store.put_profile(&source).unwrap();
        let revoked = record(&owner, 3, "revoked");
        service.store.put_profile(&revoked).unwrap();
        let operation = match route {
            0 => profile_request::Request::ConditionalPublish(target),
            1 => profile_request::Request::Publish(target),
            _ => profile_request::Request::ReplicaPut(target),
        };
        let response = service
            .profile(&replica_peer("publisher"), request(operation))
            .unwrap();
        assert_eq!(status(&response), Status::Stale);
        assert_eq!(
            service.store.get_profile(&owner.peer_id()).unwrap(),
            Some(revoked)
        );
    }
}

#[test]
fn observed_fork_and_absence_are_enforced_but_old_replicas_can_catch_up() {
    for current in [
        None,
        Some((1, "older")),
        Some((2, "reviewed")),
        Some((2, "fork")),
    ] {
        let (service, owner) = fixture();
        let source = record(&owner, 2, "reviewed");
        if let Some((version, name)) = current {
            service
                .store
                .put_profile(&record(&owner, version, name))
                .unwrap();
        }
        let response = service
            .profile(
                &peer("owner"),
                request(profile_request::Request::ConditionalPublish(target(
                    &owner,
                    Some(&source),
                ))),
            )
            .unwrap();
        let expected = if current == Some((2, "fork")) {
            Status::Stale
        } else {
            Status::Ok
        };
        assert_eq!(status(&response), expected);
    }
    let (service, owner) = fixture();
    service
        .store
        .put_profile(&record(&owner, 1, "appeared"))
        .unwrap();
    let response = service
        .profile(
            &peer("owner"),
            request(profile_request::Request::ConditionalPublish(target(
                &owner, None,
            ))),
        )
        .unwrap();
    assert_eq!(status(&response), Status::Stale);
}

#[test]
fn admission_refuses_a_conflict_before_fanout_and_exact_retries_return_the_target() {
    let (service, owner) = fixture();
    let source = record(&owner, 2, "reviewed");
    let attempted = target(&owner, Some(&source));
    service
        .store
        .put_profile(&record(&owner, 3, "intervening"))
        .unwrap();
    let response = service
        .prepare_profile_request_with_pow(
            &peer("owner"),
            request(profile_request::Request::ConditionalPublish(attempted)),
            &|_| None,
        )
        .unwrap();
    assert_eq!(status(&response), Status::Stale);

    let (service, owner) = fixture();
    let target = target(&owner, None);
    let operation = || request(profile_request::Request::ConditionalPublish(target.clone()));
    assert_eq!(
        status(&service.profile(&peer("owner"), operation()).unwrap()),
        Status::Ok
    );
    let response = service.profile(&peer("owner"), operation()).unwrap();
    assert_eq!(status(&response), Status::Stale);
    assert_eq!(response.record, Some(target));
}

#[test]
fn conditional_endpoint_rejects_legacy_authorization_and_legacy_endpoint_remains_explicit() {
    let (service, owner) = fixture();
    let legacy = record(&owner, 1, "legacy");
    let response = service
        .profile(
            &peer("owner"),
            request(profile_request::Request::ConditionalPublish(legacy.clone())),
        )
        .unwrap();
    assert_eq!(status(&response), Status::Denied);
    assert_eq!(
        status(&service.profile(&peer("owner"), publish(legacy)).unwrap()),
        Status::Ok
    );
}
