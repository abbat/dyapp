use super::*;
use crate::store::tests::temp_db_path;
use dyapp_identity::Domain;

fn owner() -> Identity {
    Identity::from_secret(&[7; 32])
}

fn saved(store: &Store) -> Profile {
    store.profile_draft().unwrap().unwrap()
}

#[test]
fn hide_uses_latest_fields_photos_devices_and_stronger_proof() {
    let (path, identity) = (temp_db_path(), owner());
    let store = Store::open(&path, &identity).unwrap();
    let request = Profile {
        version: 1,
        name: "old".into(),
        ..Profile::default()
    };
    store.save_profile_draft(&request).unwrap();
    let mut editor = Store::open(&path, &identity).unwrap();
    let device_key = Identity::from_secret(&[8; 32]);
    let device = dyapp_messaging::mls::Device::new(&identity, device_key.public_key(), 1).unwrap();
    let binding = crate::owner_binding(&device_key, &identity.public_key());
    editor
        .update_profile_draft(|draft| {
            draft.version = 7;
            draft.name = "new".into();
            draft.interests = vec!["music".into()];
            crate::publish::list_device(draft, device.certificate().clone(), binding);
            Ok(())
        })
        .unwrap();
    editor
        .add_profile_photo(&[0xFF, 0xD8, 0xFF, 1], &[0xFF, 0xD8, 0xFF, 2])
        .unwrap();
    let work = editor.prepare_profile_work(&identity, 2, 1024).unwrap();
    assert!(editor.commit_profile_work(&work).unwrap().solved);
    let mut expected = saved(&editor);
    expected.version = 8;
    assert_eq!(
        store.hide_profile_draft(&identity, &request).unwrap(),
        expected
    );
    assert_eq!(saved(&editor), expected);
    assert!(store.profile_hidden().unwrap());
    let held = store.due_search_profile(0).unwrap().unwrap();
    assert_eq!(held.verify(Domain::Profile).unwrap(), identity.peer_id());
    assert_eq!(
        dyapp_profile::verify(&held).unwrap().profile,
        Profile::tombstone(8, expected.pow_nonce, expected.pow_bits)
    );
}

#[test]
fn hide_advances_past_all_known_versions() {
    for (request_version, seen, expected) in [(3, 0, 5), (3, 6, 7), (9, 6, 10)] {
        let identity = owner();
        let store = Store::open(&temp_db_path(), &identity).unwrap();
        store
            .save_profile_draft(&Profile {
                version: 1,
                name: "current".into(),
                ..Profile::default()
            })
            .unwrap();
        store
            .hold_search_profile(
                &Profile {
                    version: 4,
                    ..Profile::default()
                }
                .sign(&identity),
            )
            .unwrap();
        store.own_profile_age(seen, 100).unwrap();
        let request = Profile {
            version: request_version,
            name: "stale".into(),
            ..Profile::default()
        };
        let hidden = store.hide_profile_draft(&identity, &request).unwrap();
        assert_eq!(
            (hidden.version, hidden.name.as_str()),
            (expected, "current")
        );
    }
}

#[test]
fn final_visibility_write_failure_rolls_back_draft_and_search_projection() {
    let identity = owner();
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    let before = Profile {
        version: 1,
        ..Profile::default()
    };
    store.save_profile_draft(&before).unwrap();
    store.hold_search_profile(&before.sign(&identity)).unwrap();
    store.set_profile_hidden(false).unwrap();
    let projection = |store: &Store| -> Vec<u8> {
        store
            .db
            .query_row(
                "SELECT `value` FROM `settings` WHERE `key` = ?1",
                ["search_publish"],
                |row| row.get(0),
            )
            .unwrap()
    };
    let old_projection = projection(&store);
    store
        .db
        .execute_batch(
            "CREATE TEMP TRIGGER `refuse_hide_flag` BEFORE INSERT ON `settings`
         WHEN NEW.`key` = 'profile_hidden'
         BEGIN SELECT RAISE(ABORT, 'hide flag refused'); END;",
        )
        .unwrap();
    let error = store.hide_profile_draft(&identity, &before).unwrap_err();
    assert!(error.to_string().contains("hide flag refused"));
    assert_eq!(saved(&store), before);
    assert_eq!(projection(&store), old_projection);
    assert!(!store.profile_hidden().unwrap());
}

#[test]
fn empty_storage_uses_the_supplied_draft_without_publishing_its_fields() {
    let identity = owner();
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    let request = Profile {
        version: 4,
        about: "local fields".into(),
        ..Profile::default()
    };
    let hidden = store.hide_profile_draft(&identity, &request).unwrap();
    assert_eq!((hidden.version, hidden.about.as_str()), (5, "local fields"));
    let held = store.due_search_profile(0).unwrap().unwrap();
    assert_eq!(dyapp_profile::verify(&held).unwrap().profile.about, "");
}

#[test]
fn stronger_request_proof_is_kept_as_one_nonce_bits_pair() {
    let identity = owner();
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    store
        .save_profile_draft(&Profile {
            name: "current".into(),
            ..Profile::default()
        })
        .unwrap();
    let mut proof_store = Store::open(&temp_db_path(), &identity).unwrap();
    let work = proof_store
        .prepare_profile_work(&identity, 2, 1024)
        .unwrap();
    assert!(proof_store.commit_profile_work(&work).unwrap().solved);
    let mut request = saved(&proof_store);
    request.version = 3;
    let hidden = store.hide_profile_draft(&identity, &request).unwrap();
    assert_eq!(hidden.name, "current");
    assert_eq!(
        (hidden.pow_nonce, hidden.pow_bits),
        (request.pow_nonce, request.pow_bits)
    );
    assert!(dyapp_profile::pow_leading_bits(&identity.public_key(), hidden.pow_nonce) >= 2);
}

#[test]
fn another_owner_cannot_sign_a_hide_for_this_store() {
    let identity = owner();
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    let before = Profile {
        version: 1,
        ..Profile::default()
    };
    store.save_profile_draft(&before).unwrap();
    let other = Identity::from_secret(&[9; 32]);
    assert!(matches!(
        store.hide_profile_draft(&other, &before),
        Err(Error::WrongIdentity)
    ));
    assert_eq!(saved(&store), before);
    assert!(!store.profile_hidden().unwrap());
    assert_eq!(store.held_search_version().unwrap(), 0);
}

#[test]
fn exhausted_version_refuses_hide_without_any_projection_changes() {
    let identity = owner();
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    let before = Profile {
        version: u64::MAX,
        ..Profile::default()
    };
    store.save_profile_draft(&before).unwrap();
    assert!(matches!(
        store.hide_profile_draft(&identity, &before),
        Err(Error::ProfileVersionExhausted)
    ));
    assert_eq!(saved(&store), before);
    assert!(!store.profile_hidden().unwrap());
    assert_eq!(store.held_search_version().unwrap(), 0);
}

#[test]
fn malformed_held_profile_is_not_silently_replaced() {
    let identity = owner();
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    let before = Profile {
        version: 1,
        ..Profile::default()
    };
    store.save_profile_draft(&before).unwrap();
    store
        .hold_search_profile(&identity.sign(Domain::Profile, vec![0xFF]))
        .unwrap();
    assert!(matches!(
        store.hide_profile_draft(&identity, &before),
        Err(Error::InvalidSetting)
    ));
    assert_eq!(saved(&store), before);
    assert!(!store.profile_hidden().unwrap());
}
