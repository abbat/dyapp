//! Replica catch-up uses bounded proof pages and discloses no current owner snapshot.
use super::*;

fn call(
    destination: &Fixture,
    target: Option<SignedRecord>,
    ancestors: Vec<SignedRecord>,
) -> proto::MediaResponse {
    let peer = Peer {
        id: Keypair::ed25519_from_bytes([55; 32])
            .unwrap()
            .public()
            .to_peer_id()
            .to_base58(),
        group: "relay".into(),
        coarse: "relay".into(),
    };
    destination.call(
        &peer,
        media_request::Request::KeepCatchup(proto::MediaKeepCatchup { target, ancestors }),
    )
}

#[test]
fn old_ancestors_are_inert_proofs_for_a_fresh_target() {
    let destination = Fixture::new();
    let mut first = destination.write(&KeepSnapshot::default(), vec![]);
    first.time = 0;
    let ancestor = destination.signed(&first);
    let state = KeepSnapshot {
        owner_present: true,
        version: 1,
        record: Some(ancestor.clone()),
    };
    let target = destination.signed(&destination.write(&state, vec![]));
    let response = call(&destination, Some(target), vec![ancestor]);
    assert_eq!(response.status, i32::from(Status::Ok));
    assert_eq!(
        response.keep_progress,
        Some(proto::MediaKeepProgress {
            ready: true,
            next_version: 0,
            target_current: Some(true),
        })
    );
    private(&response);
}

#[test]
fn expired_targets_and_future_ancestors_cannot_change_ownership() {
    let destination = Fixture::new();
    let mut first = destination.write(&KeepSnapshot::default(), vec![]);
    first.time = 0;
    let response = call(&destination, Some(destination.signed(&first)), vec![]);
    assert_eq!(response.status, i32::from(Status::Invalid));
    assert!(response.server_time > 0);
    private(&response);
    first.time = now() + 86400;
    let ancestor = destination.signed(&first);
    let state = KeepSnapshot {
        owner_present: true,
        version: 1,
        record: Some(ancestor.clone()),
    };
    let target = destination.signed(&destination.write(&state, vec![]));
    let response = call(&destination, Some(target), vec![ancestor]);
    assert_eq!(response.status, i32::from(Status::Invalid));
    private(&response);
    assert_eq!(
        destination
            .media()
            .keep_snapshot(&destination.owner.peer_id())
            .unwrap(),
        KeepSnapshot::default()
    );
    assert_eq!(destination.media().usage().unwrap().0, 0);
}

#[test]
fn quota_exhaustion_returns_full_without_partial_live_or_pending_state() {
    let (_, records) = chain();
    let mut destination = Fixture::new();
    Arc::get_mut(&mut destination.service)
        .unwrap()
        .config
        .limits
        .media_per_owner_mb = 0;
    let response = call(&destination, Some(records[3].clone()), vec![]);
    assert_eq!(response.status, i32::from(Status::Full));
    private(&response);
    assert_eq!(response.keep_progress, None);
    assert_eq!(destination.media().usage().unwrap().0, 0);
}

#[test]
fn unknown_request_variants_fail_closed_as_unsupported() {
    let destination = Fixture::new();
    let unknown = proto::MediaRequest::decode([0xfa, 0x07, 0].as_slice()).unwrap();
    assert_eq!(unknown.request, None);
    let response = destination
        .service
        .media(&destination.peer, unknown)
        .unwrap();
    assert_eq!(response.status, i32::from(Status::Unsupported));
    private(&response);
    assert_eq!(response.keep_progress, None);
}

fn accepted(response: &proto::MediaResponse, ready: bool, next_version: u64) {
    assert_eq!(response.status, i32::from(Status::Ok));
    assert_eq!(
        response.keep_progress,
        Some(proto::MediaKeepProgress {
            ready,
            next_version,
            target_current: ready.then_some(true),
        })
    );
    private(response);
}

