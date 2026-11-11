//! Real-store conditional repair; protocol steps complete through explicit callbacks.
use super::*;
use libp2p::identity::Keypair;
use std::collections::HashSet;
#[path = "ancestry/failures.rs"]
mod failures;
#[path = "ancestry/recovery.rs"]
mod recovery;

fn peer() -> Peer {
    Peer {
        id: Keypair::ed25519_from_bytes([14; 32])
            .unwrap()
            .public()
            .to_peer_id()
            .to_base58(),
        group: "ancestry".into(),
        coarse: "ancestry".into(),
    }
}

fn signed(source: &Service, owner: &Identity, hashes: Vec<Vec<u8>>) -> SignedRecord {
    let current = source
        .media
        .as_ref()
        .unwrap()
        .keep_snapshot(&owner.peer_id())
        .unwrap();
    let record = owner.sign(
        Domain::MediaConditionalKeep,
        proto::MediaKeepIntent {
            version: current.version + 1,
            hashes,
            time: chrono::Utc::now().timestamp().unsigned_abs(),
            intent_hash: vec![7; 32],
            requester: peer().id,
            expected: Some(proto::MediaKeepCondition {
                owner_present: current.owner_present,
                version: current.version,
                record_hash: current.record.as_ref().map_or_else(Vec::new, |record| {
                    dyapp_identity::sha256(&record.encode_to_vec()).to_vec()
                }),
            }),
            ..Default::default()
        }
        .encode_to_vec(),
    );
    let response = source
        .media(
            &peer(),
            proto::MediaRequest {
                request: Some(proto::media_request::Request::ConditionalKeep(
                    record.clone(),
                )),
            },
        )
        .unwrap();
    assert_eq!(response.status, i32::from(proto::Status::Ok));
    record
}

struct Chain {
    source: Service,
    owner: Identity,
    records: Vec<SignedRecord>,
}
impl Chain {
    fn new(count: usize, hash: &[u8]) -> Self {
        let source = service();
        let owner = Identity::from_secret(&[44; 32]);
        let mut records = Vec::new();
        for version in 1..=count {
            let hash = if version == 1 || version == count {
                hash.to_vec()
            } else {
                dyapp_identity::sha256(&version.to_be_bytes()).to_vec()
            };
            records.push(signed(&source, &owner, vec![hash]));
        }
        Self {
            source,
            owner,
            records,
        }
    }

    fn target(&self) -> &SignedRecord {
        self.records.last().unwrap()
    }
    fn repair(&self, nodes: &[(PeerId, &Service)]) -> Repair {
        let keep = proto::MediaKeepIntent::decode(self.target().payload.as_slice()).unwrap();
        let mut repair = Repair::new(
            &self.source,
            self.target().clone(),
            keep.hashes,
            Instant::now(),
        )
        .unwrap();
        for index in repair.keys(0) {
            repair.holder(0, index, Some(nodes[usize::from(index) % nodes.len()].0));
        }
        repair
    }
}

fn response(service: &Service, request: &proto::MediaRequest) -> proto::MediaResponse {
    match request.request.as_ref().unwrap() {
        proto::media_request::Request::Have(have) => service.media_inventory(&have.items).unwrap(),
        proto::media_request::Request::ReplicaPut(put) => service.store_media(put).unwrap(),
        proto::media_request::Request::ShardPut(put) => service.store_shard(put).unwrap(),
        _ => service.media(&peer(), request.clone()).unwrap(),
    }
}

fn drive_history(
    repair: &mut Repair,
    source: &Service,
    nodes: &[(PeerId, &Service)],
) -> Vec<proto::media_request::Request> {
    drive_with(repair, source, nodes, |target, request| {
        Some(response(target, request))
    })
}

fn drive_with(
    repair: &mut Repair,
    source: &Service,
    nodes: &[(PeerId, &Service)],
    mut dispatch: impl FnMut(&Service, &proto::MediaRequest) -> Option<proto::MediaResponse>,
) -> Vec<proto::media_request::Request> {
    let now = repair.deadline.checked_sub(TIMEOUT).unwrap();
    let mut sent = HashSet::new();
    let mut eof = HashSet::new();
    let mut trace = Vec::new();
    let me: PeerId = peer().id.parse().unwrap();
    loop {
        repair.advance(source, me);
        if repair.done() {
            return trace;
        }
        let queued = repair.queued();
        assert!(
            !queued.is_empty(),
            "repair has no readiness/completion transition"
        );
        for (to, job) in queued {
            let Some((request, reserve)) = repair.request(job, source) else {
                assert!(eof.insert(to), "repeated missing-proof transition");
                repair.response_at(job, None, now);
                continue;
            };
            assert!(
                sent.insert((to, request.encode_to_vec())),
                "repeated protocol step without progress"
            );
            let target = nodes.iter().find(|(id, _)| *id == to).unwrap().1;
            let reply = dispatch(target, &request);
            if let Some(reply) = &reply {
                assert!(reply.encoded_len() as u64 <= reserve);
            }
            trace.push(request.request.unwrap());
            repair.response_at(job, reply, now);
        }
    }
}

fn owned(service: &Service, owner: &Identity, record: &SignedRecord) {
    assert_eq!(
        service
            .media
            .as_ref()
            .unwrap()
            .keep_snapshot(&owner.peer_id())
            .unwrap()
            .record
            .as_ref(),
        Some(record)
    );
}

#[test]
fn skipped_revisions_preserve_existing_photo_bytes_without_obsolete_uploads() {
    let data = b"retained photo".to_vec();
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
    let nodes = [(PeerId::random(), &target)];
    let mut repair = chain.repair(&nodes);
    let trace = drive_history(&mut repair, &chain.source, &nodes);
    assert!(repair.transferred());
    owned(&target, &chain.owner, chain.target());
    assert_eq!(
        target.media.as_ref().unwrap().get(&hash).unwrap(),
        Some(data)
    );
    assert!(!trace.iter().any(|request| matches!(
        request,
        proto::media_request::Request::ReplicaPut(_)
            | proto::media_request::Request::ShardPut(_)
            | proto::media_request::Request::Keep(_)
    )));
}

#[test]
fn reverse_history_is_bounded_and_crosses_the_record_page_limit() {
    let data = b"bounded photo".to_vec();
    let hash = dyapp_identity::sha256(&data).to_vec();
    let chain = Chain::new(260, &hash);
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
    let nodes = [(PeerId::random(), &target)];
    let mut repair = chain.repair(&nodes);
    let trace = drive_history(&mut repair, &chain.source, &nodes);
    let pages = trace
        .iter()
        .filter_map(|request| match request {
            proto::media_request::Request::KeepCatchup(segment) => Some(segment),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(pages.len(), 3);
    assert_eq!(pages[0].ancestors.len(), 0);
    assert_eq!(pages[1].ancestors.len(), 256);
    assert_eq!(pages[2].ancestors.len(), 3);
    for page in pages {
        assert!(page.ancestors.len() <= 256);
        assert!(page.encoded_len() <= 2 * (1 << 20) + 8);
    }
    assert!(repair.transferred());
    owned(&target, &chain.owner, chain.target());
    recovery::resume_after_reopen(&chain, &hash, &data);
}
