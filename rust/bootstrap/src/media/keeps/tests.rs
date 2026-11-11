use super::{KeepSnapshot, KeepUpdate, MediaStore};
use crate::media::Put;
use dyapp_identity::{sha256, Domain, Identity, SignedRecord};
use dyapp_p2p_net::proto::MediaKeep;
use prost::Message;
use std::path::PathBuf;
use std::sync::{mpsc, Arc};
use std::time::Duration;

struct Fixture {
    store: Arc<MediaStore>,
    identity: Identity,
    dir: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../build/media-keeps")
            .join(uuid::Uuid::new_v4().to_string());
        Self {
            store: Arc::new(MediaStore::open(&dir).unwrap()),
            identity: Identity::from_secret(&[37; 32]),
            dir,
        }
    }

    fn request(&self, version: u64, data: &[u8], time: u64) -> (MediaKeep, SignedRecord) {
        let keep = MediaKeep {
            version,
            hashes: vec![sha256(data).to_vec()],
            time,
        };
        let record = self.identity.sign(Domain::MediaKeep, keep.encode_to_vec());
        (keep, record)
    }

    fn initial(&self) -> KeepSnapshot {
        let (keep, record) = self.request(1, b"original", 10);
        let expected = self.store.keep_snapshot(&self.identity.peer_id()).unwrap();
        assert_eq!(
            self.store
                .replace_keep(&self.identity.peer_id(), &expected, &keep, &record)
                .unwrap(),
            KeepUpdate::Applied(keep.hashes)
        );
        assert_eq!(
            self.store
                .put(&self.identity.peer_id(), b"original", 1024)
                .unwrap(),
            Put::Stored
        );
        self.store.keep_snapshot(&self.identity.peer_id()).unwrap()
    }
}

#[test]
fn absent_state_commits_and_replay_returns_exact_current_metadata() {
    let fixture = Fixture::new();
    assert_eq!(
        fixture
            .store
            .keep_snapshot(&fixture.identity.peer_id())
            .unwrap(),
        KeepSnapshot::default()
    );
    let current = fixture.initial();
    let (keep, record) = fixture.request(1, b"unrelated", 11);
    assert_eq!(
        fixture
            .store
            .replace_keep(
                &fixture.identity.peer_id(),
                &KeepSnapshot::default(),
                &keep,
                &record
            )
            .unwrap(),
        KeepUpdate::Conflict(current)
    );
    assert_eq!(
        fixture.store.get(&sha256(b"original")).unwrap(),
        Some(b"original".to_vec())
    );
}

#[test]
fn current_expected_record_applies_exactly_the_next_revision() {
    let fixture = Fixture::new();
    let expected = fixture.initial();
    let (keep, record) = fixture.request(2, b"next", 11);
    assert_eq!(
        fixture
            .store
            .replace_keep(&fixture.identity.peer_id(), &expected, &keep, &record)
            .unwrap(),
        KeepUpdate::Applied(keep.hashes.clone())
    );
    assert_eq!(
        fixture
            .store
            .keep_snapshot(&fixture.identity.peer_id())
            .unwrap(),
        KeepSnapshot {
            owner_present: true,
            version: 2,
            record: Some(record)
        }
    );
    assert_eq!(fixture.store.get(&sha256(b"original")).unwrap(), None);
}

#[test]
fn refreshed_signature_at_the_same_revision_invalidates_an_old_expectation() {
    let fixture = Fixture::new();
    let old = fixture.initial();
    let (refresh, refreshed) = fixture.request(1, b"original", 11);
    assert_eq!(
        fixture
            .store
            .keep_signed(&fixture.identity.peer_id(), &refresh, &refreshed)
            .unwrap(),
        Some(Vec::new())
    );
    let (keep, record) = fixture.request(2, b"obsolete", 12);
    assert_eq!(
        fixture
            .store
            .replace_keep(&fixture.identity.peer_id(), &old, &keep, &record)
            .unwrap(),
        KeepUpdate::Conflict(KeepSnapshot {
            owner_present: true,
            version: 1,
            record: Some(refreshed)
        })
    );
    assert_eq!(
        fixture.store.get(&sha256(b"original")).unwrap(),
        Some(b"original".to_vec())
    );
}

#[test]
fn attachment_only_owner_can_initialize_a_conditional_keep_without_dropping_attachments() {
    let fixture = Fixture::new();
    fixture
        .store
        .attach(
            &fixture.identity.peer_id(),
            &[7; 32],
            &[sha256(b"attached").to_vec()],
            100,
            10,
            4,
        )
        .unwrap();
    assert_eq!(
        fixture
            .store
            .put(&fixture.identity.peer_id(), b"attached", 1024)
            .unwrap(),
        Put::Stored
    );
    fixture.initial();
    assert_eq!(
        fixture.store.get(&sha256(b"attached")).unwrap(),
        Some(b"attached".to_vec())
    );
}

