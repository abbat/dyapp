use super::*;
use crate::store::tests::temp_db_path;
use dyapp_identity::Domain;
use dyapp_profile::Profile;
use std::path::Path;

fn fixture() -> (Identity, Store, SignedRecord, ProfilePublication) {
    let identity = Identity::from_secret(&[7; 32]);
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    store.set_link_pending(b"original").unwrap();
    let certificate = identity.sign(Domain::Profile, vec![1]);
    let mut attempt = store.prepare_profile_publication(&identity).unwrap();
    attempt.directory_from(&Profile {
        version: 1,
        ..Default::default()
    });
    (identity, store, certificate, attempt)
}

#[test]
fn exact_source_retries_survive_reopen_and_same_pending_does_not_renew_authorization() {
    let (identity, store, certificate, mut attempt) = fixture();
    store
        .admit_link_enrollment(&identity, &certificate, &attempt, b"original")
        .unwrap();
    let reopened = Store::open(Path::new(store.db.path().unwrap()), &identity).unwrap();
    reopened.set_link_pending(b"original").unwrap();
    reopened
        .admit_link_enrollment(&identity, &certificate, &attempt, b"original")
        .unwrap();
    attempt.directory_from(&Profile {
        version: 2,
        ..Default::default()
    });
    assert!(matches!(
        reopened.admit_link_enrollment(&identity, &certificate, &attempt, b"original"),
        Err(Error::LinkEnrollmentReviewRequired)
    ));
    assert_eq!(
        reopened.device_listed().unwrap(),
        0,
        "intent is not factual membership"
    );
    reopened.set_link_pending(b"new owner grant").unwrap();
    assert!(matches!(
        reopened.admit_link_enrollment(&identity, &certificate, &attempt, b"original"),
        Err(Error::LinkStateChanged)
    ));
    reopened
        .admit_link_enrollment(&identity, &certificate, &attempt, b"new owner grant")
        .unwrap();
}

#[test]
fn certificate_or_same_version_directory_changes_cannot_reuse_an_intent() {
    let (identity, store, certificate, mut attempt) = fixture();
    store
        .admit_link_enrollment(&identity, &certificate, &attempt, b"original")
        .unwrap();
    let other = identity.sign(Domain::Profile, vec![2]);
    assert!(matches!(
        store.admit_link_enrollment(&identity, &other, &attempt, b"original"),
        Err(Error::LinkEnrollmentReviewRequired)
    ));
    attempt.directory_from(&Profile {
        version: 1,
        name: "different".into(),
        ..Default::default()
    });
    assert!(matches!(
        store.admit_link_enrollment(&identity, &certificate, &attempt, b"original"),
        Err(Error::LinkEnrollmentReviewRequired)
    ));
}

#[test]
fn legacy_pending_does_not_gain_fresh_authorization_from_an_idempotent_save() {
    let (identity, source, certificate, attempt) = fixture();
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    // Construct the historical layout without deleting new-format provenance.
    store
        .db
        .execute(
            "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, 1, ?2)",
            params!["link_pending", source.link_pending().unwrap().unwrap()],
        )
        .unwrap();
    store.set_link_pending(b"original").unwrap();
    assert!(matches!(
        store.admit_link_enrollment(&identity, &certificate, &attempt, b"original"),
        Err(Error::LinkEnrollmentReviewRequired)
    ));
}

#[test]
fn intent_write_failure_preserves_the_fresh_grant_and_never_records_membership() {
    let (identity, store, certificate, attempt) = fixture();
    store
        .db
        .execute_batch(
            "CREATE TRIGGER `fail_enrollment_intent` BEFORE INSERT ON `settings`
         WHEN NEW.`key` = 'link_enrollment'
         BEGIN SELECT RAISE(ABORT, 'injected intent failure'); END;",
        )
        .unwrap();
    assert!(matches!(
        store.admit_link_enrollment(&identity, &certificate, &attempt, b"original"),
        Err(Error::Sql(_))
    ));
    let saved: Vec<u8> = store
        .db
        .query_row(
            "SELECT `value` FROM `settings` WHERE `key` = ?1",
            [KEY],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(saved, sha256(b"original"));
    assert_eq!(store.device_listed().unwrap(), 0);
    assert!(store.db.is_autocommit());
}

#[test]
fn known_membership_cannot_become_fresh_enrollment_at_a_contradictory_version() {
    let (identity, store, certificate, attempt) = fixture();
    store
        .note_device_listed(attempt.directory_source().unwrap().version)
        .unwrap();
    assert!(matches!(
        store.admit_link_enrollment(&identity, &certificate, &attempt, b"original"),
        Err(Error::LinkEnrollmentReviewRequired)
    ));
}

#[test]
fn unknown_or_malformed_intent_formats_fail_closed() {
    let (identity, store, certificate, attempt) = fixture();
    for (format, bytes) in [(2, vec![0_u8; 32]), (1, vec![0; 31]), (1, vec![0; 32])] {
        store
            .db
            .execute(
                "UPDATE `settings` SET `format_version` = ?1, `value` = ?2 WHERE `key` = ?3",
                params![format, bytes, KEY],
            )
            .unwrap();
        assert!(matches!(
            store.admit_link_enrollment(&identity, &certificate, &attempt, b"original"),
            Err(Error::InvalidSetting)
        ));
        assert_eq!(store.device_listed().unwrap(), 0);
        assert!(store.db.is_autocommit());
    }
}
