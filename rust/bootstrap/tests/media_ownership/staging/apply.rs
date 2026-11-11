//! Finalization failures preserve both live ownership and staged recovery state.
use super::*;
use dyapp_bootstrap::media::KeepHistoryProgress;

#[test]
fn expiry_is_rechecked_at_application_without_waiting() {
    let (_, records) = chain();
    let destination = seeded(&records[0]);
    complete(&destination, &records);
    let before = destination
        .media()
        .keep_snapshot(&destination.owner.peer_id())
        .unwrap();
    let head = proto::MediaKeepIntent::decode(records[3].payload.as_slice()).unwrap();
    let result = destination.media().apply_keep_history(
        &records[3],
        head.time + 1..=head.time + 2,
        &destination.service.config.limits,
    );
    assert!(matches!(
        result,
        Err(dyapp_bootstrap::BootstrapError::InvalidRequest(_))
    ));
    assert_eq!(
        destination
            .media()
            .keep_snapshot(&destination.owner.peer_id())
            .unwrap(),
        before
    );
    assert_eq!(stage(&destination, &records[3], &[]), Some(1));
}

#[test]
fn finalizing_an_old_target_preserves_the_newer_session() {
    let (_, records) = chain();
    let destination = seeded(&records[0]);
    stage(&destination, &records[3], &[records[2].clone()]);
    let used = destination.media().usage().unwrap().0;
    assert_eq!(
        apply(&destination, &records[2]),
        KeepHistoryProgress::Conflict
    );
    assert_eq!(stage(&destination, &records[3], &[]), Some(2));
    assert_eq!(destination.media().usage().unwrap().0, used);
}

#[test]
fn empty_keep_namespace_can_seed_the_authenticated_target() {
    let (_, records) = chain();
    let destination = Fixture::new();
    stage(&destination, &records[3], &[]);
    assert_eq!(
        apply(&destination, &records[3]),
        KeepHistoryProgress::Ready { next_version: 3 }
    );
    assert_eq!(
        destination
            .media()
            .keep_snapshot(&destination.owner.peer_id())
            .unwrap()
            .record,
        Some(records[3].clone())
    );
}

fn complete(destination: &Fixture, records: &[SignedRecord]) {
    assert_eq!(
        stage(
            destination,
            &records[3],
            &[records[2].clone(), records[1].clone()]
        ),
        Some(1)
    );
}

#[test]
fn failed_archive_rolls_back_projection_before_any_file_removal() {
    let (_, records) = chain();
    let destination = seeded(&records[0]);
    complete(&destination, &records);
    let before = destination
        .media()
        .keep_snapshot(&destination.owner.peer_id())
        .unwrap();
    let used = destination.media().usage().unwrap().0;
    let db = rusqlite::Connection::open(destination.service.config.storage.dir.join("media.db"))
        .unwrap();
    db.execute_batch("CREATE TRIGGER `fail_archive` BEFORE INSERT ON `keep_history` WHEN NEW.`version` = 3 BEGIN SELECT RAISE(ABORT, 'injected archive failure'); END;").unwrap();
    assert!(destination
        .media()
        .apply_keep_history(&records[3], fresh(), &destination.service.config.limits)
        .is_err());
    assert_eq!(
        destination
            .media()
            .keep_snapshot(&destination.owner.peer_id())
            .unwrap(),
        before
    );
    assert_eq!(destination.media().usage().unwrap().0, used);
    assert_eq!(
        destination.media().get(&sha256(b"original")).unwrap(),
        Some(b"original".to_vec())
    );
    assert_eq!(stage(&destination, &records[3], &[]), Some(1));
}

