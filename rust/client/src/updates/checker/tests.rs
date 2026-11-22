use super::*;
use dyapp_test_support::test_dir;
use rusqlite::{Connection, OpenFlags};

fn cached(version: &str) -> Cache {
    let body = serde_json::json!({"schema":1,"channel":"stable","targets":{
        "android":{"version":version,"tag":format!("v{version}"),"method":"package"}
    }});
    Cache::restore(serde_json::to_vec(&body).unwrap(), Some("\"trusted\"")).unwrap()
}

fn disk(root: &Path) -> Snapshot {
    let db = Connection::open_with_flags(
        root.join("updates.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let bytes: Vec<u8> = db
        .query_row("SELECT `payload` FROM `state` WHERE `id` = 1", [], |row| {
            row.get(0)
        })
        .unwrap();
    Snapshot::decode(&bytes).unwrap()
}

fn check_with(
    root: &Path,
    trigger: Trigger,
    started: u64,
    finished: u64,
    fetch: impl FnOnce(Option<&Cache>) -> Result<Cache, transport::Error>,
) -> Result<Outcome, Error> {
    let mut clock = [started, finished].into_iter();
    run(
        root,
        trigger,
        || Ok(clock.next().expect("only start/completion clocks")),
        fetch,
    )
}

#[test]
fn durable_admission_precedes_fetch_and_completion_precedes_exposure() {
    let dir = test_dir().unwrap();
    let result = check_with(&dir, Trigger::Manual, 100, 110, |cache| {
        assert!(cache.is_none());
        let checkpoint = disk(&dir).checkpoint();
        assert!(checkpoint.in_flight);
        assert_eq!(checkpoint.state.last_attempt, Some(100));
        assert_eq!(checkpoint.state.last_success, None);
        assert!(matches!(
            run(
                &dir,
                Trigger::Manual,
                || panic!("busy clock"),
                |_| panic!("duplicate fetch")
            )
            .unwrap(),
            Outcome::Busy
        ));
        let changed = set_interval(&dir, Interval::Week).unwrap();
        assert_eq!(changed.checkpoint().interval, Interval::Week);
        assert!(changed.checkpoint().in_flight);
        assert_eq!(disk(&dir).checkpoint(), changed.checkpoint());
        assert!(matches!(
            run(
                &dir,
                Trigger::Manual,
                || panic!("busy after interval change"),
                |_| panic!("duplicate fetch")
            )
            .unwrap(),
            Outcome::Busy
        ));
        Ok(cached("2.0.0"))
    })
    .unwrap();
    let Outcome::Validated(snapshot) = result else {
        panic!("expected committed validation")
    };
    assert_eq!(snapshot.encode().unwrap(), disk(&dir).encode().unwrap());
    assert!(!snapshot.checkpoint().in_flight);
    assert_eq!(snapshot.checkpoint().state.last_success, Some(110));
    assert_eq!(snapshot.checkpoint().interval, Interval::Week);
    let changed = set_interval(&dir, Interval::Week).unwrap();
    assert_eq!(changed.checkpoint().interval, Interval::Week);
    assert_eq!(changed.checkpoint().state, snapshot.checkpoint().state);
    assert!(matches!(
        check_with(&dir, Trigger::Timer, 111, 112, |_| panic!("not due")).unwrap(),
        Outcome::NotDue(_)
    ));
}

#[test]
fn rollback_and_transport_failure_keep_prior_cache_and_success() {
    let dir = test_dir().unwrap();
    check_with(&dir, Trigger::Manual, 100, 110, |_| Ok(cached("2.0.0"))).unwrap();
    let prior = disk(&dir).cache().unwrap().bytes().to_vec();
    let outcome = check_with(&dir, Trigger::Manual, 200, 210, |cache| {
        assert_eq!(cache.unwrap().bytes(), prior);
        Ok(cached("1.9.9"))
    })
    .unwrap();
    assert!(matches!(
        outcome,
        Outcome::Failed {
            reason: Failure::Rejected(snapshot::Error::Rollback),
            ..
        }
    ));
    assert_eq!(disk(&dir).cache().unwrap().bytes(), prior);
    assert_eq!(disk(&dir).checkpoint().state.last_success, Some(110));
    assert_eq!(disk(&dir).checkpoint().state.consecutive_failures, 1);
    let outcome = check_with(&dir, Trigger::Manual, 220, 230, |_| {
        let changed = set_interval(&dir, Interval::SixHours).unwrap();
        assert!(changed.checkpoint().in_flight);
        Err(transport::Error::Status(503))
    })
    .unwrap();
    assert!(matches!(
        outcome,
        Outcome::Failed {
            reason: Failure::Transport(_),
            ..
        }
    ));
    assert_eq!(disk(&dir).cache().unwrap().bytes(), prior);
    assert_eq!(disk(&dir).checkpoint().state.consecutive_failures, 2);
    assert_eq!(disk(&dir).checkpoint().interval, Interval::SixHours);
    assert!(matches!(
        check_with(&dir, Trigger::Timer, 240, 241, |_| panic!("backoff")).unwrap(),
        Outcome::NotDue(_)
    ));
}

#[test]
fn admission_uses_the_interval_committed_before_its_transaction() {
    let dir = test_dir().unwrap();
    check_with(&dir, Trigger::Manual, 100, 110, |_| Ok(cached("2.0.0"))).unwrap();
    let at = 110 + Interval::Day.seconds();
    let outcome = run(
        &dir,
        Trigger::Timer,
        || {
            set_interval(&dir, Interval::Week).unwrap();
            Ok(at)
        },
        |_| panic!("lengthened interval must prevent admission"),
    )
    .unwrap();
    assert!(matches!(outcome, Outcome::NotDue(_)));
    let outcome = run(
        &dir,
        Trigger::Timer,
        || {
            set_interval(&dir, Interval::SixHours).unwrap();
            Ok(at)
        },
        |cache| Ok(cache.unwrap().clone()),
    )
    .unwrap();
    assert!(matches!(outcome, Outcome::Validated(_)));
    assert_eq!(disk(&dir).checkpoint().interval, Interval::SixHours);
}

#[test]
fn interrupted_process_fixture() {
    let Some(root) = std::env::var_os("DYAPP_TEST_UPDATE_CRASH_ROOT") else {
        return;
    };
    let mut state = LockedState::try_open(Path::new(&root)).unwrap().unwrap();
    let mut snapshot = state.load().unwrap();
    let mut checkpoint = snapshot.checkpoint();
    checkpoint.in_flight = true;
    checkpoint.state.last_attempt = Some(100);
    snapshot.set_checkpoint(checkpoint);
    state.save(&snapshot).unwrap();
    // Intentionally bypass Drop: the OS must release the process lease itself.
    std::process::exit(0);
}

fn terminate_after_admission(root: &Path) {
    use std::process::{Command, Stdio};
    use std::sync::mpsc;
    use std::time::Duration;

    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "updates::checker::tests::interrupted_process_fixture",
        ])
        .env("DYAPP_TEST_UPDATE_CRASH_ROOT", root)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let (done, completion) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        let result = std::io::copy(&mut stdout, &mut std::io::sink());
        let _ = done.send(result);
    });
    let observed = completion.recv_timeout(Duration::from_secs(10));
    if !matches!(&observed, Ok(Ok(_))) {
        let _ = child.kill();
        let _ = child.wait();
        reader.join().unwrap();
        panic!("child completion signal failed: {observed:?}");
    }
    let status = child.wait().unwrap();
    reader.join().unwrap();
    assert!(status.success(), "child failed: {status}");
}

