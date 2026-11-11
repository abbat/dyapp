//! History and ownership must have one commit point, including quota and database failures.
use super::*;

fn records(fixture: &Fixture, through: u64) -> Vec<SignedRecord> {
    fixture
        .media()
        .keep_history(&fixture.owner.peer_id(), 0, through, 1 << 20)
        .unwrap()
}

fn advance(fixture: &Fixture, bytes: &[u8]) -> SignedRecord {
    let current = fixture
        .media()
        .keep_snapshot(&fixture.owner.peer_id())
        .unwrap();
    let request = fixture.write(&current, vec![sha256(bytes).to_vec()]);
    let response = fixture.apply(&request);
    assert_eq!(response.status, i32::from(Status::Ok));
    response.keep_state.unwrap().record.unwrap()
}

fn wire_bytes(record: &SignedRecord) -> usize {
    let mut bytes = Vec::new();
    prost::encoding::message::encode(1, record, &mut bytes);
    bytes.len()
}

#[test]
fn signed_history_survives_reopening_and_respects_the_frozen_target() {
    let fixture = Fixture::new();
    let first = fixture.initial();
    let second = advance(&fixture, b"second");
    advance(&fixture, b"third");
    assert_eq!(records(&fixture, 2), vec![first.clone(), second.clone()]);
    let reopened = MediaStore::open(&fixture.service.config.storage.dir).unwrap();
    assert_eq!(
        reopened
            .keep_history(&fixture.owner.peer_id(), 0, 2, 1 << 20)
            .unwrap(),
        vec![first, second]
    );
}

#[test]
fn page_budget_uses_actual_protobuf_lengths_and_never_silently_loses_a_record() {
    let fixture = Fixture::new();
    let first = fixture.initial();
    let second = advance(&fixture, b"second");
    let budget = wire_bytes(&first);
    let owner = fixture.owner.peer_id();
    assert_eq!(
        fixture.media().keep_history(&owner, 0, 2, budget).unwrap(),
        vec![first]
    );
    assert_eq!(
        fixture
            .media()
            .keep_history(&owner, 1, 2, wire_bytes(&second))
            .unwrap(),
        vec![second]
    );
    assert!(fixture
        .media()
        .keep_history(&owner, 0, 2, budget - 1)
        .is_err());
    assert!(fixture.media().keep_history(&owner, 2, 1, 1 << 20).is_err());
    assert!(fixture
        .media()
        .keep_history(&owner, 0, 2, (1 << 20) + 5)
        .is_err());
}

#[test]
fn history_budget_failures_roll_back_references_bytes_and_counters() {
    for owner_limit in [true, false] {
        let mut fixture = Fixture::new();
        let original = fixture.initial();
        let before = fixture.media().usage().unwrap().0;
        let current = fixture
            .media()
            .keep_snapshot(&fixture.owner.peer_id())
            .unwrap();
        let keep = fixture.write(&current, Vec::new());
        let limits = &mut Arc::get_mut(&mut fixture.service).unwrap().config.limits;
        if owner_limit {
            limits.media_per_owner_mb = 0;
        } else {
            limits.media_max_mb = 0;
        }
        let response = fixture.apply(&keep);
        assert_eq!(response.status, i32::from(Status::Full));
        assert!(!response.keep_applied);
        assert_eq!(records(&fixture, 2), vec![original.clone()]);
        assert_eq!(
            fixture.media().keep_of(&fixture.owner.peer_id()).unwrap(),
            Some(original)
        );
        assert_eq!(
            fixture.media().get(&sha256(b"original")).unwrap(),
            Some(b"original".to_vec())
        );
        assert_eq!(fixture.media().usage().unwrap().0, before);
    }
}