#[test]
fn accepted_ancestor_does_not_claim_current_target_admission() {
    let (source, records) = chain();
    let before = source
        .media()
        .keep_snapshot(&source.owner.peer_id())
        .unwrap();
    assert_eq!(before.version, 4);
    let response = call(&source, Some(records[2].clone()), vec![]);
    assert_eq!(response.status, i32::from(Status::Ok));
    assert_eq!(
        response.keep_progress,
        Some(proto::MediaKeepProgress {
            ready: true,
            next_version: 2,
            target_current: Some(false),
        })
    );
    private(&response);
    assert_eq!(
        source
            .media()
            .keep_snapshot(&source.owner.peer_id())
            .unwrap(),
        before
    );
    let decoded = proto::MediaResponse::decode(response.encode_to_vec().as_slice()).unwrap();
    assert_eq!(decoded.keep_progress.unwrap().target_current, Some(false));
}

#[test]
fn old_ready_reply_does_not_establish_current_target_admission() {
    let old = proto::MediaKeepProgress::decode([0x08, 0x01].as_slice()).unwrap();
    assert_eq!((old.ready, old.target_current), (true, None));
}

#[test]
fn denied_blob_metadata_does_not_block_other_authorized_photos() {
    let source = Fixture::new();
    let first = source.initial();
    let before = source
        .media()
        .keep_snapshot(&source.owner.peer_id())
        .unwrap();
    let blocked = sha256(b"blocked photo").to_vec();
    let hashes = vec![sha256(b"original").to_vec(), blocked.clone()];
    let record = source
        .apply(&source.write(&before, hashes))
        .keep_state
        .unwrap()
        .record
        .unwrap();
    let destination = seeded(&first);
    destination
        .service
        .deny
        .write()
        .unwrap()
        .insert(dyapp_identity::hex(&blocked));
    let response = call(&destination, Some(record.clone()), vec![first]);
    accepted(&response, true, 0);
    for (data, status) in [
        (b"original".to_vec(), Status::Ok),
        (b"blocked photo".to_vec(), Status::Refused),
    ] {
        assert_blob_policy(&destination, &record, data, status);
    }
    let response = destination.call(
        &destination.peer,
        media_request::Request::Get(proto::GetMedia {
            hash: blocked,
            ..Default::default()
        }),
    );
    assert_eq!(response.status, i32::from(Status::Refused));
    destination
        .service
        .deny
        .write()
        .unwrap()
        .insert(destination.owner.peer_id());
    let response = call(&destination, Some(record), vec![]);
    assert_eq!(response.status, i32::from(Status::Refused));
    assert_eq!(response.keep_progress, None);
    private(&response);
}

