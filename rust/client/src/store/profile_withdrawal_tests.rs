use super::*;
use crate::store::tests::temp_db_path;
use dyapp_identity::Identity;

#[test]
fn preparation_exceeds_every_known_version_and_refuses_exhaustion() {
    let identity = Identity::from_secret(&[7; 32]);
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    store
        .save_profile_draft(&Profile {
            version: 2,
            ..Default::default()
        })
        .unwrap();
    let request = Profile {
        version: 3,
        ..Default::default()
    };
    assert_eq!(
        store
            .prepare_profile_withdrawal(&identity, &request, 4)
            .unwrap()
            .version,
        5
    );
    store.own_profile_age(6, 100).unwrap();
    assert_eq!(
        store
            .prepare_profile_withdrawal(&identity, &request, 4)
            .unwrap()
            .version,
        7
    );
    let held = Profile {
        version: 8,
        ..Default::default()
    }
    .sign(&identity);
    store.search_profile_sent(&held, 1000).unwrap();
    assert_eq!(
        store
            .prepare_profile_withdrawal(&identity, &request, 4)
            .unwrap()
            .version,
        9
    );
    assert!(matches!(
        store.prepare_profile_withdrawal(&identity, &request, u64::MAX),
        Err(Error::ProfileVersionExhausted)
    ));
    assert_eq!(store.profile_draft().unwrap().unwrap().version, 2);
}

#[test]
fn completion_preserves_latest_fields_photos_devices_and_stronger_proof() {
    let identity = Identity::from_secret(&[7; 32]);
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    let request = Profile {
        name: "old".into(),
        ..Default::default()
    };
    let prepared = store
        .prepare_profile_withdrawal(&identity, &request, 0)
        .unwrap();
    let key = Identity::generate();
    let device = dyapp_messaging::mls::Device::new(&identity, key.public_key(), 1).unwrap();
    let mut latest = Profile {
        name: "new".into(),
        interests: vec!["rust".into()],
        devices: vec![device.certificate().clone()],
        device_owners: vec![crate::owner_binding(&key, &identity.public_key())],
        photos: vec![dyapp_profile::Photo {
            full: vec![vec![7; 32]],
            thumbnail: vec![8; 32],
        }],
        ..Default::default()
    };
    assert!(crate::publish::work(
        &mut latest,
        &identity.public_key(),
        1,
        1000
    ));
    store.save_profile_draft(&latest).unwrap();
    latest.version = prepared.version;
    assert_eq!(
        store
            .commit_profile_withdrawal(&identity, &request, &prepared)
            .unwrap(),
        latest
    );
    assert_eq!(store.profile_draft().unwrap(), Some(latest));
    assert!(store.profile_hidden().unwrap());
    assert_eq!(
        store.due_search_profile(0).unwrap(),
        Some(prepared.sign(&identity))
    );
}

#[test]
fn request_proof_and_fields_are_a_fallback_when_no_draft_exists() {
    let identity = Identity::from_secret(&[7; 32]);
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    let mut request = Profile {
        name: "fallback".into(),
        ..Default::default()
    };
    assert!(crate::publish::work(
        &mut request,
        &identity.public_key(),
        1,
        1000
    ));
    let prepared = store
        .prepare_profile_withdrawal(&identity, &request, 0)
        .unwrap();
    let result = store
        .commit_profile_withdrawal(&identity, &request, &prepared)
        .unwrap();
    assert_eq!(
        (result.name.as_str(), result.pow_nonce, result.pow_bits),
        ("fallback", request.pow_nonce, request.pow_bits)
    );
}

#[test]
fn newer_draft_search_or_observation_refuses_completion_without_mutation() {
    for source in ["draft", "search", "seen"] {
        let identity = Identity::from_secret(&[7; 32]);
        let store = Store::open(&temp_db_path(), &identity).unwrap();
        let request = Profile::default();
        let prepared = store
            .prepare_profile_withdrawal(&identity, &request, 0)
            .unwrap();
        let newer = Profile {
            version: 2,
            name: source.into(),
            ..Default::default()
        };
        match source {
            "draft" => store.save_profile_draft(&newer).unwrap(),
            "search" => store
                .search_profile_sent(&newer.sign(&identity), 1000)
                .unwrap(),
            _ => {
                store.own_profile_age(2, 100).unwrap();
            }
        }
        let before = store.profile_draft().unwrap();
        assert!(matches!(
            store.commit_profile_withdrawal(&identity, &request, &prepared),
            Err(Error::ProfileCompletionSuperseded)
        ));
        assert_eq!(store.profile_draft().unwrap(), before);
        assert!(!store.profile_hidden().unwrap());
        assert!(store.due_search_profile(0).unwrap().is_none());
    }
}

