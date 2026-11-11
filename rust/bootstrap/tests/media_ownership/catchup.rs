//! A higher signed counter is not evidence of the destination's ownership ancestry.
use super::*;

fn replicate(destination: &Fixture, record: SignedRecord, bytes: &[u8]) -> proto::MediaResponse {
    destination
        .service
        .store_media(&proto::MediaReplicaPut {
            data: bytes.to_vec(),
            authorization: Some(proto::media_replica_put::Authorization::Keep(record)),
            ..Default::default()
        })
        .unwrap()
}

fn applied(fixture: &Fixture, bytes: &[u8]) -> SignedRecord {
    let snapshot = fixture
        .media()
        .keep_snapshot(&fixture.owner.peer_id())
        .unwrap();
    let intent = fixture.write(&snapshot, vec![sha256(bytes).to_vec()]);
    let response = fixture.apply(&intent);
    assert_eq!(response.status, i32::from(Status::Ok));
    assert!(response.keep_applied);
    response.keep_state.unwrap().record.unwrap()
}

#[test]
fn a_skipped_revision_cannot_replace_existing_ownership_without_ancestry() {
    let source = Fixture::new();
    let destination = Fixture::new();
    let first = source.initial();
    let seeded = replicate(&destination, first.clone(), b"original");
    assert_eq!(seeded.status, i32::from(Status::Ok));
    let intermediate = applied(&source, b"intermediate");
    let latest = applied(&source, b"latest");
    let missed = replicate(&destination, latest.clone(), b"latest");
    assert_eq!(missed.status, i32::from(Status::NotFound));
    assert_eq!(
        destination
            .media()
            .keep_of(&destination.owner.peer_id())
            .unwrap(),
        Some(first)
    );
    assert_eq!(
        destination.media().get(&sha256(b"original")).unwrap(),
        Some(b"original".to_vec())
    );
    // Replaying the signed missing step proves the linear ancestry without weakening the guard.
    assert_eq!(
        replicate(&destination, intermediate, b"intermediate").status,
        i32::from(Status::Ok)
    );
    assert_eq!(
        replicate(&destination, latest.clone(), b"latest").status,
        i32::from(Status::Ok)
    );
    assert_eq!(
        destination
            .media()
            .keep_of(&destination.owner.peer_id())
            .unwrap(),
        Some(latest)
    );
}
