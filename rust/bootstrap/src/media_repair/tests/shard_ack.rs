//! Shard transfer confirmation remains separate from recoverable source data.
use super::*;

fn inventory_stage(repair: &mut Repair, source: &Service, target: &Service, me: PeerId) {
    repair.advance(source, me);
    let queued = repair.queued();
    assert_eq!(queued.len(), 1);
    for (_, job) in queued {
        let request = repair.request(job).unwrap().0.request.unwrap();
        let proto::media_request::Request::Have(have) = request else {
            panic!("expected inventory");
        };
        repair.response(job, Some(target.media_inventory(&have.items).unwrap()));
    }
}

fn write_reply(
    target: &Service,
    request: &proto::media_request::Request,
    refused: bool,
) -> Option<proto::MediaResponse> {
    match request {
        proto::media_request::Request::ShardPut(put) if refused && put.index == 1 => None,
        proto::media_request::Request::ShardPut(put) => Some(target.store_shard(put).unwrap()),
        proto::media_request::Request::ReplicaPut(put) => Some(target.store_media(put).unwrap()),
        _ => panic!("expected transfer write"),
    }
}

fn sharded_handover(refused: bool) {
    let (source, target) = (service(), service());
    let (me, remote) = (PeerId::random(), PeerId::random());
    let (manifest, shards) = media_encode(&vec![7; MEDIA_OBJECT_BYTES + 1]).unwrap();
    let hash = manifest.hash.clone();
    let record = proof(hash.clone());
    let replica = replica(&record, Vec::new(), Some(manifest));
    store_shards(&source, &replica, &shards, 0..shards.len());
    let mut repair = fresh_inventory(&source, &record, &hash, remote);
    inventory_stage(&mut repair, &source, &target, me);
    repair.advance(&source, me);
    let queued = repair.queued();
    assert_eq!(queued.len(), shards.len() + 1);
    for (_, job) in queued {
        let request = repair.request(job).unwrap().0.request.unwrap();
        repair.response(job, write_reply(&target, &request, refused));
    }
    repair.advance(&source, me);
    assert!(repair.done());
    assert_eq!(repair.transferred(), !refused);
    assert_eq!(
        repair.results().collect::<Vec<_>>(),
        vec![(hash.as_slice(), false)]
    );
    assert_eq!(
        target.media.as_ref().unwrap().shard(&hash, 1).unwrap(),
        (!refused).then(|| shards[1].clone())
    );
}

#[test]
fn refused_shard_write_does_not_confirm_a_recoverable_transfer() {
    sharded_handover(true);
}

#[test]
fn acknowledged_manifest_and_shard_writes_confirm_transfer() {
    sharded_handover(false);
}
