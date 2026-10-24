//! Like, unlike and view events: a replay ledger and the latest like state per direction.

use super::{Error, Store};
use crate::interactions::{Interaction, Kind};
use rusqlite::{params, OptionalExtension};

/// The latest like state in both directions with one peer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Likes {
    /// This profile likes the peer.
    pub ours: bool,
    /// The peer likes this profile.
    pub theirs: bool,
}

impl Likes {
    /// A match is a like in both directions.
    pub const fn matched(self) -> bool {
        self.ours && self.theirs
    }
}

impl Store {
    /// Apply one verified like, unlike or view exactly once. Exactly one of `sender` and `target`
    /// must be this profile. The latest like state per direction is ordered by
    /// `(HLC, device, event ID)`, so an unlike stays a tombstone against an older like.
    /// Returns whether the event became the latest like state; a replay or view returns false.
    pub fn apply_interaction(
        &mut self,
        sender: &str,
        target: &str,
        device: &[u8],
        event: Interaction,
    ) -> Result<bool, Error> {
        if (sender == self.owner_peer_id) == (target == self.owner_peer_id) {
            return Err(Error::InteractionConflict);
        }
        let hlc = i64::try_from(event.hlc).map_err(|_| Error::ClockRange)?;
        let payload = event.encode();
        let tx = self.db.transaction()?;
        let inserted = tx.execute(
            "INSERT INTO `interaction_ledger` (`sender`, `event_id`, `target`, `device`, `payload`)
             VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT (`sender`, `event_id`) DO NOTHING",
            params![
                sender,
                event.event_id.as_slice(),
                target,
                device,
                payload.as_slice()
            ],
        )?;
        if inserted == 0 {
            let same: bool = tx.query_row(
                "SELECT `target` = ?3 AND `device` = ?4 AND `payload` = ?5
                 FROM `interaction_ledger` WHERE `sender` = ?1 AND `event_id` = ?2",
                params![
                    sender,
                    event.event_id.as_slice(),
                    target,
                    device,
                    payload.as_slice()
                ],
                |row| row.get(0),
            )?;
            return if same {
                Ok(false)
            } else {
                Err(Error::InteractionConflict)
            };
        }
        let liked = match event.kind {
            Kind::Like => true,
            Kind::Unlike => false,
            Kind::View => {
                tx.commit()?;
                return Ok(false);
            }
        };
        let changed = tx.execute(
            "INSERT INTO `like_state` (`sender`, `target`, `liked`, `hlc`, `device`, `event_id`)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) ON CONFLICT (`sender`, `target`) DO UPDATE SET
             `liked` = excluded.`liked`, `hlc` = excluded.`hlc`, `device` = excluded.`device`,
             `event_id` = excluded.`event_id`
             WHERE (excluded.`hlc`, excluded.`device`, excluded.`event_id`)
                 > (`like_state`.`hlc`, `like_state`.`device`, `like_state`.`event_id`)",
            params![
                sender,
                target,
                i64::from(liked),
                hlc,
                device,
                event.event_id.as_slice()
            ],
        )?;
        tx.commit()?;
        Ok(changed != 0)
    }