fn assert_blob_policy(destination: &Fixture, record: &SignedRecord, data: Vec<u8>, status: Status) {
    let response = destination
        .service
        .store_media(&proto::MediaReplicaPut {
            data,
            authorization: Some(proto::media_replica_put::Authorization::Keep(
                record.clone(),
            )),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(response.status, i32::from(status));
}

fn private(response: &proto::MediaResponse) {
    assert_eq!(response.keep_state, None);
    assert!(!response.keep_applied);
    assert_eq!(response.missing, Vec::<Vec<u8>>::new());
}

#[test]
fn node_relay_resumes_pages_without_intermediate_blob_data_or_origin_receipts() {
    let (_, records) = chain();
    let destination = seeded(&records[0]);
    let first = call(
        &destination,
        Some(records[3].clone()),
        vec![records[2].clone()],
    );
    accepted(&first, false, 2);
    assert_eq!(
        destination
            .media()
            .keep_snapshot(&destination.owner.peer_id())
            .unwrap()
            .version,
        1
    );
    let next = call(
        &destination,
        Some(records[3].clone()),
        vec![records[1].clone()],
    );
    accepted(&next, true, 1);
    assert_eq!(
        destination.media().get(&sha256(b"original")).unwrap(),
        Some(b"original".to_vec())
    );
    assert_eq!(
        destination
            .media()
            .keep_snapshot(&destination.owner.peer_id())
            .unwrap()
            .record,
        Some(records[3].clone())
    );
    let used = destination.media().usage().unwrap().0;
    assert_eq!(
        call(
            &destination,
            Some(records[3].clone()),
            vec![records[2].clone(), records[1].clone()]
        ),
        next
    );
    assert_eq!(destination.media().usage().unwrap().0, used);
}

#[test]
fn node_admission_precedes_even_targetless_catchup() {
    let mut destination = Fixture::new();
    Arc::get_mut(&mut destination.service)
        .unwrap()
        .config
        .network
        .id_pow_bits = 255;
    let response = call(&destination, None, vec![]);
    assert_eq!(response.status, i32::from(Status::Denied));
    assert_eq!(response.keep_progress, None);
    private(&response);
    assert_eq!(destination.media().usage().unwrap().0, 0);
}

#[test]
fn oversize_segments_are_rejected_before_signatures_or_live_mutation() {
    let (_, records) = chain();
    let destination = seeded(&records[0]);
    let before = destination
        .media()
        .keep_snapshot(&destination.owner.peer_id())
        .unwrap();
    let oversized = SignedRecord {
        payload: vec![0; (1 << 20) + 1],
        ..records[0].clone()
    };
    let large_part = SignedRecord {
        payload: vec![0; 700_000],
        ..records[0].clone()
    };
    for (target, ancestors) in [
        (records[3].clone(), vec![records[2].clone(); 257]),
        (oversized, vec![]),
        (records[3].clone(), vec![large_part.clone(), large_part]),
    ] {
        let response = call(&destination, Some(target), ancestors);
        assert_eq!(response.status, i32::from(Status::TooLarge));
        assert!(response.max_bytes > 0);
        assert_eq!(response.keep_progress, None);
        private(&response);
        assert_eq!(
            destination
                .media()
                .keep_snapshot(&destination.owner.peer_id())
                .unwrap(),
            before
        );
    }
}

#[test]
fn malformed_proofs_and_targets_return_no_private_state() {
    let (_, records) = chain();
    let destination = seeded(&records[0]);
    let before = destination
        .media()
        .keep_snapshot(&destination.owner.peer_id())
        .unwrap();
    let mut forged = records[3].clone();
    forged.signature[0] ^= 1;
    let wrong_domain = destination
        .owner
        .sign(Domain::MediaKeep, records[3].payload.clone());
    let other_owner = Identity::from_secret(&[22; 32])
        .sign(Domain::MediaConditionalKeep, records[2].payload.clone());
    for (target, ancestors, status) in [
        (None, vec![], Status::Invalid),
        (Some(forged), vec![], Status::Denied),
        (Some(wrong_domain), vec![], Status::Denied),
        (Some(records[3].clone()), vec![other_owner], Status::Invalid),
        (
            Some(records[3].clone()),
            vec![records[1].clone()],
            Status::Invalid,
        ),
    ] {
        let response = call(&destination, target, ancestors);
        assert_eq!(response.status, i32::from(status));
        assert_eq!(response.keep_progress, None);
        private(&response);
        assert_eq!(
            destination
                .media()
                .keep_snapshot(&destination.owner.peer_id())
                .unwrap(),
            before
        );
    }
}

#[test]
fn conflicting_live_branch_returns_stale_without_its_snapshot() {
    let (_, records) = chain();
    let destination = seeded(&records[0]);
    let before = destination
        .media()
        .keep_snapshot(&destination.owner.peer_id())
        .unwrap();
    let competing = destination.write(&before, vec![sha256(b"competing").to_vec()]);
    assert_eq!(destination.apply(&competing).status, i32::from(Status::Ok));
    let live = destination
        .media()
        .keep_snapshot(&destination.owner.peer_id())
        .unwrap();
    let response = call(
        &destination,
        Some(records[3].clone()),
        vec![records[2].clone(), records[1].clone()],
    );
    assert_eq!(response.status, i32::from(Status::Stale));
    assert_eq!(response.keep_progress, None);
    private(&response);
    assert_eq!(
        destination
            .media()
            .keep_snapshot(&destination.owner.peer_id())
            .unwrap(),
        live
    );
}
