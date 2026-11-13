//! Linked private-state wire round trips and pending-row storage.
use super::super::tests::temp_db_path;
use super::*;
use crate::interactions::Kind;
use dyapp_identity::Identity;
use prost::Message as _;

#[test]
fn a_pending_link_stays_until_cleared_and_reads_only_its_format() {
    let store = Store::open(&temp_db_path(), &Identity::generate()).unwrap();
    assert_eq!(store.link_pending().unwrap(), None);
    assert!(!store.is_linked_device().unwrap());
    store.set_link_pending(b"first").unwrap();
    store.set_link_pending(b"state").unwrap();
    assert_eq!(store.link_pending().unwrap().unwrap(), b"state");
    store
        .db
        .execute(
            "UPDATE `settings` SET `format_version` = 2 WHERE `key` = ?1",
            [PENDING],
        )
        .unwrap();
    assert!(matches!(store.link_pending(), Err(Error::InvalidSetting)));
    store.clear_link_pending().unwrap();
    assert_eq!(store.link_pending().unwrap(), None);
    assert!(store.is_linked_device().unwrap());
}

#[test]
fn linked_notes_keep_text_clock_actor_and_reject_invalid_snapshots() {
    use crate::chat::PeerChange;
    let owner = Identity::generate();
    let old = Store::open(&temp_db_path(), &owner).unwrap();
    let high = 1u64 << 40;
    old.apply_peer_change(
        "bob",
        &PeerChange::Note("Case\nLine".into()),
        high.into(),
        &[2; 32],
    )
    .unwrap();
    let snapshot = old.link_state().unwrap();
    let mut linked = Store::open(&temp_db_path(), &owner).unwrap();
    linked.import_link_state(&snapshot).unwrap();
    assert_eq!(linked.contact_note("bob").unwrap(), "Case\nLine");
    assert!(u64::from(linked.tick(0).unwrap()) > high);
    assert!(!linked
        .receive_peer_change(
            "bob",
            &PeerChange::Note("older actor".into()),
            high,
            dyapp_messaging::hlc::Timestamp::from(high).wall_ms(),
            &[1; 32]
        )
        .unwrap());
    let mut invalid = LinkState::decode(snapshot.as_slice()).unwrap();
    invalid.peers[0].note = "hidden\u{202e}".into();
    assert!(matches!(
        linked.import_link_state(&invalid.encode_to_vec()),
        Err(Error::InvalidSetting)
    ));
    assert_eq!(linked.contact_note("bob").unwrap(), "Case\nLine");
}

#[test]
fn linked_favorites_keep_their_clock_and_hide_blocked_peers() {
    let owner = Identity::generate();
    let old = Store::open(&temp_db_path(), &owner).unwrap();
    old.apply_peer_change(
        "bob",
        &crate::chat::PeerChange::Favorite(true),
        1u64.into(),
        &[],
    )
    .unwrap();
    old.set_blocked("bob", true).unwrap();
    let high = 1i64 << 40;
    old.db
        .execute("UPDATE `peers` SET `favorite_hlc` = ?1", [high])
        .unwrap();
    let mut linked = Store::open(&temp_db_path(), &owner).unwrap();
    linked
        .import_link_state(&old.link_state().unwrap())
        .unwrap();
    assert!(linked.favorite("bob").unwrap());
    assert_eq!(
        linked.favorite_peers().unwrap(),
        [] as [std::string::String; 0]
    );
    assert!(u64::from(linked.tick(0).unwrap()) > u64::try_from(high).unwrap());
    linked.set_blocked("bob", false).unwrap();
    assert_eq!(linked.favorite_peers().unwrap(), ["bob"]);
}

#[test]
fn a_fresh_store_takes_the_owners_state_and_later_events_win() {
    let alice = Identity::generate();
    let mut old = Store::open(&temp_db_path(), &alice).unwrap();
    old.set_blocked("bob", true).unwrap();
    old.set_muted("carol", true).unwrap();
    old.set_local_name("carol", "Carrie").unwrap();
    old.db
        .execute(
            "UPDATE `peers` SET `verified` = 1 WHERE `peer_id` = 'carol'",
            [],
        )
        .unwrap();
    old.interact("dave", Kind::Like, b"phone", 1_000).unwrap();
    old.pass("erin", true, 1_000).unwrap();
    old.set_profile_hidden(true).unwrap();
    // A row can be newer than the local clock, as one from a peer's clock is.
    old.db
        .execute(
            "UPDATE `pass_history` SET `updated_hlc` = `updated_hlc` + 1000000",
            [],
        )
        .unwrap();

    let mut new = Store::open(&temp_db_path(), &alice).unwrap();
    let state = old.link_state().unwrap();
    new.import_link_state(&state).unwrap();
    new.import_link_state(&state).unwrap();

    assert!(new.blocked("bob").unwrap());
    assert!(new.muted("carol").unwrap());
    assert_eq!(new.local_name("carol").unwrap(), "Carrie");
    assert_eq!(new.passed_peers().unwrap(), ["erin"]);
    assert!(new.profile_hidden().unwrap());
    let (rows, clock): (i64, u64) = (
        new.db
            .query_row("SELECT `updated_hlc` FROM `pass_history`", [], |row| {
                row.get(0)
            })
            .unwrap(),
        load_hlc(&new.db).unwrap().into(),
    );
    assert!(clock >= u64::try_from(rows).unwrap());
    check_exported_snapshot(&state, &new);
}

fn check_exported_snapshot(state: &[u8], new: &Store) {
    // Peers, likes and passes arrive whole: export again and only the clock may differ.
    let old_state = LinkState::decode(state).unwrap();
    let new_state = LinkState::decode(new.link_state().unwrap().as_slice()).unwrap();
    assert_eq!(
        LinkState {
            hlc: 0,
            ..old_state
        },
        LinkState {
            hlc: 0,
            ..new_state
        }
    );
}

#[test]
fn bytes_that_are_no_state_change_nothing() {
    let alice = Identity::generate();
    let mut store = Store::open(&temp_db_path(), &alice).unwrap();
    assert!(matches!(
        store.import_link_state(&[0xFF]),
        Err(Error::InvalidSetting)
    ));
    assert_eq!(
        store.blocked_peers().unwrap(),
        [] as [std::string::String; 0]
    );
}