    /// The latest like state in both directions between this profile and `peer_id`.
    pub fn likes(&self, peer_id: &str) -> Result<Likes, Error> {
        let liked = |sender: &str, target: &str| -> Result<bool, Error> {
            let liked: Option<i64> = self
                .db
                .query_row(
                    "SELECT `liked` FROM `like_state` WHERE `sender` = ?1 AND `target` = ?2",
                    params![sender, target],
                    |row| row.get(0),
                )
                .optional()?;
            Ok(liked == Some(1))
        };
        Ok(Likes {
            ours: liked(&self.owner_peer_id, peer_id)?,
            theirs: liked(peer_id, &self.owner_peer_id)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Likes;
    use crate::interactions::{Interaction, Kind};
    use crate::store::tests::temp_db_path;
    use crate::store::{Error, Store};
    use dyapp_identity::Identity;
    use dyapp_messaging::hlc::Timestamp;
    use rusqlite::Connection;

    fn event(kind: Kind, id: u8, hlc: u64) -> Interaction {
        Interaction {
            kind,
            event_id: [id; 16],
            hlc,
        }
    }

    fn put(
        store: &mut Store,
        sender: &str,
        target: &str,
        device: &[u8],
        event: Interaction,
    ) -> bool {
        store
            .apply_interaction(sender, target, device, event)
            .unwrap()
    }

    #[test]
    fn interactions_order_likes_and_derive_matches_across_restart() {
        let path = temp_db_path();
        let alice = Identity::generate();
        let (a, b) = (alice.peer_id(), Identity::generate().peer_id());
        let mut store = Store::open(&path, &alice).unwrap();
        assert!(put(&mut store, &a, &b, b"a1", event(Kind::Like, 1, 10)));
        assert!(!store.likes(&b).unwrap().matched());
        assert!(put(&mut store, &b, &a, b"b1", event(Kind::Like, 2, 5)));
        assert!(store.likes(&b).unwrap().matched());
        assert!(store.set_pass(&b, true, Timestamp::new(20, 0)).unwrap());
        assert!(store.likes(&b).unwrap().matched());

        // An unlike that arrives before its older like stays a tombstone, replays included.
        assert!(put(&mut store, &b, &a, b"b1", event(Kind::Unlike, 3, 30)));
        assert!(!put(&mut store, &b, &a, b"b1", event(Kind::Like, 4, 20)));
        assert!(!put(&mut store, &b, &a, b"b1", event(Kind::Like, 4, 20)));
        assert!(!put(&mut store, &b, &a, b"b1", event(Kind::Unlike, 3, 30)));
        assert_eq!(
            store.likes(&b).unwrap(),
            Likes {
                ours: true,
                theirs: false
            }
        );
        drop(store);

        let mut store = Store::open(&path, &alice).unwrap();
        assert!(!store.likes(&b).unwrap().matched());
        // Equal HLC values tie by device ID, then by event ID.
        assert!(put(&mut store, &b, &a, b"b2", event(Kind::Like, 5, 30)));
        assert!(!put(&mut store, &b, &a, b"b0", event(Kind::Unlike, 9, 30)));
        assert!(put(&mut store, &b, &a, b"b2", event(Kind::Unlike, 6, 30)));
        assert!(!store.likes(&b).unwrap().theirs);
        assert!(put(&mut store, &b, &a, b"b1", event(Kind::Like, 7, 31)));
        assert!(store.likes(&b).unwrap().matched());
        assert!(!put(&mut store, &b, &a, b"b1", event(Kind::View, 8, 99)));
        assert!(store.likes(&b).unwrap().matched());
    }

    #[test]
    fn conflicting_or_foreign_interactions_change_nothing() {
        let alice = Identity::generate();
        let (a, b, c) = (
            alice.peer_id(),
            Identity::generate().peer_id(),
            Identity::generate().peer_id(),
        );
        let mut store = Store::open(&temp_db_path(), &alice).unwrap();
        assert!(put(&mut store, &b, &a, b"b1", event(Kind::Like, 1, 10)));
        for (sender, target, device, kind) in [
            (b.as_str(), a.as_str(), b"b1", Kind::Unlike),
            (b.as_str(), a.as_str(), b"b2", Kind::Like),
            (b.as_str(), c.as_str(), b"b1", Kind::Like),
        ] {
            assert!(matches!(
                store.apply_interaction(sender, target, device, event(kind, 1, 10)),
                Err(Error::InteractionConflict)
            ));
        }
        for (sender, target) in [(&b, &c), (&a, &a)] {
            assert!(matches!(
                store.apply_interaction(sender, target, b"x1", event(Kind::Like, 2, 50)),
                Err(Error::InteractionConflict)
            ));
        }
        assert!(matches!(
            store.apply_interaction(&b, &a, b"b1", event(Kind::Unlike, 3, u64::MAX)),
            Err(Error::ClockRange)
        ));
        assert!(store.likes(&b).unwrap().theirs);
        assert!(!store.likes(&c).unwrap().theirs);
    }

    #[test]
    fn devices_converge_on_the_same_events_in_any_order() {
        let alice = Identity::generate();
        let (a, b) = (alice.peer_id(), Identity::generate().peer_id());
        let events = [
            (&b, &a, b"b1", event(Kind::Like, 1, 10)),
            (&b, &a, b"b2", event(Kind::Unlike, 2, 10)),
            (&a, &b, b"a1", event(Kind::Like, 3, 12)),
            (&a, &b, b"a2", event(Kind::Unlike, 4, 11)),
            (&b, &a, b"b1", event(Kind::Like, 5, 9)),
        ];
        let mut results = Vec::new();
        for order in [[0, 1, 2, 3, 4], [4, 3, 2, 1, 0], [2, 4, 0, 3, 1]] {
            let mut store = Store::open(&temp_db_path(), &alice).unwrap();
            for index in order {
                let (sender, target, device, event) = events[index];
                put(&mut store, sender, target, device, event);
            }
            results.push(store.likes(&b).unwrap());
        }
        assert!(results.iter().all(|likes| *likes
            == Likes {
                ours: true,
                theirs: false
            }));
    }

    #[test]
    fn migrates_v2_database_to_interaction_state() {
        let path = temp_db_path();
        let alice = Identity::generate();
        drop(Store::open(&path, &alice).unwrap());
        let db = Connection::open(&path).unwrap();
        db.execute_batch(
            "DROP TABLE `interaction_ledger`; DROP TABLE `like_state`; PRAGMA user_version = 2;",
        )
        .unwrap();
        drop(db);
        let (a, b) = (alice.peer_id(), Identity::generate().peer_id());
        let mut store = Store::open(&path, &alice).unwrap();
        assert!(put(&mut store, &a, &b, b"a1", event(Kind::Like, 1, 1)));
        assert!(store.likes(&b).unwrap().ours);
    }
}
