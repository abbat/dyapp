//! Repair carries the original expiry of envelopes and the age of profiles.
use super::write_token::{envelope, mailbox_of, put, today, token};
use super::*;

/// A replica put of envelopes `ids` with `remaining` seconds each, and the stored expiries.
fn replicate(
    ids: &[u8],
    remaining: Vec<u32>,
) -> (dyapp_test_support::TestDir, Service, Vec<Option<i64>>) {
    let (owner, contact) = (
        Identity::try_generate().unwrap(),
        Identity::try_generate().unwrap(),
    );
    let mailbox = mailbox_of(&owner);
    let (service_test_dir, service) = service();
    let records = ids.iter().map(|&id| {
        let token = token(&owner, &mailbox, &contact, today());
        contact.sign(
            Domain::Envelope,
            envelope(id, &mailbox, token).encode_to_vec(),
        )
    });
    let batch = proto::Envelopes {
        envelopes: records.collect(),
        remaining_seconds: remaining,
        ..Default::default()
    };
    let request = mailbox_request::Request::ReplicaPut(batch);
    assert_eq!(put(&service, &replica_peer("r"), request), Status::Ok);
    let held = service.held(&mailbox).unwrap();
    let expiries = ids
        .iter()
        .map(|&id| {
            held.get(&vec![id; 16])
                .map(|(_, _, expires_at)| *expires_at)
        })
        .collect();
    (service_test_dir, service, expiries)
}

fn near(actual: Option<i64>, expected: i64) {
    let actual = actual.expect("stored");
    assert!((actual - expected).abs() <= 5, "{actual} vs {expected}");
}

#[test]
fn a_replica_put_carries_the_remaining_time_capped_at_the_retention() {
    let (_test_dir, service, expiries) = replicate(&[1, 2], vec![600, u32::MAX]);
    let now = crate::clock::Clock::system().timestamp();
    near(expiries[0], now + 600);
    near(expiries[1], now + service.retention());
}

#[test]
fn a_zero_remaining_time_drops_the_envelope() {
    let (_test_dir, _, expiries) = replicate(&[1, 2], vec![0, 600]);
    assert_eq!(expiries[0], None);
    assert!(expiries[1].is_some());
}

#[test]
fn a_mismatched_or_missing_list_gives_a_fresh_retention() {
    for remaining in [vec![], vec![600]] {
        let (_test_dir, service, expiries) = replicate(&[1, 2], remaining);
        let fresh = crate::clock::Clock::system().timestamp() + service.retention();
        near(expiries[0], fresh);
        near(expiries[1], fresh);
    }
}

#[test]
fn a_carried_expiry_leaves_a_stored_envelope_alone() {
    let (owner, contact) = (
        Identity::try_generate().unwrap(),
        Identity::try_generate().unwrap(),
    );
    let mailbox = mailbox_of(&owner);
    let (_service_test_dir, service) = service();
    let token = token(&owner, &mailbox, &contact, today());
    let record = contact.sign(
        Domain::Envelope,
        envelope(1, &mailbox, token).encode_to_vec(),
    );
    let request = mailbox_request::Request::Put(record.clone());
    assert_eq!(put(&service, &peer("c"), request), Status::Ok);
    let batch = proto::Envelopes {
        envelopes: vec![record],
        remaining_seconds: vec![60],
        ..Default::default()
    };
    let request = mailbox_request::Request::ReplicaPut(batch);
    assert_eq!(put(&service, &replica_peer("r"), request), Status::Ok);
    let fresh = crate::clock::Clock::system().timestamp() + service.retention();
    near(Some(service.held(&mailbox).unwrap()[&vec![1; 16]].2), fresh);
}

#[test]
fn a_profile_replica_carries_the_owner_age_and_an_expired_one_is_not_stored() {
    let (_service_test_dir, service) = service();
    let ttl = service.config.limits.profile_ttl_days * 86_400;
    let send = |identity: &Identity, age_seconds| {
        let request = ProfileRequest {
            request: Some(profile_request::Request::ReplicaPut(
                Profile {
                    version: 1,
                    ..Profile::default()
                }
                .sign(identity),
            )),
            age_seconds,
        };
        status(&service.profile(&replica_peer("r"), request).unwrap())
    };
    let (aged, expired) = (
        Identity::try_generate().unwrap(),
        Identity::try_generate().unwrap(),
    );
    assert_eq!(send(&aged, 3600), Status::Ok);
    let seen = service.store.profile_last_seen(&aged.peer_id()).unwrap();
    near(seen, crate::clock::Clock::system().timestamp() - 3600);
    assert_eq!(send(&expired, ttl), Status::Ok);
    assert!(service
        .store
        .get_profile(&expired.peer_id())
        .unwrap()
        .is_none());
}
