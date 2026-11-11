//! Incomplete reverse proofs remain invisible to live ownership and accepted ancestry.
use super::*;
use std::ops::RangeInclusive;
#[path = "staging/apply.rs"]
mod application;

fn apply(
    destination: &Fixture,
    target: &SignedRecord,
) -> dyapp_bootstrap::media::KeepHistoryProgress {
    destination
        .media()
        .apply_keep_history(target, fresh(), &destination.service.config.limits)
        .unwrap()
}

#[test]
fn final_projection_preserves_bytes_needed_again_after_skipped_revisions() {
    use dyapp_bootstrap::media::KeepHistoryProgress::{Pending, Ready};
    let (_, records) = chain();
    let destination = seeded(&records[0]);
    let owner = destination.owner.peer_id();
    stage(&destination, &records[3], &[records[2].clone()]);
    assert_eq!(
        apply(&destination, &records[3]),
        Pending { next_version: 2 }
    );
    assert_eq!(
        destination.media().keep_snapshot(&owner).unwrap().version,
        1
    );
    stage(&destination, &records[3], &[records[1].clone()]);
    let used = destination.media().usage().unwrap().0;
    let reopened = MediaStore::open(&destination.service.config.storage.dir).unwrap();
    assert_eq!(
        reopened
            .apply_keep_history(&records[3], fresh(), &destination.service.config.limits)
            .unwrap(),
        Ready { next_version: 1 }
    );
    assert_eq!(
        reopened.keep_snapshot(&owner).unwrap().record,
        Some(records[3].clone())
    );
    assert_eq!(
        reopened.get(&sha256(b"original")).unwrap(),
        Some(b"original".to_vec())
    );
    assert_eq!(reopened.usage().unwrap().0, used);
    assert_eq!(apply(&destination, &records[3]), Ready { next_version: 1 });
    assert_eq!(destination.media().usage().unwrap().0, used);
    assert_eq!(
        destination
            .media()
            .keep_history(&owner, 0, 4, 1 << 20)
            .unwrap(),
        records
    );
}

fn fresh() -> RangeInclusive<u64> {
    let time = now();
    time.saturating_sub(30 * 86400)..=time.saturating_add(600)
}

fn chain() -> (Fixture, Vec<SignedRecord>) {
    let source = Fixture::new();
    let mut records = vec![source.initial()];
    for version in 2u64..=4 {
        let current = source
            .media()
            .keep_snapshot(&source.owner.peer_id())
            .unwrap();
        let hash = if version == 4 {
            sha256(b"original")
        } else {
            sha256(&version.to_be_bytes())
        };
        let intent = source.write(&current, vec![hash.to_vec()]);
        let response = source.apply(&intent);
        assert_eq!(response.status, i32::from(Status::Ok));
        records.push(response.keep_state.unwrap().record.unwrap());
    }
    (source, records)
}

