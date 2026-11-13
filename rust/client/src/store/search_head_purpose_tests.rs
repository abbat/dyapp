use super::*;
use crate::store::tests::temp_db_path;
use dyapp_identity::Identity;
use dyapp_profile::{Profile, ProfileCondition};

fn saved(store: &Store) -> (i64, Vec<u8>) {
    store
        .db
        .query_row(
            "SELECT `format_version`, `value` FROM `settings` WHERE `key` = ?",
            [HEAD_KEY],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap()
}

fn historical(store: &Store, record: &SignedRecord, pending: bool) {
    let mut head = store.record_head(record).unwrap();
    head.purpose = None;
    store
        .set_search_publish(&State {
            last_sent: 1000,
            hold_until: 1120,
            pending: pending.then(|| record.clone()),
        })
        .unwrap();
    store.set_search_head(head).unwrap();
}

#[test]
fn historical_reads_derive_only_verified_pending_purpose_without_writing() {
    let cases = [
        (Profile::tombstone(8, 0, 0), Purpose::LegacyWithdrawal),
        (
            Profile::search_tombstone(8, 0, 0),
            Purpose::SearchWithdrawal,
        ),
    ];
    for (profile, purpose) in cases {
        for pending in [false, true] {
            let path = temp_db_path();
            let owner = Identity::from_secret(&[7; 32]);
            let store = Store::open(&path, &owner).unwrap();
            historical(&store, &profile.sign(&owner), pending);
            let before = saved(&store);
            let private = store.link_state().unwrap();
            drop(store);
            let store = Store::open_existing(&path, &owner).unwrap();
            let (state, head) = store.search_projection().unwrap();
            assert_eq!(head.unwrap().purpose, pending.then_some(purpose));
            assert_eq!((state.last_sent, state.hold_until), (1000, 1120));
            assert_eq!(saved(&store), before);
            assert_eq!(store.link_state().unwrap(), private);
        }
    }
}

#[test]
fn contradictory_pending_purpose_is_refused_without_rewriting_state() {
    let owner = Identity::from_secret(&[7; 32]);
    let store = Store::open(&temp_db_path(), &owner).unwrap();
    let record = Profile::search_tombstone(8, 0, 0).sign(&owner);
    store.hold_search_profile(&record).unwrap();
    let mut head = store.search_head().unwrap().unwrap();
    head.purpose = Some(Purpose::Ordinary);
    store.set_search_head(head).unwrap();
    let before = saved(&store);
    assert!(matches!(
        store.search_projection(),
        Err(Error::InvalidSetting)
    ));
    assert!(matches!(
        store.search_profile_sent(&record, 1000),
        Err(Error::InvalidSetting)
    ));
    assert_eq!(saved(&store), before);
}

#[test]
fn unknown_purpose_invalid_version_and_malformed_head_encodings_fail_closed() {
    let owner = Identity::from_secret(&[7; 32]);
    let store = Store::open(&temp_db_path(), &owner).unwrap();
    let head = store
        .record_head(&Profile::search_tombstone(8, 0, 0).sign(&owner))
        .unwrap();
    let (format, value) = head.encode();
    let mut unknown = value.clone();
    *unknown.last_mut().unwrap() = u8::MAX;
    let mut zero = value.clone();
    zero.first_chunk_mut::<8>().unwrap().fill(0);
    for (format, bytes) in [
        (format, unknown),
        (format, zero),
        (3, value),
        (1, vec![0; 40]),
        (2, vec![0; 40]),
    ] {
        assert!(matches!(
            Head::decode(format, &bytes),
            Err(Error::InvalidSetting)
        ));
    }
}

#[test]
fn verified_purpose_survives_acknowledgement_and_existing_only_reopen() {
    let cases = [
        (
            Profile {
                version: 8,
                ..Default::default()
            },
            0,
        ),
        (
            Profile {
                previous: Some(ProfileCondition::default()),
                ..Profile::tombstone(8, 0, 0)
            },
            0,
        ),
        (Profile::search_tombstone(8, 0, 0), 1),
        (Profile::tombstone(8, 0, 0), 2),
    ];
    for (profile, purpose) in cases {
        let path = temp_db_path();
        let owner = Identity::from_secret(&[7; 32]);
        let record = profile.sign(&owner);
        let store = Store::open(&path, &owner).unwrap();
        store.hold_search_profile(&record).unwrap();
        store.search_profile_sent(&record, 1000).unwrap();
        let (format, bytes) = saved(&store);
        assert_eq!(format, 2);
        assert_eq!(bytes.len(), 41);
        assert_eq!(bytes.last(), Some(&purpose));
        drop(store);
        let store = Store::open_existing(&path, &owner).unwrap();
        assert_eq!(saved(&store), (format, bytes));
        assert!(store.due_search_profile(u64::MAX).unwrap().is_none());
        assert_eq!(store.known_search_version().unwrap(), profile.version);
    }
}
