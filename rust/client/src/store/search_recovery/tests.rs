use super::*;
use crate::store::tests::temp_db_path;
use dyapp_identity::SignedRecord;
use dyapp_profile::ProfileCondition;
use prost::Message as _;
use std::{path::PathBuf, time::Duration};

mod proof_tests;

type Rows = Vec<(String, i64, Vec<u8>)>;
fn rows(store: &Store) -> Rows {
    store
        .db
        .prepare("SELECT `key`, `format_version`, `value` FROM `settings` ORDER BY `key`")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}
fn sealed(store: &Store) -> (Vec<u8>, i64) {
    store
        .db
        .query_row(
            "SELECT `sealed`, `revision` FROM `mls_state` WHERE `slot` = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap()
}

struct Fixture {
    store: Store,
    owner: Identity,
    path: PathBuf,
    legacy: SignedRecord,
}
impl Fixture {
    fn new(acked: bool) -> Self {
        let path = temp_db_path();
        let owner = Identity::from_secret(&[7; 32]);
        let mut store = Store::open(&path, &owner).unwrap();
        // Opaque sealed device bytes must never be opened or rewritten by search recovery.
        store
            .stage_outbound(b"original sealed device", &[])
            .unwrap();
        let source = Profile {
            version: 3,
            ..Default::default()
        }
        .sign(&owner);
        store.search_profile_sent(&source, 1000).unwrap();
        let draft = Profile {
            version: 4,
            name: "private draft".into(),
            about: "keep this text".into(),
            photos: vec![dyapp_profile::Photo {
                full: vec![vec![5; 32]],
                thumbnail: vec![6; 32],
            }],
            ..Default::default()
        };
        store.save_profile_draft(&draft).unwrap();
        store.set_profile_hidden(true).unwrap();
        store.note_device_listed(3).unwrap();
        store.own_profile_age(7, 1000).unwrap();
        let legacy = Profile::tombstone(4, 0, 0).sign(&owner);
        store.hold_search_profile(&legacy).unwrap();
        if acked {
            store.search_profile_sent(&legacy, 1001).unwrap();
            let mut bytes = 4u64.to_be_bytes().to_vec();
            bytes.extend(sha256(&legacy.encode_to_vec()));
            store
                .db
                .execute(
                    "UPDATE `settings` SET `format_version` = ?1, `value` = ?2 WHERE `key` = ?3",
                    params![1, bytes, "search_profile_head"],
                )
                .unwrap();
        }
        store
            .search_profile_rate_limited(1000, Duration::from_secs(120))
            .unwrap();
        Self {
            store,
            owner,
            path,
            legacy,
        }
    }
    fn plan(&self) -> SearchWithdrawalRecovery {
        self.store
            .prepare_search_withdrawal_recovery()
            .unwrap()
            .unwrap()
    }
}

#[test]
fn pending_and_acknowledged_recovery_preserve_private_state_and_retry_idempotently() {
    for acked in [false, true] {
        let f = Fixture::new(acked);
        let before = rows(&f.store);
        let credential = sealed(&f.store);
        let private = f.store.link_state().unwrap();
        let mut expected = f.store.profile_draft().unwrap().unwrap();
        let plan = f.plan();
        assert_eq!(rows(&f.store), before);
        expected.version = 8;
        assert_eq!(
            f.store
                .confirm_search_withdrawal_recovery(&f.owner, &plan)
                .unwrap(),
            expected
        );
        assert_eq!(f.store.due_search_profile(1119).unwrap(), None);
        let record = f.store.due_search_profile(1120).unwrap().unwrap();
        assert!(dyapp_profile::verify(&record).is_err());
        assert!(
            dyapp_profile::verify_search(&record)
                .unwrap()
                .profile
                .search_only
        );
        check_retries(&f, &plan, &record, &expected);
        assert_eq!(f.store.link_state().unwrap(), private);
        assert_eq!(sealed(&f.store), credential);
        assert_eq!(f.store.device_listed().unwrap(), 3);
        assert_eq!(f.store.own_profile_seen().unwrap(), 7);
    }
}

fn check_retries(
    f: &Fixture,
    plan: &SearchWithdrawalRecovery,
    record: &SignedRecord,
    expected: &Profile,
) {
    f.store.search_profile_sent(&f.legacy, 1002).unwrap();
    assert_eq!(
        f.store.due_search_profile(1120).unwrap(),
        Some(record.clone())
    );
    assert_eq!(
        f.store
            .confirm_search_withdrawal_recovery(&f.owner, plan)
            .unwrap(),
        *expected
    );
    f.store.search_profile_sent(record, 1120).unwrap();
    let accepted = rows(&f.store);
    assert_eq!(
        f.store
            .confirm_search_withdrawal_recovery(&f.owner, plan)
            .unwrap(),
        *expected
    );
    assert_eq!(rows(&f.store), accepted);
    assert!(f
        .store
        .prepare_search_withdrawal_recovery()
        .unwrap()
        .is_none());
}

#[derive(Clone, Copy)]
enum Change {
    Draft,
    Head,
    Ack,
    Backoff,
    Visibility,
    Seen,
    Listed,
    PendingLink,
    LinkContext,
}
fn change(store: &Store, owner: &Identity, legacy: &SignedRecord, change: Change) {
    match change {
        Change::Draft => {
            let mut draft = store.profile_draft().unwrap().unwrap();
            draft.name = "newer independent edit".into();
            store.save_profile_draft(&draft).unwrap();
        }
        Change::Head => store
            .hold_search_profile(&Profile::search_tombstone(9, 0, 0).sign(owner))
            .unwrap(),
        Change::Ack => store.search_profile_sent(legacy, 1003).unwrap(),
        Change::Backoff => store
            .search_profile_rate_limited(1000, Duration::from_secs(300))
            .unwrap(),
        Change::Visibility => store.set_profile_hidden(false).unwrap(),
        Change::Seen => {
            store.own_profile_age(9, 1001).unwrap();
        }
        Change::Listed => store.note_device_listed(9).unwrap(),
        Change::PendingLink => store
            .set_link_pending(b"replacement accepted link")
            .unwrap(),
        Change::LinkContext => {
            store
                .db
                .execute(
                    "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, 1, ?2)",
                    params!["link_enrollment", sha256(b"new completed enrollment")],
                )
                .unwrap();
        }
    }
}

#[test]
fn independent_changes_refuse_stale_owner_confirmation_without_overwriting_any_row() {
    for change_kind in [
        Change::Draft,
        Change::Head,
        Change::Ack,
        Change::Backoff,
        Change::Visibility,
        Change::Seen,
        Change::Listed,
        Change::PendingLink,
        Change::LinkContext,
    ] {
        let f = Fixture::new(false);
        let plan = f.plan();
        let other = Store::open_existing(&f.path, &f.owner).unwrap();
        change(&other, &f.owner, &f.legacy, change_kind);
        let expected = rows(&other);
        assert!(matches!(
            f.store.confirm_search_withdrawal_recovery(&f.owner, &plan),
            Err(Error::SearchWithdrawalRecoveryChanged)
        ));
        assert_eq!(rows(&other), expected);
    }
}

#[test]
fn wrong_owner_or_another_database_cannot_apply_the_review() {
    let f = Fixture::new(false);
    let plan = f.plan();
    let before = rows(&f.store);
    assert!(matches!(
        f.store
            .confirm_search_withdrawal_recovery(&Identity::from_secret(&[8; 32]), &plan),
        Err(Error::WrongIdentity)
    ));
    assert_eq!(rows(&f.store), before);
    let other = Fixture::new(false);
    let before = rows(&other.store);
    assert!(matches!(
        other
            .store
            .confirm_search_withdrawal_recovery(&other.owner, &plan),
        Err(Error::SearchWithdrawalRecoveryChanged)
    ));
    assert_eq!(rows(&other.store), before);
}

#[test]
fn final_head_write_failure_rolls_back_draft_queue_and_private_state() {
    let f = Fixture::new(false);
    let plan = f.plan();
    let before = rows(&f.store);
    let credential = sealed(&f.store);
    f.store
        .db
        .execute_batch(
            "CREATE TEMP TRIGGER `refuse_recovery_head` BEFORE INSERT ON `settings`
        WHEN NEW.`key` = 'search_profile_head' BEGIN SELECT RAISE(ABORT, 'head refused'); END;",
        )
        .unwrap();
    assert!(f
        .store
        .confirm_search_withdrawal_recovery(&f.owner, &plan)
        .is_err());
    assert_eq!(rows(&f.store), before);
    assert_eq!(sealed(&f.store), credential);
}

#[test]
fn exhaustion_and_pending_link_refuse_preparation_without_mutation() {
    let f = Fixture::new(false);
    let mut draft = f.store.profile_draft().unwrap().unwrap();
    draft.version = u64::MAX;
    f.store.save_profile_draft(&draft).unwrap();
    let before = rows(&f.store);
    assert!(matches!(
        f.store.prepare_search_withdrawal_recovery(),
        Err(Error::ProfileVersionExhausted)
    ));
    assert_eq!(rows(&f.store), before);
    let f = Fixture::new(false);
    f.store.set_link_pending(b"unfinished link").unwrap();
    let before = rows(&f.store);
    assert!(matches!(
        f.store.prepare_search_withdrawal_recovery(),
        Err(Error::LinkStateReviewRequired)
    ));
    assert_eq!(rows(&f.store), before);
}

#[test]
fn a_visible_ambiguous_queue_requires_review_instead_of_hiding_the_profile() {
    let f = Fixture::new(false);
    f.store.set_profile_hidden(false).unwrap();
    let before = rows(&f.store);
    assert!(matches!(
        f.store.prepare_search_withdrawal_recovery(),
        Err(Error::SearchWithdrawalReviewRequired)
    ));
    assert_eq!(rows(&f.store), before);
}

#[test]
fn a_committed_review_cannot_be_reused_after_unhide_or_link_replacement() {
    for change_kind in [Change::Visibility, Change::PendingLink, Change::LinkContext] {
        let f = Fixture::new(false);
        let plan = f.plan();
        f.store
            .confirm_search_withdrawal_recovery(&f.owner, &plan)
            .unwrap();
        change(&f.store, &f.owner, &f.legacy, change_kind);
        let before = rows(&f.store);
        assert!(matches!(
            f.store.confirm_search_withdrawal_recovery(&f.owner, &plan),
            Err(Error::SearchWithdrawalRecoveryChanged)
        ));
        assert_eq!(rows(&f.store), before);
    }
}

#[test]
fn missing_draft_requires_explicit_recovery_without_creating_one() {
    let owner = Identity::from_secret(&[7; 32]);
    let store = Store::open(&temp_db_path(), &owner).unwrap();
    store.set_profile_hidden(true).unwrap();
    store
        .hold_search_profile(&Profile::tombstone(2, 0, 0).sign(&owner))
        .unwrap();
    let before = rows(&store);
    assert!(matches!(
        store.prepare_search_withdrawal_recovery(),
        Err(Error::SearchWithdrawalReviewRequired)
    ));
    assert_eq!(rows(&store), before);
}

#[test]
fn writer_admission_precedes_validation_and_committed_replacement_stays_authoritative() {
    let f = Fixture::new(false);
    let plan = f.plan();
    f.store.db.busy_timeout(Duration::ZERO).unwrap();
    let other = Store::open_existing(&f.path, &f.owner).unwrap();
    let transaction =
        Transaction::new_unchecked(&other.db, TransactionBehavior::Immediate).unwrap();
    other
        .hold_search_profile_in_tx(&Profile::search_tombstone(9, 0, 0).sign(&f.owner))
        .unwrap();
    assert!(
        matches!(f.store.confirm_search_withdrawal_recovery(&f.owner, &plan),
        Err(Error::Sql(rusqlite::Error::SqliteFailure(error, _))) if error.code == rusqlite::ErrorCode::DatabaseBusy)
    );
    transaction.commit().unwrap();
    let before = rows(&other);
    assert!(matches!(
        f.store.confirm_search_withdrawal_recovery(&f.owner, &plan),
        Err(Error::SearchWithdrawalRecoveryChanged)
    ));
    assert_eq!(rows(&other), before);
}

#[test]
fn ordinary_deletion_and_known_search_withdrawal_are_never_reinterpreted() {
    for profile in [
        Profile {
            previous: Some(ProfileCondition::default()),
            ..Profile::tombstone(8, 0, 0)
        },
        Profile::search_tombstone(8, 0, 0),
    ] {
        let f = Fixture::new(false);
        f.store
            .hold_search_profile(&profile.sign(&f.owner))
            .unwrap();
        let before = rows(&f.store);
        assert!(f
            .store
            .prepare_search_withdrawal_recovery()
            .unwrap()
            .is_none());
        assert_eq!(rows(&f.store), before);
    }
}
