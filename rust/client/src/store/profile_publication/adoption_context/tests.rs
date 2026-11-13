use super::*;
use crate::store::tests::temp_db_path;
use dyapp_profile::Profile;
use std::path::Path;

#[test]
fn an_exact_receipt_cannot_replay_below_a_newer_ordinary_observation() {
    let (identity, store) = fixture();
    store.set_profile_hidden(true).unwrap();
    let mut local = store.profile_draft().unwrap().unwrap();
    local.version = 4;
    store.save_profile_draft(&local).unwrap();
    let context = store
        .prepare_profile_adoption_context(&identity)
        .unwrap()
        .unwrap();
    let remote = Profile {
        version: 2,
        name: "remote".into(),
        ..Default::default()
    };
    store
        .commit_reviewed_adoption(&identity, &context, &remote, &remote)
        .unwrap();
    let saved = store.profile_draft().unwrap();
    let head = store.due_search_profile(0).unwrap();
    assert!(saved.as_ref().unwrap().version > remote.version);
    store.own_profile_age(3, 10).unwrap();
    let attempt = store.prepare_profile_publication(&identity).unwrap();
    assert!(matches!(
        store.commit_profile_import(&identity, &attempt, &remote),
        Err(Error::ProfileCompletionSuperseded)
    ));
    assert_eq!(store.profile_draft().unwrap(), saved);
    assert_eq!(store.due_search_profile(0).unwrap(), head);
    assert_eq!(store.own_profile_seen().unwrap(), 3);
}

#[test]
fn reviewed_draft_receipt_failure_rolls_back_projection_and_blob_pruning() {
    let (identity, store) = fixture();
    let before = store.profile_draft().unwrap();
    let orphan = store.save_profile_blob(&[0xFF, 0xD8, 0xFF, 7]).unwrap();
    let context = store
        .prepare_profile_adoption_context(&identity)
        .unwrap()
        .unwrap();
    let remote = Profile {
        version: 3,
        name: "remote".into(),
        ..Default::default()
    };
    let selected = Profile {
        name: "chosen local".into(),
        ..remote.clone()
    };
    store
        .db
        .execute_batch(
            "CREATE TEMP TRIGGER `refuse_review_receipt` BEFORE INSERT ON `settings`
         WHEN NEW.`key` = 'profile_completion_receipt'
         BEGIN SELECT RAISE(ABORT, 'review receipt refused'); END;",
        )
        .unwrap();
    let error = store
        .commit_reviewed_adoption(&identity, &context, &remote, &selected)
        .unwrap_err();
    assert!(error.to_string().contains("review receipt refused"));
    assert_eq!(store.device_listed().unwrap(), 0);
    assert_eq!(store.profile_draft().unwrap(), before);
    assert!(store.due_search_profile(1000).unwrap().is_none());
    assert!(store
        .completed_profile_version(&identity, &remote)
        .unwrap()
        .is_none());
    assert!(store.profile_photo(&orphan).unwrap().is_some());
    assert_eq!(
        store.link_pending().unwrap(),
        Some(b"original link".to_vec())
    );
}

#[test]
fn hidden_review_exhaustion_is_refused_before_any_completion_write() {
    let (identity, store) = fixture();
    store.set_profile_hidden(true).unwrap();
    let before = store.profile_draft().unwrap();
    let context = store
        .prepare_profile_adoption_context(&identity)
        .unwrap()
        .unwrap();
    let remote = Profile {
        version: u64::MAX,
        ..Default::default()
    };
    assert!(matches!(
        store.validate_reviewed_adoption(&identity, &context, &remote, &remote),
        Err(Error::ProfileVersionExhausted)
    ));
    assert_eq!(store.profile_draft().unwrap(), before);
    assert!(store.due_search_profile(1000).unwrap().is_none());
    assert!(store
        .profile_adoption_context_current(&identity, &context)
        .unwrap());
}

fn fixture() -> (Identity, Store) {
    let identity = Identity::from_secret(&[7; 32]);
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    store
        .save_profile_draft(&Profile {
            version: 1,
            name: "before".into(),
            ..Default::default()
        })
        .unwrap();
    store.set_link_pending(b"original link").unwrap();
    (identity, store)
}

#[test]
fn review_context_binds_database_and_owner_but_survives_reopen() {
    let (identity, store) = fixture();
    let context = store
        .prepare_profile_adoption_context(&identity)
        .unwrap()
        .unwrap();
    let reopened = Store::open(Path::new(store.db.path().unwrap()), &identity).unwrap();
    assert!(reopened
        .profile_adoption_context_current(&identity, &context)
        .unwrap());
    let (_, other_database) = fixture();
    assert!(!other_database
        .profile_adoption_context_current(&identity, &context)
        .unwrap());
    let other_owner = Identity::from_secret(&[8; 32]);
    assert!(matches!(
        store.profile_adoption_context_current(&other_owner, &context),
        Err(Error::WrongIdentity)
    ));
    assert!(store.db.is_autocommit());
    assert!(reopened.db.is_autocommit());
}

#[test]
fn local_edits_and_replaced_link_state_are_preserved_and_require_new_review() {
    let (identity, store) = fixture();
    let mut writer = Store::open(Path::new(store.db.path().unwrap()), &identity).unwrap();
    let context = store
        .prepare_profile_adoption_context(&identity)
        .unwrap()
        .unwrap();
    writer
        .update_profile_draft(|draft| {
            draft.name = "after review".into();
            Ok(())
        })
        .unwrap();
    assert!(!store
        .profile_adoption_context_current(&identity, &context)
        .unwrap());
    assert_eq!(store.profile_draft().unwrap().unwrap().name, "after review");
    let next = store
        .prepare_profile_adoption_context(&identity)
        .unwrap()
        .unwrap();
    writer.set_link_pending(b"replacement link").unwrap();
    assert!(!store
        .profile_adoption_context_current(&identity, &next)
        .unwrap());
    assert_eq!(
        store.link_pending().unwrap(),
        Some(b"replacement link".to_vec())
    );
    assert!(store.db.is_autocommit());
}

