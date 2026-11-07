use super::*;
mod adversarial;
use crate::store::sticker_packs::{exchange::response::PackResponseJob, uploads::UploadPlan};

fn pending() -> (Invite, std::path::PathBuf, Pack, PackResponseJob) {
    let (mut pair, path) = pair();
    let pack = sticker_packs::tests::pack("Cats");
    pair.session.store.save_sticker_pack(&SEAL, &pack).unwrap();
    pair.session
        .stage_sticker(pair.group.id(), &sticker(&pack), &SEAL)
        .unwrap();
    deliver(
        &mut pair,
        &Body::PackRequest(PackRequest {
            pack_id: *pack.id(),
        }),
        3,
    );
    let job = pair
        .session
        .store
        .due_sticker_pack_responses(crate::now() * 1000)
        .unwrap()
        .remove(0);
    (pair, path, pack, job)
}
fn confirmed(pair: &mut Invite, pack: &Pack) -> crate::attachment::Attachment {
    let (mut candidate, _) = crate::attachment::seal(&pack.encode()).unwrap();
    candidate.mime = "application/octet-stream".into();
    let pending = match pair
        .session
        .store
        .plan_sticker_pack_upload(&candidate, crate::now())
        .unwrap()
    {
        UploadPlan::Upload(pending) => pending,
        _ => panic!("fresh pack fixture"),
    };
    pair.session
        .store
        .finish_sticker_pack_upload(&pending, crate::now())
        .unwrap();
    candidate
}

#[tokio::test]
async fn upload_failure_keeps_job_and_common_outbox_with_dynamic_retry_deadline() {
    let (mut pair, path, _, job) = pending();
    let count = pair.session.due_outbound(i64::MAX).unwrap().len();
    let sealed = pair.session.store.sealed_device().unwrap();
    assert_eq!(
        pair.session
            .respond_sticker_pack_requests(&mut [], &pair.bob_id, &SEAL, || false)
            .await
            .unwrap(),
        0
    );
    assert_eq!(pair.session.due_outbound(i64::MAX).unwrap().len(), count);
    assert!(pair.session.store.sealed_device().unwrap() == sealed);
    let db = rusqlite::Connection::open(path).unwrap();
    let deadline: i64 = db.query_row(
        "SELECT `retry_at_ms` FROM `sticker_pack_responses` WHERE `peer_id` = ?1 AND `pack_id` = ?2",
        rusqlite::params![&job.peer, job.pack_id.as_slice()], |row| row.get(0)).unwrap();
    assert!(pair
        .session
        .store
        .due_sticker_pack_responses((deadline - 1) as u64)
        .unwrap()
        .is_empty());
    assert!(
        pair.session
            .store
            .due_sticker_pack_responses(deadline as u64)
            .unwrap()
            == vec![job]
    );
}

#[tokio::test]
async fn confirmed_blob_reuse_stages_response_and_mailbox_failure_uses_common_retry() {
    let (mut pair, _, pack, _) = pending();
    let descriptor = confirmed(&mut pair, &pack);
    assert!(descriptor.release_secret != [0; 32]);
    let count = pair.session.due_outbound(i64::MAX).unwrap().len();
    assert_eq!(
        pair.session
            .respond_sticker_pack_requests(&mut [], &pair.bob_id, &SEAL, || false)
            .await
            .unwrap(),
        1
    );
    assert!(pair.session.due_outbound(i64::MAX).unwrap().len() > count);
    assert!(pair
        .session
        .store
        .due_sticker_pack_responses(crate::now() * 1000)
        .unwrap()
        .is_empty());
    assert!(
        matches!(pair.session.store.plan_sticker_pack_upload(&descriptor, crate::now()).unwrap(),
        UploadPlan::Reuse(ref retained) if *retained == descriptor)
    );
}