#[test]
fn same_version_fork_rolls_back_draft_and_visibility() {
    let identity = Identity::from_secret(&[7; 32]);
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    let request = Profile {
        name: "keep".into(),
        ..Default::default()
    };
    store.save_profile_draft(&request).unwrap();
    let prepared = store
        .prepare_profile_withdrawal(&identity, &request, 0)
        .unwrap();
    let shown = Profile {
        version: prepared.version,
        ..Default::default()
    }
    .sign(&identity);
    store.hold_search_profile(&shown).unwrap();
    assert!(matches!(
        store.commit_profile_withdrawal(&identity, &request, &prepared),
        Err(Error::SearchProfileConflict)
    ));
    assert_eq!(store.profile_draft().unwrap(), Some(request));
    assert!(!store.profile_hidden().unwrap());
    assert_eq!(store.due_search_profile(1000).unwrap(), Some(shown));
}

#[test]
fn equal_version_ordinary_draft_is_not_overwritten_before_its_search_write() {
    let identity = Identity::from_secret(&[7; 32]);
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    let request = Profile::default();
    let prepared = store
        .prepare_profile_withdrawal(&identity, &request, 0)
        .unwrap();
    let current = Profile {
        version: prepared.version,
        name: "ordinary publication".into(),
        ..Default::default()
    };
    store.save_profile_draft(&current).unwrap();
    assert!(matches!(
        store.commit_profile_withdrawal(&identity, &request, &prepared),
        Err(Error::ProfileCompletionSuperseded)
    ));
    assert_eq!(store.profile_draft().unwrap(), Some(current));
    assert!(!store.profile_hidden().unwrap());
    assert!(store.due_search_profile(0).unwrap().is_none());
}

#[test]
fn equal_version_hidden_completion_is_idempotent_after_search_ack() {
    let identity = Identity::from_secret(&[7; 32]);
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    let request = Profile::default();
    let prepared = store
        .prepare_profile_withdrawal(&identity, &request, 0)
        .unwrap();
    let first = store
        .commit_profile_withdrawal(&identity, &request, &prepared)
        .unwrap();
    store
        .search_profile_sent(&prepared.sign(&identity), 1000)
        .unwrap();
    assert_eq!(
        store
            .commit_profile_withdrawal(&identity, &request, &prepared)
            .unwrap(),
        first
    );
    assert!(store.due_search_profile(u64::MAX).unwrap().is_none());
}

#[test]
fn final_flag_failure_rolls_back_all_projection_rows() {
    let identity = Identity::from_secret(&[7; 32]);
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    let request = Profile {
        version: 1,
        ..Default::default()
    };
    store.save_profile_draft(&request).unwrap();
    let held = request.sign(&identity);
    store.hold_search_profile(&held).unwrap();
    let prepared = store
        .prepare_profile_withdrawal(&identity, &request, 0)
        .unwrap();
    store
        .db
        .execute_batch(
            "CREATE TEMP TRIGGER `refuse_withdraw` BEFORE INSERT ON `settings`
         WHEN NEW.`key` = 'profile_hidden' BEGIN SELECT RAISE(ABORT, 'withdraw refused'); END;",
        )
        .unwrap();
    let error = store
        .commit_profile_withdrawal(&identity, &request, &prepared)
        .unwrap_err();
    assert!(error.to_string().contains("withdraw refused"));
    assert_eq!(store.profile_draft().unwrap(), Some(request));
    assert_eq!(store.due_search_profile(1000).unwrap(), Some(held));
    assert_eq!(store.known_search_version().unwrap(), 1);
    assert!(!store.profile_hidden().unwrap());
}

#[test]
fn owner_and_canonical_tombstone_checks_precede_writes() {
    let identity = Identity::from_secret(&[7; 32]);
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    let request = Profile::default();
    let prepared = Profile::tombstone(1, 0, 0);
    let stranger = Identity::generate();
    assert!(matches!(
        store.prepare_profile_withdrawal(&stranger, &request, 0),
        Err(Error::WrongIdentity)
    ));
    assert!(matches!(
        store.commit_profile_withdrawal(&stranger, &request, &prepared),
        Err(Error::WrongIdentity)
    ));
    for invalid in [
        Profile::default(),
        Profile {
            name: "not a tombstone".into(),
            ..prepared
        },
    ] {
        assert!(matches!(
            store.commit_profile_withdrawal(&identity, &request, &invalid),
            Err(Error::InvalidSetting)
        ));
    }
    assert!(store.profile_draft().unwrap().is_none());
}

#[test]
fn completion_acquires_writer_before_reading_current_draft() {
    let identity = Identity::from_secret(&[7; 32]);
    let path = temp_db_path();
    let first = Store::open(&path, &identity).unwrap();
    let second = Store::open(&path, &identity).unwrap();
    second.db.busy_timeout(std::time::Duration::ZERO).unwrap();
    let writer = Transaction::new_unchecked(&first.db, TransactionBehavior::Immediate).unwrap();
    assert!(
        matches!(second.commit_profile_withdrawal(&identity, &Profile::default(), &Profile::tombstone(1, 0, 0)),
        Err(Error::Sql(rusqlite::Error::SqliteFailure(error, _))) if error.code == rusqlite::ErrorCode::DatabaseBusy)
    );
    writer.rollback().unwrap();
    assert!(second.profile_draft().unwrap().is_none());
}