#[test]
fn interrupted_restore_counts_failure_once_without_network_or_sleep() {
    let dir = test_dir().unwrap();
    terminate_after_admission(&dir);
    assert!(disk(&dir).checkpoint().in_flight);
    for now in [110, 120] {
        assert!(matches!(
            check_with(&dir, Trigger::Timer, now, now, |_| panic!(
                "interrupted backoff"
            ))
            .unwrap(),
            Outcome::NotDue(_)
        ));
        let checkpoint = disk(&dir).checkpoint();
        assert!(!checkpoint.in_flight);
        assert_eq!(checkpoint.state.consecutive_failures, 1);
    }
}

#[test]
fn failed_admission_never_fetches_and_failed_completion_never_exposes_validation() {
    let dir = test_dir().unwrap();
    let state = LockedState::try_open(&dir).unwrap().unwrap();
    let db = Connection::open(dir.join("updates.sqlite")).unwrap();
    db.execute_batch("CREATE TRIGGER `fail_save` BEFORE UPDATE ON `state` BEGIN SELECT RAISE(ABORT, 'controlled write failure'); END;").unwrap();
    drop(state);
    assert!(matches!(
        check_with(&dir, Trigger::Manual, 100, 110, |_| panic!(
            "uncommitted admission"
        ))
        .unwrap_err(),
        Error::Storage(_)
    ));
    assert!(!disk(&dir).checkpoint().in_flight);
    let completion = test_dir().unwrap();
    let result = check_with(&completion, Trigger::Manual, 100, 110, |_| {
        let db = Connection::open(completion.join("updates.sqlite")).unwrap();
        db.execute_batch("CREATE TRIGGER `fail_save` BEFORE UPDATE ON `state` BEGIN SELECT RAISE(ABORT, 'controlled completion failure'); END;").unwrap();
        Ok(cached("2.0.0"))
    });
    assert!(matches!(result.unwrap_err(), Error::Storage(_)));
    let retained = disk(&completion);
    assert!(retained.checkpoint().in_flight);
    assert!(retained.cache().is_none());
    assert_eq!(retained.checkpoint().state.last_success, None);
}

#[test]
fn validated_cached_response_retains_bytes_and_clears_failure_backoff() {
    let dir = test_dir().unwrap();
    check_with(&dir, Trigger::Manual, 100, 110, |_| Ok(cached("2.0.0"))).unwrap();
    check_with(&dir, Trigger::Manual, 200, 210, |_| {
        Err(transport::Error::Status(503))
    })
    .unwrap();
    let prior = disk(&dir).cache().unwrap().bytes().to_vec();
    let outcome = check_with(&dir, Trigger::Manual, 300, 310, |cache| {
        Ok(cache.unwrap().clone())
    })
    .unwrap();
    let Outcome::Validated(snapshot) = outcome else {
        panic!("validated conditional cache")
    };
    assert_eq!(snapshot.cache().unwrap().bytes(), prior);
    assert_eq!(snapshot.checkpoint().state.last_success, Some(310));
    assert_eq!(snapshot.checkpoint().state.consecutive_failures, 0);
    assert_eq!(disk(&dir).encode().unwrap(), snapshot.encode().unwrap());
}

#[test]
fn a_missing_completion_clock_records_failure_without_admitting_the_response() {
    let dir = test_dir().unwrap();
    let mut clock = [Ok(100), Err(Error::Clock)].into_iter();
    let outcome = run(
        &dir,
        Trigger::Manual,
        || clock.next().unwrap(),
        |_| Ok(cached("2.0.0")),
    )
    .unwrap();
    assert!(matches!(
        outcome,
        Outcome::Failed {
            reason: Failure::Clock,
            ..
        }
    ));
    let retained = disk(&dir);
    assert!(!retained.checkpoint().in_flight);
    assert!(retained.cache().is_none());
    assert_eq!(retained.checkpoint().state.consecutive_failures, 1);
}
