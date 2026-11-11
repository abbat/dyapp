//! Old, conflicting and malformed admission replies never authorize writes.
use super::*;

fn fixture() -> (Chain, Service, Vec<u8>, Vec<u8>) {
    let data = b"guarded photo".to_vec();
    let hash = dyapp_identity::sha256(&data).to_vec();
    let chain = Chain::new(4, &hash);
    let target = service();
    assert_ok(
        &target
            .store_media(&replica(&chain.records[0], data.clone(), None))
            .unwrap(),
    );
    assert_ok(
        &chain
            .source
            .store_media(&replica(chain.target(), data.clone(), None))
            .unwrap(),
    );
    (chain, target, data, hash)
}

fn no_write(trace: &[proto::media_request::Request]) {
    assert!(!trace.iter().any(|request| matches!(
        request,
        proto::media_request::Request::ReplicaPut(_)
            | proto::media_request::Request::ShardPut(_)
            | proto::media_request::Request::Keep(_)
    )));
}

fn unsupported_replies() -> [Option<proto::MediaResponse>; 4] {
    let ok = proto::MediaResponse {
        status: proto::Status::Ok.into(),
        ..Default::default()
    };
    [
        None,
        Some(proto::MediaResponse {
            status: proto::Status::Unsupported.into(),
            ..Default::default()
        }),
        Some(proto::MediaResponse {
            keep_progress: Some(proto::MediaKeepProgress {
                ready: true,
                next_version: 3,
                target_current: None,
            }),
            ..ok.clone()
        }),
        Some(proto::MediaResponse {
            keep_progress: Some(proto::MediaKeepProgress {
                ready: true,
                next_version: 3,
                target_current: Some(false),
            }),
            ..ok
        }),
    ]
}

#[test]
fn unsupported_or_missing_current_receipts_fail_closed() {
    for reply in unsupported_replies() {
        let (chain, target, data, hash) = fixture();
        let nodes = [(PeerId::random(), &target)];
        let mut repair = chain.repair(&nodes);
        let trace = drive_with(&mut repair, &chain.source, &nodes, |service, request| {
            if matches!(
                request.request,
                Some(proto::media_request::Request::KeepCatchup(_))
            ) {
                reply.clone()
            } else {
                Some(response(service, request))
            }
        });
        assert!(!repair.transferred());
        assert_eq!(
            repair.results().collect::<Vec<_>>(),
            vec![(hash.as_slice(), false)]
        );
        owned(&target, &chain.owner, &chain.records[0]);
        assert_eq!(
            target.media.as_ref().unwrap().get(&hash).unwrap(),
            Some(data)
        );
        no_write(&trace);
    }
}

#[test]
fn nondecreasing_page_progress_does_not_reset_its_deadline() {
    let (chain, target, _, _) = fixture();
    let nodes = [(PeerId::random(), &target)];
    let mut repair = chain.repair(&nodes);
    let before = repair.deadline;
    let (job, request) = step(&mut repair, &chain.source);
    repair.response_at(
        job,
        Some(response(&target, &request)),
        before.checked_sub(TIMEOUT).unwrap(),
    );
    let (job, request) = step(&mut repair, &chain.source);
    assert!(
        matches!(request.request, Some(proto::media_request::Request::KeepCatchup(ref segment)) if !segment.ancestors.is_empty())
    );
    let reply = proto::MediaResponse {
        status: proto::Status::Ok.into(),
        keep_progress: Some(proto::MediaKeepProgress {
            ready: false,
            next_version: 3,
            target_current: None,
        }),
        ..Default::default()
    };
    repair.response_at(job, Some(reply), before.checked_sub(TIMEOUT / 2).unwrap());
    assert_eq!(repair.deadline, before);
    let trace = drive_history(&mut repair, &chain.source, &nodes);
    assert!(!repair.transferred());
    owned(&target, &chain.owner, &chain.records[0]);
    no_write(&trace);
}

#[test]
fn malformed_probe_progress_never_authorizes_a_destination() {
    for (ready, next_version, target_current) in [
        (false, 0, None),
        (false, 3, Some(true)),
        (true, 4, Some(true)),
    ] {
        let (chain, target, _, _) = fixture();
        let nodes = [(PeerId::random(), &target)];
        let mut repair = chain.repair(&nodes);
        let before = repair.deadline;
        let (job, _) = step(&mut repair, &chain.source);
        repair.response_at(
            job,
            Some(proto::MediaResponse {
                status: proto::Status::Ok.into(),
                keep_progress: Some(proto::MediaKeepProgress {
                    ready,
                    next_version,
                    target_current,
                }),
                ..Default::default()
            }),
            before.checked_sub(TIMEOUT / 2).unwrap(),
        );
        assert_eq!(repair.deadline, before);
        let trace = drive_history(&mut repair, &chain.source, &nodes);
        assert!(!repair.transferred());
        owned(&target, &chain.owner, &chain.records[0]);
        no_write(&trace);
    }
}

#[test]
fn invalid_conditional_signature_cannot_downgrade_to_legacy() {
    let (chain, _, _, hash) = fixture();
    let mut record = chain.target().clone();
    record.payload.push(0);
    assert!(Repair::new(&chain.source, record, vec![hash], Instant::now()).is_err());
}

fn step(repair: &mut Repair, source: &Service) -> (usize, proto::MediaRequest) {
    repair.advance(source, peer().id.parse().unwrap());
    let jobs = repair.queued();
    assert_eq!(jobs.len(), 1);
    let job = jobs[0].1;
    (job, repair.request(job, source).unwrap().0)
}

#[test]
fn verified_progress_extends_deadline_but_expired_progress_cannot_revive_it() {
    let (chain, target, _, _) = fixture();
    let nodes = [(PeerId::random(), &target)];
    let mut repair = chain.repair(&nodes);
    let old = repair.deadline;
    let (job, request) = step(&mut repair, &chain.source);
    let at = old.checked_sub(TIMEOUT / 2).unwrap();
    repair.response_at(job, Some(response(&target, &request)), at);
    assert_eq!(repair.deadline, at + TIMEOUT);
    let (job, request) = step(&mut repair, &chain.source);
    let expired = repair.deadline;
    repair.response_at(job, Some(response(&target, &request)), expired);
    assert_eq!(repair.deadline, expired);
    assert!(repair.unconfirmed);
    assert!(!repair.transferred());
}

#[test]
fn missing_required_proofs_finish_without_waiting_or_pruning() {
    let (chain, target, data, hash) = fixture();
    let seeded = service();
    assert_ok(
        &seeded
            .store_media(&replica(chain.target(), data.clone(), None))
            .unwrap(),
    );
    let nodes = [(PeerId::random(), &target)];
    let mut repair = Repair::new(
        &seeded,
        chain.target().clone(),
        vec![hash.clone()],
        Instant::now(),
    )
    .unwrap();
    for index in repair.keys(0) {
        repair.holder(0, index, Some(nodes[0].0));
    }
    let trace = drive_history(&mut repair, &seeded, &nodes);
    assert!(repair.done());
    assert!(!repair.transferred());
    owned(&target, &chain.owner, &chain.records[0]);
    assert_eq!(
        target.media.as_ref().unwrap().get(&hash).unwrap(),
        Some(data)
    );
    no_write(&trace);
}