fn seeded(record: &SignedRecord) -> Fixture {
    let destination = Fixture::new();
    let response = destination
        .service
        .store_media(&proto::MediaReplicaPut {
            data: b"original".to_vec(),
            authorization: Some(proto::media_replica_put::Authorization::Keep(
                record.clone(),
            )),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(response.status, i32::from(Status::Ok));
    destination
}

fn stage(destination: &Fixture, target: &SignedRecord, ancestors: &[SignedRecord]) -> Option<u64> {
    destination
        .media()
        .stage_keep_history(
            target,
            ancestors,
            fresh(),
            &destination.service.config.limits,
        )
        .unwrap()
}

#[test]
fn staged_pages_never_replace_live_references_or_accepted_history() {
    let (_, records) = chain();
    let destination = seeded(&records[0]);
    let owner = destination.owner.peer_id();
    let before = destination.media().keep_snapshot(&owner).unwrap();
    let used = destination.media().usage().unwrap().0;
    assert_eq!(
        stage(&destination, &records[3], &[records[2].clone()]),
        Some(2)
    );
    assert_eq!(
        stage(&destination, &records[3], &[records[1].clone()]),
        Some(1)
    );
    assert_eq!(
        stage(&destination, &records[3], &[records[0].clone()]),
        Some(0)
    );
    assert_eq!(destination.media().keep_snapshot(&owner).unwrap(), before);
    assert_eq!(
        destination.media().get(&sha256(b"original")).unwrap(),
        Some(b"original".to_vec())
    );
    assert_eq!(
        destination
            .media()
            .keep_history(&owner, 0, 4, 1 << 20)
            .unwrap(),
        vec![records[0].clone()]
    );
    assert!(destination.media().usage().unwrap().0 > used);
}

#[test]
fn provisional_ownerless_data_does_not_create_a_live_owner_row() {
    let (_, records) = chain();
    let destination = Fixture::new();
    assert_eq!(stage(&destination, &records[3], &[]), Some(3));
    assert!(
        !destination
            .media()
            .keep_snapshot(&destination.owner.peer_id())
            .unwrap()
            .owner_present
    );
    assert_eq!(
        destination
            .media()
            .keep_history(&destination.owner.peer_id(), 0, 4, 1 << 20)
            .unwrap(),
        Vec::<SignedRecord>::new()
    );
    assert!(destination.media().usage().unwrap().0 > 0);
}

#[test]
fn reopening_and_overlapping_retries_preserve_the_cursor_without_double_charging() {
    let (_, records) = chain();
    let destination = seeded(&records[0]);
    assert_eq!(
        stage(&destination, &records[3], &[records[2].clone()]),
        Some(2)
    );
    let used = destination.media().usage().unwrap().0;
    let reopened = MediaStore::open(&destination.service.config.storage.dir).unwrap();
    assert_eq!(
        reopened
            .stage_keep_history(
                &records[3],
                &[records[2].clone()],
                fresh(),
                &destination.service.config.limits
            )
            .unwrap(),
        Some(2)
    );
    assert_eq!(reopened.usage().unwrap().0, used);
    assert_eq!(
        reopened
            .stage_keep_history(
                &records[3],
                &[records[2].clone(), records[1].clone()],
                fresh(),
                &destination.service.config.limits
            )
            .unwrap(),
        Some(1)
    );
    assert_eq!(
        destination.media().get(&sha256(b"original")).unwrap(),
        Some(b"original".to_vec())
    );
}

#[test]
fn invalid_links_foreign_signatures_and_stale_targets_leave_the_session_unchanged() {
    let (source, records) = chain();
    let destination = seeded(&records[0]);
    assert_eq!(
        stage(&destination, &records[3], &[records[2].clone()]),
        Some(2)
    );
    let used = destination.media().usage().unwrap().0;
    let foreign = Identity::from_secret(&[18; 32])
        .sign(Domain::MediaConditionalKeep, records[1].payload.clone());
    assert!(destination
        .media()
        .stage_keep_history(
            &records[3],
            &[foreign],
            fresh(),
            &destination.service.config.limits
        )
        .is_err());
    assert!(destination
        .media()
        .stage_keep_history(
            &records[3],
            &[records[0].clone()],
            fresh(),
            &destination.service.config.limits
        )
        .is_err());
    let mut old = proto::MediaKeepIntent::decode(records[3].payload.as_slice()).unwrap();
    old.time = fresh().start().saturating_sub(1);
    let old = source.signed(&old);
    assert!(destination
        .media()
        .stage_keep_history(&old, &[], fresh(), &destination.service.config.limits)
        .is_err());
    assert_eq!(stage(&destination, &records[3], &[]), Some(2));
    assert_eq!(destination.media().usage().unwrap().0, used);
}

#[test]
fn budget_failure_restores_the_previous_target_and_cursor() {
    let (_, records) = chain();
    let destination = seeded(&records[0]);
    assert_eq!(
        stage(&destination, &records[2], &[records[1].clone()]),
        Some(1)
    );
    let used = destination.media().usage().unwrap().0;
    for owner_limit in [true, false] {
        let mut limits = NodeConfig::default().limits;
        if owner_limit {
            limits.media_per_owner_mb = 0;
        } else {
            limits.media_max_mb = 0;
        }
        let result = destination
            .media()
            .stage_keep_history(&records[3], &[], fresh(), &limits);
        assert!(matches!(
            result,
            Err(dyapp_bootstrap::BootstrapError::MediaHistoryFull)
        ));
        assert_eq!(stage(&destination, &records[2], &[]), Some(1));
        assert_eq!(destination.media().usage().unwrap().0, used);
    }
}

#[test]
fn older_targets_cannot_reset_a_newer_incomplete_session() {
    let (_, records) = chain();
    let destination = seeded(&records[0]);
    assert_eq!(
        stage(&destination, &records[3], &[records[2].clone()]),
        Some(2)
    );
    let used = destination.media().usage().unwrap().0;
    assert_eq!(
        stage(&destination, &records[2], &[records[1].clone()]),
        None
    );
    assert_eq!(stage(&destination, &records[3], &[]), Some(2));
    assert_eq!(destination.media().usage().unwrap().0, used);
}

#[test]
fn pending_expiry_also_cleans_sessions_without_live_owner_rows() {
    let (_, records) = chain();
    let destination = Fixture::new();
    stage(&destination, &records[3], &[records[2].clone()]);
    assert_eq!(destination.media().expire_owners(i64::MAX).unwrap(), 0);
    assert_eq!(destination.media().usage().unwrap().0, 0);
    assert_eq!(stage(&destination, &records[3], &[]), Some(3));
}

#[test]
fn a_database_failure_cannot_leave_a_partial_new_session() {
    let (_, records) = chain();
    let destination = Fixture::new();
    let db = rusqlite::Connection::open(destination.service.config.storage.dir.join("media.db"))
        .unwrap();
    db.execute_batch("CREATE TRIGGER `fail_pending` BEFORE INSERT ON `keep_pending_records` WHEN NEW.`version` = 2 BEGIN SELECT RAISE(ABORT, 'injected staging failure'); END;").unwrap();
    let result = destination.media().stage_keep_history(
        &records[3],
        &[records[2].clone(), records[1].clone()],
        fresh(),
        &destination.service.config.limits,
    );
    assert!(result.is_err());
    assert_eq!(destination.media().usage().unwrap().0, 0);
    let count: i64 = db
        .query_row("SELECT COUNT(*) FROM `keep_pending`", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn pressure_evicts_provisional_records_before_live_photos() {
    let (_, records) = chain();
    let destination = seeded(&records[0]);
    let owner = destination.owner.peer_id();
    let before = destination.media().keep_snapshot(&owner).unwrap();
    let used = destination.media().usage().unwrap().0;
    stage(&destination, &records[3], &[records[2].clone()]);
    assert_eq!(destination.media().evict(used + 1).unwrap(), 1);
    assert_eq!(destination.media().usage().unwrap().0, used);
    assert_eq!(destination.media().keep_snapshot(&owner).unwrap(), before);
    assert_eq!(
        destination.media().get(&sha256(b"original")).unwrap(),
        Some(b"original".to_vec())
    );
    assert_eq!(stage(&destination, &records[3], &[]), Some(3));
}

#[test]
fn pressure_can_reclaim_proofs_without_a_live_owner_row() {
    let (_, records) = chain();
    let destination = Fixture::new();
    stage(&destination, &records[3], &[records[2].clone()]);
    assert_eq!(destination.media().evict(1).unwrap(), 1);
    assert_eq!(destination.media().usage().unwrap().0, 0);
    assert!(
        !destination
            .media()
            .keep_snapshot(&destination.owner.peer_id())
            .unwrap()
            .owner_present
    );
}
