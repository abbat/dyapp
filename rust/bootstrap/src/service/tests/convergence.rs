//! Three holders converge after divergent writes, late join and out-of-order replica delivery.
use super::*;

fn record(owner: &Identity, version: u64) -> SignedRecord {
    Profile {
        version,
        name: format!("revision {version}"),
        ..Profile::default()
    }
    .sign(owner)
}

fn offer(node: &Service, record: &SignedRecord) -> Status {
    let response = node
        .profile(
            &replica_peer("convergence"),
            ProfileRequest {
                request: Some(profile_request::Request::ReplicaPut(record.clone())),
                age_seconds: 0,
            },
        )
        .unwrap();
    status(&response)
}

#[test]
fn split_three_holders_heal_with_a_late_joiner_and_tombstone() {
    let nodes = [service(), service(), service()];
    let owner = Identity::from_secret(&[61; 32]);
    let first = record(&owner, 1);
    let revival = record(&owner, 2);
    let tombstone = Profile {
        previous: Some(dyapp_profile::ProfileCondition::observed(Some(&revival)).unwrap()),
        ..Profile::tombstone(3, 0, 0)
    }
    .sign(&owner);

    for node in &nodes[..2] {
        assert_eq!(offer(node, &first), Status::Ok);
    }
    assert_eq!(offer(&nodes[0], &revival), Status::Ok);
    assert_eq!(offer(&nodes[1], &revival), Status::Ok);
    assert_eq!(offer(&nodes[1], &tombstone), Status::Ok);
    assert_eq!(
        nodes[0]
            .store
            .get_profile(&owner.peer_id())
            .unwrap()
            .unwrap(),
        revival
    );
    assert_eq!(
        nodes[1]
            .store
            .get_profile(&owner.peer_id())
            .unwrap()
            .unwrap(),
        tombstone
    );
    assert!(nodes[2]
        .store
        .get_profile(&owner.peer_id())
        .unwrap()
        .is_none());

    assert_eq!(offer(&nodes[1], &revival), Status::Stale);
    assert_eq!(offer(&nodes[0], &tombstone), Status::Ok);
    for _ in 0..2 {
        for node in &nodes {
            for record in [&tombstone, &first, &revival, &tombstone] {
                assert!(matches!(offer(node, record), Status::Ok | Status::Stale));
            }
        }
    }
    for node in &nodes {
        let stored = node.store.get_profile(&owner.peer_id()).unwrap().unwrap();
        assert_eq!(stored, tombstone);
        assert!(dyapp_profile::verify(&stored).unwrap().profile.deleted);
    }
}
