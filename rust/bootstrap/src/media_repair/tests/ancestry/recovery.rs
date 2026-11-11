//! Admission preserves newer ownership and restores bytes from independent public sources.
use super::*;

pub(super) fn resume_after_reopen(chain: &Chain, hash: &[u8], data: &[u8]) {
    let mut target = service();
    assert_ok(
        &target
            .store_media(&replica(&chain.records[0], data.to_vec(), None))
            .unwrap(),
    );
    let remote = PeerId::random();
    let nodes = [(remote, &target)];
    let mut repair = chain.repair(&nodes);
    let now = repair.deadline.checked_sub(TIMEOUT).unwrap();
    for _ in 0..2 {
        repair.advance(&chain.source, peer().id.parse().unwrap());
        let jobs = repair.queued();
        assert_eq!(jobs.len(), 1);
        let job = jobs[0].1;
        let request = repair.request(job, &chain.source).unwrap().0;
        let reply = response(&target, &request);
        assert!(!reply.keep_progress.as_ref().unwrap().ready);
        repair.response_at(job, Some(reply), now);
    }
    owned(&target, &chain.owner, &chain.records[0]);
    let path = target.config.storage.dir.clone();
    drop(target.media.take());
    target.media = Some(MediaStore::open(&path).unwrap());
    let nodes = [(remote, &target)];
    let mut resumed = chain.repair(&nodes);
    let trace = drive_history(&mut resumed, &chain.source, &nodes);
    let pages = trace
        .iter()
        .filter_map(|request| match request {
            proto::media_request::Request::KeepCatchup(segment) => Some(segment),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        pages
            .iter()
            .map(|page| page.ancestors.len())
            .collect::<Vec<_>>(),
        vec![0, 3]
    );
    assert!(resumed.transferred());
    owned(&target, &chain.owner, chain.target());
    assert_eq!(
        target.media.as_ref().unwrap().get(hash).unwrap(),
        Some(data.to_vec())
    );
}

#[test]
fn sharded_repair_preserves_present_shards_and_writes_only_the_gap() {
    let (manifest, shards) = media_encode(&vec![9; MEDIA_OBJECT_BYTES + 1]).unwrap();
    let hash = manifest.hash.clone();
    let chain = Chain::new(4, &hash);
    let target = service();
    store_shards(
        &target,
        &replica(&chain.records[0], Vec::new(), Some(manifest.clone())),
        &shards,
        [0],
    );
    store_shards(
        &chain.source,
        &replica(chain.target(), Vec::new(), Some(manifest)),
        &shards,
        0..shards.len(),
    );
    let nodes = [(PeerId::random(), &target)];
    let mut repair = chain.repair(&nodes);
    let trace = drive_history(&mut repair, &chain.source, &nodes);
    assert!(repair.transferred());
    owned(&target, &chain.owner, chain.target());
    let written = trace
        .iter()
        .filter_map(|request| match request {
            proto::media_request::Request::ShardPut(put) => Some(put.index),
            _ => None,
        })
        .collect::<HashSet<_>>();
    assert_eq!(written, (1..u32::try_from(shards.len()).unwrap()).collect());
    for (index, shard) in shards.iter().enumerate() {
        assert_eq!(shard_bytes(&target, &hash, index), *shard);
    }
}

fn superseded_target(chain: &Chain, data: &[u8]) -> (Service, SignedRecord) {
    let target = service();
    let request = proto::MediaRequest {
        request: Some(proto::media_request::Request::KeepCatchup(
            proto::MediaKeepCatchup {
                target: Some(chain.target().clone()),
                ancestors: chain.records[..chain.records.len() - 1]
                    .iter()
                    .rev()
                    .cloned()
                    .collect(),
            },
        )),
    };
    assert_ok(&response(&target, &request));
    let other = proof(dyapp_identity::sha256(data).to_vec());
    assert_ok(
        &target
            .store_media(&replica(&other, data.to_vec(), None))
            .unwrap(),
    );
    let newer = signed(
        &target,
        &chain.owner,
        vec![dyapp_identity::sha256(b"new owner intent").to_vec()],
    );
    (target, newer)
}

#[test]
fn unadmitted_holder_can_supply_public_bytes_but_is_not_overwritten_or_counted() {
    let data = b"public fallback photo".to_vec();
    let hash = dyapp_identity::sha256(&data).to_vec();
    let chain = Chain::new(3, &hash);
    let (target, newer) = superseded_target(&chain, &data);
    let fresh = service();
    let nodes = [(PeerId::random(), &target), (PeerId::random(), &fresh)];
    let mut repair = chain.repair(&nodes);
    let trace = drive_history(&mut repair, &chain.source, &nodes);
    assert!(!repair.transferred());
    assert_eq!(
        repair.results().collect::<Vec<_>>(),
        vec![(hash.as_slice(), false)]
    );
    owned(&target, &chain.owner, &newer);
    owned(&fresh, &chain.owner, chain.target());
    assert_eq!(
        fresh.media.as_ref().unwrap().get(&hash).unwrap(),
        Some(data)
    );
    assert!(trace
        .iter()
        .any(|request| matches!(request, proto::media_request::Request::Get(_))));
}

#[test]
fn seeded_source_without_older_proof_can_fill_an_admitted_empty_holder() {
    let data = b"seeded head photo".to_vec();
    let hash = dyapp_identity::sha256(&data).to_vec();
    let chain = Chain::new(4, &hash);
    let seeded = service();
    assert_ok(
        &seeded
            .store_media(&replica(chain.target(), data.clone(), None))
            .unwrap(),
    );
    let target = service();
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
    drive_history(&mut repair, &seeded, &nodes);
    assert!(repair.transferred());
    owned(&target, &chain.owner, chain.target());
    assert_eq!(
        target.media.as_ref().unwrap().get(&hash).unwrap(),
        Some(data)
    );
}