#[test]
fn unsigned_legacy_revision_is_part_of_the_expectation() {
    let fixture = Fixture::new();
    fixture
        .store
        .keep(&fixture.identity.peer_id(), 4, &[], 10)
        .unwrap();
    let current = fixture
        .store
        .keep_snapshot(&fixture.identity.peer_id())
        .unwrap();
    assert_eq!(
        current,
        KeepSnapshot {
            owner_present: true,
            version: 4,
            record: None
        }
    );
    let (keep, record) = fixture.request(5, b"new", 11);
    assert_eq!(
        fixture
            .store
            .replace_keep(
                &fixture.identity.peer_id(),
                &KeepSnapshot::default(),
                &keep,
                &record
            )
            .unwrap(),
        KeepUpdate::Conflict(current.clone())
    );
    assert_eq!(
        fixture
            .store
            .replace_keep(&fixture.identity.peer_id(), &current, &keep, &record)
            .unwrap(),
        KeepUpdate::Applied(keep.hashes)
    );
}

#[test]
fn a_mid_projection_failure_rolls_back_record_references_and_bytes() {
    let fixture = Fixture::new();
    let expected = fixture.initial();
    super::lock(&fixture.store.db).unwrap().execute_batch(
        "CREATE TRIGGER `fail_keep` BEFORE INSERT ON `blobs` BEGIN SELECT RAISE(ABORT, 'injected keep failure'); END;"
    ).unwrap();
    let (keep, record) = fixture.request(2, b"next", 11);
    assert!(fixture
        .store
        .replace_keep(&fixture.identity.peer_id(), &expected, &keep, &record)
        .is_err());
    assert_eq!(
        fixture
            .store
            .keep_snapshot(&fixture.identity.peer_id())
            .unwrap(),
        expected
    );
    assert_eq!(
        fixture.store.get(&sha256(b"original")).unwrap(),
        Some(b"original".to_vec())
    );
}

#[test]
fn writer_admission_precedes_reading_or_changing_the_expected_state() {
    let fixture = Fixture::new();
    let expected = fixture.initial();
    super::lock(&fixture.store.db)
        .unwrap()
        .busy_timeout(Duration::ZERO)
        .unwrap();
    let writer = rusqlite::Connection::open(fixture.dir.join("media.db")).unwrap();
    writer.execute_batch("BEGIN IMMEDIATE").unwrap();
    let (keep, record) = fixture.request(2, b"next", 11);
    assert!(fixture
        .store
        .replace_keep(&fixture.identity.peer_id(), &expected, &keep, &record)
        .is_err());
    writer.execute_batch("ROLLBACK").unwrap();
    assert_eq!(
        fixture
            .store
            .keep_snapshot(&fixture.identity.peer_id())
            .unwrap(),
        expected
    );
    assert_eq!(
        fixture.store.get(&sha256(b"original")).unwrap(),
        Some(b"original".to_vec())
    );
}

#[test]
fn storage_boundaries_and_invalid_projection_leave_current_ownership_unchanged() {
    let fixture = Fixture::new();
    let expected = fixture.initial();
    for version in [0, 1, 3, i64::MAX.cast_unsigned() + 1, u64::MAX] {
        let (keep, record) = fixture.request(version, b"invalid", 11);
        assert!(fixture
            .store
            .replace_keep(&fixture.identity.peer_id(), &expected, &keep, &record)
            .is_err());
    }
    let (mut keep, record) = fixture.request(2, b"next", 11);
    keep.hashes.clear();
    assert!(fixture
        .store
        .replace_keep(&fixture.identity.peer_id(), &expected, &keep, &record)
        .is_err());
    assert_eq!(
        fixture
            .store
            .keep_snapshot(&fixture.identity.peer_id())
            .unwrap(),
        expected
    );
    assert_eq!(
        fixture.store.get(&sha256(b"original")).unwrap(),
        Some(b"original".to_vec())
    );
}

#[test]
fn unsigned_version_zero_does_not_match_absent_ownership() {
    let fixture = Fixture::new();
    fixture
        .store
        .keep(
            &fixture.identity.peer_id(),
            0,
            &[sha256(b"original").to_vec()],
            10,
        )
        .unwrap();
    assert_eq!(
        fixture
            .store
            .put(&fixture.identity.peer_id(), b"original", 1024)
            .unwrap(),
        Put::Stored
    );
    let current = fixture
        .store
        .keep_snapshot(&fixture.identity.peer_id())
        .unwrap();
    assert!(current.owner_present);
    let (keep, record) = fixture.request(1, b"next", 11);
    assert_eq!(
        fixture
            .store
            .replace_keep(
                &fixture.identity.peer_id(),
                &KeepSnapshot::default(),
                &keep,
                &record
            )
            .unwrap(),
        KeepUpdate::Conflict(current)
    );
    assert_eq!(
        fixture.store.get(&sha256(b"original")).unwrap(),
        Some(b"original".to_vec())
    );
}