#[test]
fn search_ack_preserves_review_but_a_new_signed_head_invalidates_it() {
    let (identity, store) = fixture();
    let queued = Profile {
        version: 2,
        name: "queued".into(),
        ..Default::default()
    }
    .sign(&identity);
    store.hold_search_profile(&queued).unwrap();
    let context = store
        .prepare_profile_adoption_context(&identity)
        .unwrap()
        .unwrap();
    store.search_profile_sent(&queued, 100).unwrap();
    assert!(store
        .profile_adoption_context_current(&identity, &context)
        .unwrap());
    let newer = Profile {
        version: 3,
        name: "newer".into(),
        ..Default::default()
    }
    .sign(&identity);
    store.hold_search_profile(&newer).unwrap();
    assert!(!store
        .profile_adoption_context_current(&identity, &context)
        .unwrap());
    assert_eq!(store.profile_draft().unwrap().unwrap().name, "before");
}

#[test]
fn visibility_observed_version_and_device_listing_each_invalidate_review() {
    let changes: [fn(&Store); 3] = [
        |store| store.set_profile_hidden(true).unwrap(),
        |store| {
            store.own_profile_age(2, 100).unwrap();
        },
        |store| store.note_device_listed(2).unwrap(),
    ];
    for change in changes {
        let (identity, store) = fixture();
        let context = store
            .prepare_profile_adoption_context(&identity)
            .unwrap()
            .unwrap();
        change(&store);
        assert!(!store
            .profile_adoption_context_current(&identity, &context)
            .unwrap());
    }
}

#[test]
fn uncommitted_writer_cannot_expose_a_mixed_profile_link_and_search_snapshot() {
    let (identity, reader) = fixture();
    let writer = Store::open(Path::new(reader.db.path().unwrap()), &identity).unwrap();
    let transaction =
        Transaction::new_unchecked(&writer.db, TransactionBehavior::Immediate).unwrap();
    let staged = Profile {
        version: 2,
        name: "staged".into(),
        ..Default::default()
    };
    writer.save_profile_draft(&staged).unwrap();
    writer.set_link_pending_in_tx(b"staged link").unwrap();
    writer
        .hold_search_profile_in_tx(&staged.sign(&identity))
        .unwrap();
    let before = reader
        .prepare_profile_adoption_context(&identity)
        .unwrap()
        .unwrap();
    assert_eq!(before.publication.draft().name, "before");
    assert_eq!(before.pending_link, b"original link");
    assert!(before.head.is_none());
    assert!(reader
        .profile_adoption_context_current(&identity, &before)
        .unwrap());
    transaction.commit().unwrap();
    assert!(!reader
        .profile_adoption_context_current(&identity, &before)
        .unwrap());
    let after = reader
        .prepare_profile_adoption_context(&identity)
        .unwrap()
        .unwrap();
    assert_eq!(after.publication.draft(), &staged);
    assert_eq!(after.pending_link, b"staged link");
    assert!(after.head.is_some());
    assert!(reader.db.is_autocommit());
}

#[test]
fn historical_membership_requires_the_original_database_owner_and_pending_operation() {
    let (identity, store) = fixture();
    let context = store
        .prepare_profile_adoption_context(&identity)
        .unwrap()
        .unwrap();
    let (_, other) = fixture();
    assert!(matches!(
        other.remember_reviewed_membership(&identity, &context, Some(2)),
        Err(Error::LinkStateChanged)
    ));
    let wrong = Identity::from_secret(&[9; 32]);
    assert!(matches!(
        store.remember_reviewed_membership(&wrong, &context, Some(2)),
        Err(Error::WrongIdentity)
    ));
    store.set_link_pending(b"replacement").unwrap();
    assert!(matches!(
        store.remember_reviewed_membership(&identity, &context, Some(2)),
        Err(Error::LinkStateChanged)
    ));
    assert_eq!(other.device_listed().unwrap(), 0);
    assert_eq!(store.device_listed().unwrap(), 0);
    assert!(store.db.is_autocommit());
}

#[test]
fn historical_membership_survives_draft_edits_without_treating_them_as_consent() {
    let (identity, mut store) = fixture();
    let context = store
        .prepare_profile_adoption_context(&identity)
        .unwrap()
        .unwrap();
    store
        .update_profile_draft(|draft| {
            draft.name = "later private choice".into();
            Ok(())
        })
        .unwrap();
    let draft = store.profile_draft().unwrap();
    store
        .remember_reviewed_membership(&identity, &context, Some(2))
        .unwrap();
    store
        .remember_reviewed_membership(&identity, &context, Some(1))
        .unwrap();
    assert_eq!(store.device_listed().unwrap(), 2);
    assert_eq!(store.profile_draft().unwrap(), draft);
    assert!(!store
        .profile_adoption_context_current(&identity, &context)
        .unwrap());
    assert!(store.link_pending().unwrap().is_some());
}

#[test]
fn no_pending_link_produces_no_context_or_default_profile() {
    let identity = Identity::from_secret(&[7; 32]);
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    assert!(store
        .prepare_profile_adoption_context(&identity)
        .unwrap()
        .is_none());
    assert_eq!(store.profile_draft().unwrap(), None);
    assert_eq!(store.link_pending().unwrap(), None);
    assert!(store.db.is_autocommit());
}
