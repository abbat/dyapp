//! Random retries, reopen and rollback preserve the conditional-ownership reference model.
use super::*;
use dyapp_bootstrap::media::KeepHistoryProgress;
use proptest::prelude::*;

#[derive(Clone, Copy, Debug)]
enum Op {
    Stage,
    Retry,
    Apply,
    Restart,
    FailStage,
    FailApply,
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        3 => Just(Op::Stage),
        2 => Just(Op::Retry),
        2 => Just(Op::Apply),
        1 => Just(Op::Restart),
        1 => Just(Op::FailStage),
        1 => Just(Op::FailApply),
    ]
}

#[derive(Default)]
struct Model {
    next: Option<u64>,
    applied: bool,
}

impl Model {
    fn new() -> Self {
        Self {
            next: Some(3),
            applied: false,
        }
    }
}

fn audit(fixture: &Fixture) -> Result<(), TestCaseError> {
    let db = rusqlite::Connection::open(fixture.service.config.storage.dir.join("media.db"))?;
    let sum = |sql: &str| db.query_row(sql, [], |row| row.get::<_, i64>(0));
    let pending = sum("SELECT COALESCE(SUM(`bytes`), 0) FROM `keep_pending`")?;
    let pending_rows =
        sum("SELECT COALESCE(SUM(length(`record`)), 0) FROM `keep_pending_records`")?;
    let history = sum("SELECT COALESCE(SUM(`history_bytes`), 0) FROM `owners`")?;
    let history_rows = sum("SELECT COALESCE(SUM(length(`record`)), 0) FROM `keep_history`")?;
    prop_assert_eq!(pending, pending_rows, "staged proof accounting");
    prop_assert_eq!(history, history_rows, "accepted history accounting");

    let mut statement =
        db.prepare("SELECT DISTINCT `hash`, `size` FROM `blobs` WHERE `size` IS NOT NULL")?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)?))
    })?;
    let mut blobs = 0;
    for row in rows {
        let (hash, size) = row?;
        let name = dyapp_identity::hex(&hash);
        let path = fixture
            .service
            .config
            .storage
            .dir
            .join("data")
            .join(&name[..2])
            .join(&name[2..4])
            .join(name);
        prop_assert_eq!(i64::try_from(std::fs::metadata(path)?.len()).unwrap(), size);
        blobs += size;
    }
    prop_assert_eq!(
        fixture.media().usage().unwrap().0,
        (pending + history + blobs).unsigned_abs(),
        "node media accounting"
    );
    Ok(())
}

fn fail_stage(fixture: &Fixture, records: &[SignedRecord], next: u64) {
    let db =
        rusqlite::Connection::open(fixture.service.config.storage.dir.join("media.db")).unwrap();
    db.execute_batch(
        "CREATE TRIGGER `fail_model_stage` BEFORE INSERT ON `keep_pending_records`
         BEGIN SELECT RAISE(ABORT, 'injected model stage failure'); END;",
    )
    .unwrap();
    assert!(fixture
        .media()
        .stage_keep_history(
            &records[3],
            &[records[usize::try_from(next).unwrap() - 1].clone()],
            fresh(),
            &fixture.service.config.limits,
        )
        .is_err());
    db.execute_batch("DROP TRIGGER `fail_model_stage`").unwrap();
}

fn fail_apply(fixture: &Fixture, records: &[SignedRecord]) {
    let db =
        rusqlite::Connection::open(fixture.service.config.storage.dir.join("media.db")).unwrap();
    db.execute_batch(
        "CREATE TRIGGER `fail_model_apply` BEFORE INSERT ON `keep_history`
         BEGIN SELECT RAISE(ABORT, 'injected model apply failure'); END;",
    )
    .unwrap();
    assert!(fixture
        .media()
        .apply_keep_history(&records[3], fresh(), &fixture.service.config.limits)
        .is_err());
    db.execute_batch("DROP TRIGGER `fail_model_apply`").unwrap();
}

fn apply(
    fixture: &Fixture,
    records: &[SignedRecord],
    model: &mut Model,
) -> Result<(), TestCaseError> {
    let result = fixture
        .media()
        .apply_keep_history(&records[3], fresh(), &fixture.service.config.limits)
        .unwrap();
    if let Some(next) = model.next.filter(|next| *next <= 1) {
        prop_assert_eq!(result, KeepHistoryProgress::Ready { next_version: next });
        model.applied = true;
        model.next = None;
    } else {
        prop_assert_eq!(
            result,
            KeepHistoryProgress::Pending {
                next_version: model.next.unwrap_or(3)
            }
        );
    }
    Ok(())
}

fn check_model(
    fixture: &Fixture,
    model: &Model,
    records: &[SignedRecord],
) -> Result<(), TestCaseError> {
    let current = fixture
        .media()
        .keep_snapshot(&fixture.owner.peer_id())
        .unwrap();
    prop_assert_eq!(
        current.record.as_ref(),
        Some(if model.applied {
            &records[3]
        } else {
            &records[0]
        })
    );
    prop_assert_eq!(
        fixture
            .media()
            .keep_history(&fixture.owner.peer_id(), 0, 4, 1 << 20)
            .unwrap(),
        if model.applied {
            records.to_vec()
        } else {
            vec![records[0].clone()]
        }
    );
    Ok(())
}

fn inject_failure(op: Op, fixture: &Fixture, records: &[SignedRecord], model: &Model) {
    match op {
        Op::FailStage => {
            if let Some(next) = model.next.filter(|next| *next > 1) {
                fail_stage(fixture, records, next);
            }
        }
        Op::FailApply => {
            if model.next.is_some_and(|next| next <= 1) {
                fail_apply(fixture, records);
            }
        }
        _ => unreachable!(),
    }
}

fn check(schedule: &[Op]) -> Result<(), TestCaseError> {
    let (_, records) = chain();
    let mut destination = seeded(&records[0]);
    let mut model = Model::new();
    for op in schedule {
        if model.applied && !matches!(op, Op::Restart) {
            audit(&destination)?;
            continue;
        }
        match *op {
            Op::Stage => {
                if let Some(next) = model.next.filter(|next| *next > 0) {
                    prop_assert_eq!(
                        stage(
                            &destination,
                            &records[3],
                            &[records[usize::try_from(next).unwrap() - 1].clone()]
                        ),
                        Some(next - 1)
                    );
                    model.next = Some(next - 1);
                }
            }
            Op::Retry => {
                if let Some(next) = model.next.filter(|next| *next < 3) {
                    let version = next + 1;
                    prop_assert_eq!(
                        stage(
                            &destination,
                            &records[3],
                            &[records[usize::try_from(version).unwrap() - 1].clone()]
                        ),
                        Some(next)
                    );
                }
            }
            Op::Apply => apply(&destination, &records, &mut model)?,
            Op::Restart => destination.reopen_media(),
            Op::FailStage | Op::FailApply => {
                inject_failure(*op, &destination, &records, &model);
            }
        }
        check_model(&destination, &model, &records)?;
        audit(&destination)?;
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 32,
        failure_persistence: Some(Box::new(
            proptest::test_runner::FileFailurePersistence::Direct(
                "build/dyapp-bootstrap-media-model-regressions.txt"
            )
        )),
        ..ProptestConfig::default()
    })]

    #[test]
    fn conditional_ancestry_reopen_and_rollback_match_model(
        schedule in prop::collection::vec(op(), 1..30),
    ) {
        check(&schedule)?;
    }
}
