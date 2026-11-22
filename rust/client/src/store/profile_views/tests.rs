use super::*;
use crate::interactions::{Interaction, Kind};
use dyapp_identity::Identity;
use dyapp_messaging::hlc::Timestamp;

const DAY_MS: u64 = 86_400_000;
const LEAP_DAY: u64 = 19_782;

fn event(id: u8, day: u64, kind: Kind) -> Interaction {
    Interaction {
        event_id: [id; 16],
        kind,
        hlc: Timestamp::new(day * DAY_MS, 0).into(),
    }
}

fn receive(store: &mut Store, peer: &str, id: u8, day: u64) {
    assert!(!store
        .receive_interaction(
            peer,
            b"device",
            event(id, day, Kind::View),
            (LEAP_DAY + 1) * DAY_MS,
        )
        .unwrap());
}

#[test]
fn latest_received_days_are_ordered_persistent_and_independent_of_sending() {
    let dir = dyapp_test_support::test_dir().unwrap();
    let path = dir.path().join("owner.db");
    let owner = Identity::try_generate().unwrap();
    let mut peers: Vec<_> = (0..3)
        .map(|_| Identity::try_generate().unwrap().peer_id())
        .collect();
    peers.sort();
    let mut store = Store::open(&path, &owner).unwrap();
    assert!(!store.send_profile_views().unwrap());
    receive(&mut store, &peers[0], 1, 0);
    receive(&mut store, &peers[1], 2, LEAP_DAY);
    receive(&mut store, &peers[2], 3, LEAP_DAY);
    receive(&mut store, &peers[0], 4, 19_675);
    receive(&mut store, &peers[0], 4, 19_675); // Exact replay.
    receive(&mut store, &peers[0], 5, 1); // A late old view never replaces a newer day.
    store
        .receive_own_interaction(
            &peers[0],
            b"linked device",
            event(6, LEAP_DAY + 1, Kind::View),
            (LEAP_DAY + 1) * DAY_MS,
        )
        .unwrap();
    let expected = vec![
        ReceivedProfileView {
            peer_id: peers[1].clone(),
            utc_date: "2024-02-29".into(),
        },
        ReceivedProfileView {
            peer_id: peers[2].clone(),
            utc_date: "2024-02-29".into(),
        },
        ReceivedProfileView {
            peer_id: peers[0].clone(),
            utc_date: "2023-11-14".into(),
        },
    ];
    assert_eq!(store.received_profile_views().unwrap(), expected);
    assert!(store.chat_rows().unwrap().is_empty());
    for peer in &peers {
        assert_eq!(store.likes(peer).unwrap(), crate::store::Likes::default());
    }
    drop(store);
    let reopened = Store::open(&path, &owner).unwrap();
    assert_eq!(reopened.received_profile_views().unwrap(), expected);
    let other = Store::open(
        &dir.path().join("other.db"),
        &Identity::try_generate().unwrap(),
    )
    .unwrap();
    assert!(other.received_profile_views().unwrap().is_empty());
}

#[test]
fn moderation_hides_views_without_rewriting_history_or_hiding_matches() {
    let dir = dyapp_test_support::test_dir().unwrap();
    let owner = Identity::try_generate().unwrap();
    let mut store = Store::open(&dir.path().join("owner.db"), &owner).unwrap();
    let peers: Vec<_> = (0..3)
        .map(|_| Identity::try_generate().unwrap().peer_id())
        .collect();
    for (id, peer) in (1..).zip(&peers) {
        receive(&mut store, peer, id, LEAP_DAY);
    }
    let ours = owner.peer_id();
    let peer = &peers[2];
    store
        .apply_interaction(&ours, peer, b"ours", event(10, LEAP_DAY + 1, Kind::Like))
        .unwrap();
    store
        .apply_interaction(peer, &ours, b"theirs", event(11, LEAP_DAY + 1, Kind::Like))
        .unwrap();
    assert!(store.likes(peer).unwrap().matched());
    let before = store.received_profile_views().unwrap();
    assert_eq!(before.len(), 3);
    store
        .apply_interaction(&ours, peer, b"ours", event(12, LEAP_DAY + 2, Kind::Unlike))
        .unwrap();
    store.set_blocked(&peers[0], true).unwrap();
    assert!(store
        .pass(&peers[1], true, (LEAP_DAY + 2) * DAY_MS)
        .unwrap());
    assert!(store.received_profile_views().unwrap().is_empty());
    for peer in &peers {
        assert!(store.viewed_on(peer, &ours, LEAP_DAY * DAY_MS).unwrap());
    }
    store.set_blocked(&peers[0], false).unwrap();
    assert!(store
        .pass(&peers[1], false, (LEAP_DAY + 2) * DAY_MS)
        .unwrap());
    let expected: Vec<_> = before
        .into_iter()
        .filter(|row| row.peer_id != *peer)
        .collect();
    assert_eq!(store.received_profile_views().unwrap(), expected);
}

#[test]
fn invalid_sqlite_days_are_errors_not_guessed_dates() {
    let dir = dyapp_test_support::test_dir().unwrap();
    let owner = Identity::try_generate().unwrap();
    let peer = Identity::try_generate().unwrap().peer_id();
    for day in [i64::MIN, -1, i64::MAX] {
        let store = Store::open(&dir.path().join(format!("day-{day}.db")), &owner).unwrap();
        // Model a corrupt database; normal writes already reject negatives through the schema.
        store
            .db
            .pragma_update(None, "ignore_check_constraints", true)
            .unwrap();
        store
            .db
            .execute(
                "INSERT INTO `view_days` (`sender`, `target`, `day`) VALUES (?1, ?2, ?3)",
                rusqlite::params![peer, owner.peer_id(), day],
            )
            .unwrap();
        store
            .db
            .pragma_update(None, "ignore_check_constraints", false)
            .unwrap();
        assert!(
            matches!(store.received_profile_views(), Err(Error::ClockRange)),
            "day {day} must not be rendered"
        );
    }
}

#[test]
fn zero_sqlite_day_is_the_valid_utc_epoch() {
    let dir = dyapp_test_support::test_dir().unwrap();
    let owner = Identity::try_generate().unwrap();
    let peer = Identity::try_generate().unwrap().peer_id();
    let mut store = Store::open(&dir.path().join("epoch.db"), &owner).unwrap();
    receive(&mut store, &peer, 1, 0);
    assert_eq!(
        store.received_profile_views().unwrap(),
        vec![ReceivedProfileView {
            peer_id: peer,
            utc_date: "1970-01-01".into(),
        }]
    );
}
