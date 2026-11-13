use super::*;
use crate::store::tests::temp_db_path;
use dyapp_profile::Profile;
use std::path::Path;

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
    writer.set_link_pending(b"staged link").unwrap();
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