#[test]
fn changed_staged_signature_cannot_be_archived_or_prune_live_data() {
    let (_, records) = chain();
    let destination = seeded(&records[0]);
    complete(&destination, &records);
    let before = destination
        .media()
        .keep_snapshot(&destination.owner.peer_id())
        .unwrap();
    let mut corrupt = records[2].clone();
    corrupt.signature[0] ^= 1;
    let db = rusqlite::Connection::open(destination.service.config.storage.dir.join("media.db"))
        .unwrap();
    db.execute(
        "UPDATE `keep_pending_records` SET `record` = ? WHERE `version` = 3",
        [corrupt.encode_to_vec()],
    )
    .unwrap();
    assert!(destination
        .media()
        .apply_keep_history(&records[3], fresh(), &destination.service.config.limits)
        .is_err());
    assert_eq!(
        destination
            .media()
            .keep_snapshot(&destination.owner.peer_id())
            .unwrap(),
        before
    );
    assert_eq!(
        destination.media().get(&sha256(b"original")).unwrap(),
        Some(b"original".to_vec())
    );
}

#[test]
fn final_budget_failure_preserves_the_session_and_live_head() {
    let (_, records) = chain();
    let destination = seeded(&records[0]);
    complete(&destination, &records);
    let before = destination
        .media()
        .keep_snapshot(&destination.owner.peer_id())
        .unwrap();
    let used = destination.media().usage().unwrap().0;
    for owner_limit in [true, false] {
        let mut limits = destination.service.config.limits.clone();
        if owner_limit {
            limits.media_per_owner_mb = 0;
        } else {
            limits.media_max_mb = 0;
        }
        assert!(matches!(
            destination
                .media()
                .apply_keep_history(&records[3], fresh(), &limits),
            Err(dyapp_bootstrap::BootstrapError::MediaHistoryFull)
        ));
        assert_eq!(
            destination
                .media()
                .keep_snapshot(&destination.owner.peer_id())
                .unwrap(),
            before
        );
        assert_eq!(destination.media().usage().unwrap().0, used);
    }
    assert_eq!(
        apply(&destination, &records[3]),
        KeepHistoryProgress::Ready { next_version: 1 }
    );
}

#[test]
fn a_competing_live_branch_is_never_overwritten() {
    let (_, records) = chain();
    let destination = seeded(&records[0]);
    complete(&destination, &records);
    let current = destination
        .media()
        .keep_snapshot(&destination.owner.peer_id())
        .unwrap();
    let competing = destination.write(&current, vec![sha256(b"competing").to_vec()]);
    assert_eq!(destination.apply(&competing).status, i32::from(Status::Ok));
    let before = destination
        .media()
        .keep_snapshot(&destination.owner.peer_id())
        .unwrap();
    assert_eq!(
        apply(&destination, &records[3]),
        KeepHistoryProgress::Conflict
    );
    assert_eq!(
        destination
            .media()
            .keep_snapshot(&destination.owner.peer_id())
            .unwrap(),
        before
    );
}

#[test]
fn accepted_head_backfill_changes_neither_live_record_nor_retention() {
    let (_, records) = chain();
    let destination = seeded(&records[3]);
    let owner = destination.owner.peer_id();
    let before = destination.media().keep_snapshot(&owner).unwrap();
    let db = rusqlite::Connection::open(destination.service.config.storage.dir.join("media.db"))
        .unwrap();
    let seen = || {
        db.query_row(
            "SELECT `last_seen` FROM `owners` WHERE `owner` = ?",
            [&owner],
            |row| row.get::<_, i64>(0),
        )
        .unwrap()
    };
    let time = seen();
    complete(&destination, &records);
    assert_eq!(
        apply(&destination, &records[3]),
        KeepHistoryProgress::Ready { next_version: 1 }
    );
    assert_eq!(destination.media().keep_snapshot(&owner).unwrap(), before);
    assert_eq!(seen(), time);
    stage(&destination, &records[3], &[records[0].clone()]);
    assert_eq!(
        apply(&destination, &records[3]),
        KeepHistoryProgress::Ready { next_version: 0 }
    );
    assert_eq!(
        destination
            .media()
            .keep_history(&owner, 0, 4, 1 << 20)
            .unwrap(),
        records
    );
    assert_eq!(seen(), time);
}