#[test]
fn replica_history_exhaustion_returns_full_without_creating_ownership() {
    let source = Fixture::new();
    let proof = source.initial();
    let mut destination = Fixture::new();
    Arc::get_mut(&mut destination.service)
        .unwrap()
        .config
        .limits
        .media_per_owner_mb = 0;
    let response = destination
        .service
        .store_media(&proto::MediaReplicaPut {
            data: b"original".to_vec(),
            authorization: Some(proto::media_replica_put::Authorization::Keep(proof)),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(response.status, i32::from(Status::Full));
    assert!(
        !destination
            .media()
            .keep_snapshot(&destination.owner.peer_id())
            .unwrap()
            .owner_present
    );
    assert_eq!(records(&destination, 1), Vec::<SignedRecord>::new());
}

#[test]
fn database_history_failure_rolls_back_the_ownership_projection() {
    let fixture = Fixture::new();
    let original = fixture.initial();
    let db =
        rusqlite::Connection::open(fixture.service.config.storage.dir.join("media.db")).unwrap();
    db.execute_batch(
        "CREATE TRIGGER `fail_history` BEFORE INSERT ON `keep_history` BEGIN SELECT RAISE(ABORT, 'injected history failure'); END;"
    ).unwrap();
    let current = fixture
        .media()
        .keep_snapshot(&fixture.owner.peer_id())
        .unwrap();
    let request = fixture.write(&current, Vec::new());
    let result = fixture.service.media(
        &fixture.peer,
        proto::MediaRequest {
            request: Some(media_request::Request::ConditionalKeep(
                fixture.signed(&request),
            )),
        },
    );
    assert!(result.is_err());
    assert_eq!(
        fixture
            .media()
            .keep_snapshot(&fixture.owner.peer_id())
            .unwrap(),
        current
    );
    assert_eq!(records(&fixture, 2), vec![original]);
    assert_eq!(
        fixture.media().get(&sha256(b"original")).unwrap(),
        Some(b"original".to_vec())
    );
}

#[test]
fn failed_or_conflicting_requests_never_enter_accepted_history() {
    let fixture = Fixture::new();
    let original = fixture.initial();
    let before = fixture
        .media()
        .keep_snapshot(&fixture.owner.peer_id())
        .unwrap();
    let obsolete = fixture.write(&before, Vec::new());
    let second = advance(&fixture, b"newer");
    assert_eq!(fixture.apply(&obsolete).status, i32::from(Status::Stale));
    assert_eq!(records(&fixture, 2), vec![original, second]);
}

#[test]
fn stored_history_is_verified_before_it_can_be_forwarded() {
    let fixture = Fixture::new();
    let mut record = fixture.initial();
    record.signature[0] ^= 1;
    let db =
        rusqlite::Connection::open(fixture.service.config.storage.dir.join("media.db")).unwrap();
    db.execute(
        "UPDATE `keep_history` SET `record` = ? WHERE `owner` = ? AND `version` = ?",
        rusqlite::params![record.encode_to_vec(), fixture.owner.peer_id(), 1],
    )
    .unwrap();
    assert!(fixture
        .media()
        .keep_history(&fixture.owner.peer_id(), 0, 1, 1 << 20)
        .is_err());
}

#[test]
fn owner_expiry_removes_history_before_a_new_epoch_reuses_its_revision() {
    let fixture = Fixture::new();
    fixture.initial();
    assert_eq!(fixture.media().expire_owners(i64::MAX).unwrap(), 1);
    assert_eq!(records(&fixture, 1), Vec::<SignedRecord>::new());
    let fresh = fixture.initial();
    assert_eq!(records(&fixture, 1), vec![fresh]);
}

#[test]
fn node_usage_accounts_for_signed_history_and_releases_it_on_expiry() {
    let fixture = Fixture::new();
    let record = fixture.initial();
    let expected = u64::try_from(record.encoded_len() + b"original".len()).unwrap();
    assert_eq!(fixture.media().usage().unwrap().0, expected);
    fixture.media().expire_owners(i64::MAX).unwrap();
    assert_eq!(fixture.media().usage().unwrap().0, 0);
}

#[test]
fn an_older_development_schema_is_rejected_without_replacing_owner_data() {
    let fixture = Fixture::new();
    let dir = fixture.service.config.storage.dir.join("old-development");
    std::fs::create_dir_all(&dir).unwrap();
    let db = rusqlite::Connection::open(dir.join("media.db")).unwrap();
    db.execute_batch(
        "CREATE TABLE `owners` (`owner` TEXT PRIMARY KEY, `version` INTEGER NOT NULL, `last_seen` INTEGER NOT NULL, `record` BLOB);"
    ).unwrap();
    db.execute(
        "INSERT INTO `owners` (`owner`, `version`, `last_seen`) VALUES (?, ?, ?)",
        rusqlite::params!["prior", 7, 1],
    )
    .unwrap();
    assert!(MediaStore::open(&dir).is_err());
    let version: i64 = db
        .query_row(
            "SELECT `version` FROM `owners` WHERE `owner` = ?",
            ["prior"],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(version, 7);
}