#[test]
fn the_last_sqlite_revision_is_supported_and_exhaustion_preserves_its_bytes() {
    let fixture = Fixture::new();
    fixture.initial();
    let last = i64::MAX.cast_unsigned();
    let (keep, record) = fixture.request(last - 1, b"original", 20);
    fixture
        .store
        .keep_signed(&fixture.identity.peer_id(), &keep, &record)
        .unwrap();
    let expected = fixture
        .store
        .keep_snapshot(&fixture.identity.peer_id())
        .unwrap();
    let (keep, record) = fixture.request(last, b"original", 21);
    assert_eq!(
        fixture
            .store
            .replace_keep(&fixture.identity.peer_id(), &expected, &keep, &record)
            .unwrap(),
        KeepUpdate::Applied(Vec::new())
    );
    let expected = fixture
        .store
        .keep_snapshot(&fixture.identity.peer_id())
        .unwrap();
    let (keep, record) = fixture.request(last + 1, b"next", 22);
    assert!(fixture
        .store
        .replace_keep(&fixture.identity.peer_id(), &expected, &keep, &record)
        .is_err());
    assert_eq!(
        fixture
            .store
            .keep_snapshot(&fixture.identity.peer_id())
            .unwrap(),
        expected
    );
    assert_eq!(
        fixture.store.get(&sha256(b"original")).unwrap(),
        Some(b"original".to_vec())
    );
}

#[test]
fn malformed_persisted_records_fail_closed_before_any_reference_change() {
    let fixture = Fixture::new();
    let expected = fixture.initial();
    super::lock(&fixture.store.db)
        .unwrap()
        .execute(
            "UPDATE `owners` SET `record` = ? WHERE `owner` = ?",
            rusqlite::params![vec![0xff_u8], fixture.identity.peer_id()],
        )
        .unwrap();
    let (keep, record) = fixture.request(2, b"next", 11);
    assert!(fixture
        .store
        .keep_snapshot(&fixture.identity.peer_id())
        .is_err());
    assert!(fixture
        .store
        .replace_keep(&fixture.identity.peer_id(), &expected, &keep, &record)
        .is_err());
    assert_eq!(
        fixture.store.get(&sha256(b"original")).unwrap(),
        Some(b"original".to_vec())
    );
}

fn prepared_writer(
    fixture: &Fixture,
    expected: &KeepSnapshot,
    data: &[u8],
    ready: mpsc::Sender<()>,
    done: mpsc::Sender<crate::Result<KeepUpdate>>,
) -> (mpsc::Sender<()>, std::thread::JoinHandle<()>) {
    let (keep, record) = fixture.request(2, data, 11);
    let (go, start) = mpsc::channel();
    let (store, expected, owner) = (
        Arc::clone(&fixture.store),
        expected.clone(),
        fixture.identity.peer_id(),
    );
    let worker = std::thread::spawn(move || {
        ready.send(()).unwrap();
        // The coordinator owns phase deadlines and drops the release sender if it fails.
        start.recv().expect("release prepared writer");
        done.send(store.replace_keep(&owner, &expected, &keep, &record))
            .unwrap();
    });
    (go, worker)
}

#[test]
fn concurrent_prepared_replacements_have_one_winner_and_one_exact_conflict() {
    let fixture = Fixture::new();
    let expected = fixture.initial();
    let (ready_send, ready) = mpsc::channel();
    let (done_send, done) = mpsc::channel();
    let (release, workers): (Vec<_>, Vec<_>) = [b"one".as_slice(), b"two".as_slice()]
        .into_iter()
        .map(|data| {
            prepared_writer(
                &fixture,
                &expected,
                data,
                ready_send.clone(),
                done_send.clone(),
            )
        })
        .unzip();
    for _ in 0..2 {
        ready
            .recv_timeout(Duration::from_secs(5))
            .expect("writer ready");
    }
    for go in release {
        go.send(()).unwrap();
    }
    let results: Vec<_> = (0..2)
        .map(|_| {
            done.recv_timeout(Duration::from_secs(5))
                .expect("writer complete")
                .unwrap()
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, KeepUpdate::Applied(_)))
            .count(),
        1
    );
    let current = fixture
        .store
        .keep_snapshot(&fixture.identity.peer_id())
        .unwrap();
    assert!(results.contains(&KeepUpdate::Conflict(current)));
}
