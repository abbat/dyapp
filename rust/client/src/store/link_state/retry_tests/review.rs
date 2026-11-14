//! Read-only comparison must not grant import or device authorization.
use super::*;
use crate::store::{LinkPreference as Field, LinkPreferenceValue as Value};

fn legacy(store: &Store, state: &[u8]) {
    store
        .db
        .execute(
            "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, 1, ?2)",
            rusqlite::params![PENDING, state],
        )
        .unwrap();
}

#[test]
fn comparison_offers_only_owner_preferences_without_writes_or_import_progress() {
    let identity = Identity::from_secret(&[7; 32]);
    let state = source(&identity);
    let mut store = Store::open(&temp_db_path(), &identity).unwrap();
    private_choices(&store, false, 2_000);
    store
        .interact("dave", Kind::Like, b"laptop", 1_000)
        .unwrap();
    store
        .interact("dave", Kind::Unlike, b"laptop", 2_000)
        .unwrap();
    legacy(&store, &state);
    let before = store.link_state().unwrap();
    let writes = store.db.total_changes();
    let review = store.prepare_link_state_review(&identity).unwrap().unwrap();
    assert!(review.is_current(&store, &identity).unwrap());
    let differences = review.differences();
    assert_eq!(differences.len(), 8);
    let note = differences
        .iter()
        .find(|difference| difference.field == Field::Note)
        .unwrap();
    assert_eq!(note.peer.as_deref(), Some("carol"));
    assert!(note.local == Value::Text(String::new()));
    assert!(note.pending == Value::Text("original note".into()));
    assert!(differences
        .iter()
        .any(|difference| difference.field == Field::OutgoingLike));
    assert_eq!(store.db.total_changes(), writes);
    assert_eq!(store.link_state().unwrap(), before);
    assert_eq!(store.link_pending().unwrap(), Some(state.clone()));
    assert!(matches!(
        store.import_link_state(&state),
        Err(Error::LinkStateReviewRequired)
    ));
}

#[test]
fn incoming_likes_trust_observations_and_unknown_fields_are_not_preference_choices() {
    let identity = Identity::from_secret(&[7; 32]);
    for case in 0..3 {
        let store = Store::open(&temp_db_path(), &identity).unwrap();
        private_choices(&store, false, 2_000);
        let mut pending = snapshot(&store);
        match case {
            0 => pending.likes.push(Like {
                sender: "someone".into(),
                target: identity.peer_id(),
                liked: true,
                hlc: 5,
                device: b"other".to_vec(),
                event_id: vec![1; 16],
            }),
            1 => pending.peers[0].verified = true,
            _ => {}
        }
        let mut bytes = pending.encode_to_vec();
        if case == 2 {
            bytes.extend([0xF8, 0x07, 0x01]);
        }
        legacy(&store, &bytes);
        let review = store.prepare_link_state_review(&identity).unwrap().unwrap();
        assert!(review.differences().is_empty());
        assert!(review.has_protected_differences());
    }
}

fn snapshot(store: &Store) -> LinkState {
    LinkState::decode(store.link_state().unwrap().as_slice()).unwrap()
}

#[test]
fn comparison_refuses_other_owners_databases_and_changed_private_context() {
    let identity = Identity::from_secret(&[7; 32]);
    let other = Identity::from_secret(&[9; 32]);
    let state = source(&identity);
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    legacy(&store, &state);
    assert!(matches!(
        store.prepare_link_state_review(&other),
        Err(Error::WrongIdentity)
    ));
    let review = store.prepare_link_state_review(&identity).unwrap().unwrap();
    assert!(matches!(
        review.is_current(&store, &other),
        Err(Error::WrongIdentity)
    ));
    let second = Store::open(&temp_db_path(), &identity).unwrap();
    legacy(&second, &state);
    assert!(!review.is_current(&second, &identity).unwrap());
    store.set_profile_hidden(true).unwrap();
    assert!(!review.is_current(&store, &identity).unwrap());
}

#[test]
fn replacement_progress_and_duplicate_rows_cannot_reuse_a_review() {
    let identity = Identity::from_secret(&[7; 32]);
    let state = source(&identity);
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    assert!(store
        .prepare_link_state_review(&identity)
        .unwrap()
        .is_none());
    legacy(&store, &state);
    let review = store.prepare_link_state_review(&identity).unwrap().unwrap();
    store
        .set_link_pending(&LinkState::default().encode_to_vec())
        .unwrap();
    assert!(!review.is_current(&store, &identity).unwrap());
    assert!(store
        .prepare_link_state_review(&identity)
        .unwrap()
        .is_none());
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    let mut duplicate = LinkState::decode(state.as_slice()).unwrap();
    duplicate.peers.push(duplicate.peers[0].clone());
    legacy(&store, &duplicate.encode_to_vec());
    assert!(matches!(
        store.prepare_link_state_review(&identity),
        Err(Error::InvalidSetting)
    ));
}
