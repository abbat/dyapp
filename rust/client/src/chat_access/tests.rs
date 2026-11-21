use super::*;
use dyapp_identity::Identity;
use rusqlite::{params, Connection};

struct Fixture {
    store: Store,
    db: Connection,
    peer: String,
}

impl Fixture {
    fn new() -> Self {
        let path = crate::store::tests::temp_db_path();
        let owner = Identity::try_generate().unwrap();
        Self {
            store: Store::open(&path, &owner).unwrap(),
            db: Connection::open(path).unwrap(),
            peer: Identity::try_generate().unwrap().peer_id(),
        }
    }

    fn conversation(&self, state: i64) {
        let request = (state < 3).then_some(vec![7u8; 16]);
        self.db
            .execute(
                "INSERT INTO `conversations` (`peer_id`, `state`, `request_id`) VALUES (?1, ?2, ?3)
             ON CONFLICT (`peer_id`) DO UPDATE SET `state` = excluded.`state`,
                 `request_id` = excluded.`request_id`",
                params![&self.peer, state, request],
            )
            .unwrap();
    }
}

#[test]
fn target_binds_the_actual_owner_and_all_existing_conversation_phases_without_writes() {
    let f = Fixture::new();
    let draft = f.store.profile_draft().unwrap();
    for state in 1..=4 {
        f.conversation(state);
        let phase = f.store.conversation(&f.peer).unwrap();
        assert_eq!(
            target(&f.store, &f.peer).unwrap(),
            Target {
                owner_peer_id: f.store.owner_peer_id().to_owned(),
                peer_id: f.peer.clone(),
            }
        );
        assert_eq!(f.store.conversation(&f.peer).unwrap(), phase);
        assert_eq!(f.store.profile_draft().unwrap(), draft);
        assert!(f.store.due_outbound(0).unwrap().is_empty());
    }
}

#[test]
fn own_blocked_and_unknown_targets_keep_the_existing_refusals() {
    let f = Fixture::new();
    assert!(matches!(
        target(&f.store, f.store.owner_peer_id()),
        Err(Error::OwnPeer)
    ));
    assert!(matches!(target(&f.store, &f.peer), Err(Error::UnknownPeer)));
    assert!(matches!(target(&f.store, ""), Err(Error::UnknownPeer)));
    f.store.set_blocked(&f.peer, true).unwrap();
    assert!(matches!(target(&f.store, &f.peer), Err(Error::Blocked)));
    f.conversation(3);
    assert!(matches!(target(&f.store, &f.peer), Err(Error::Blocked)));
    f.store.set_blocked(&f.peer, false).unwrap();
    assert_eq!(target(&f.store, &f.peer).unwrap().peer_id, f.peer);
}

#[test]
fn unreadable_conversation_state_is_a_storage_failure_not_an_empty_or_repaired_target() {
    let f = Fixture::new();
    f.conversation(3);
    f.db.execute_batch("ALTER TABLE `conversations` RENAME TO `unavailable_conversations`")
        .unwrap();
    assert!(matches!(target(&f.store, &f.peer), Err(Error::Store(_))));
    f.db.execute_batch("ALTER TABLE `unavailable_conversations` RENAME TO `conversations`")
        .unwrap();
    assert_eq!(target(&f.store, &f.peer).unwrap().peer_id, f.peer);
}
