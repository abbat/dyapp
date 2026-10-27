//! Like, unlike and view events: a replay ledger and the latest like state per direction.

use super::{Error, Store};
use crate::interactions::{Interaction, Kind};
use crate::PreparedEnvelope;
use dyapp_messaging::hlc::{HybridClock, Timestamp};
use rusqlite::{params, Connection, OptionalExtension, Transaction};

const DAY_MS: u64 = 86_400_000;
const HLC_KEY: &str = "interaction_hlc";

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
    /// Choose an event without changing the store. The caller encrypts this exact event, then
    /// commits it with the sealed MLS state and prepared outbound writes via [`Self::stage_interaction`].
    /// A disabled or already-sent daily view returns `None`.
    pub fn draft_interaction(
        &self,
        target: &str,
        kind: Kind,
        wall_ms: u64,
    ) -> Result<Option<Interaction>, Error> {
        if target == self.owner_peer_id {
            return Err(Error::InteractionConflict);
        }
        if kind == Kind::View && !self.send_profile_views()? {
            return Ok(None);
        }
        let hlc = HybridClock::resume(load_hlc(&self.db)?).now_at(wall_ms);
        if kind == Kind::View && self.viewed_on(&self.owner_peer_id, target, hlc.wall_ms())? {
            return Ok(None);
        }
        let mut event_id = [0; 16];
        getrandom::fill(&mut event_id).map_err(|_| Error::Random)?;
        Ok(Some(Interaction {
            kind,
            event_id,
            hlc: hlc.into(),
        }))
    }

    /// Commit an own event and the exact encrypted outbound signal in one transaction. A stale
    /// draft, duplicate daily view or failed outbox write leaves every local state unchanged.
    pub fn stage_interaction(
        &mut self,
        target: &str,
        device: &[u8],
        event: Interaction,
        sealed_device: &[u8],
        writes: &[PreparedEnvelope],
    ) -> Result<(), Error> {
        if target == self.owner_peer_id
            || writes.is_empty()
            || (event.kind == Kind::View && !self.send_profile_views()?)
        {
            return Err(Error::InteractionConflict);
        }
        let tx = self.db.transaction()?;
        let hlc = Timestamp::from(event.hlc);
        if hlc <= load_hlc(&tx)? {
            return Err(Error::InteractionConflict);
        }
        let owner = self.owner_peer_id.as_str();
        if event.kind == Kind::View && !record_view(&tx, owner, target, hlc)? {
            return Err(Error::InteractionConflict);
        }
        save_hlc(&tx, hlc)?;
        Self::apply(&tx, owner, owner, target, device, event)?;
        Self::stage_outbound_tx(&tx, None, sealed_device, writes, true)?;
        tx.commit()?;
        Ok(())
    }

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
        let tx = self.db.transaction()?;
        let changed = Self::apply(&tx, &self.owner_peer_id, sender, target, device, event)?;
        tx.commit()?;
        Ok(changed)
    }

    /// Test-only local event helper. Production sends use [`Self::draft_interaction`] and
    /// [`Self::stage_interaction`] so event state and encrypted outbox are committed together.
    #[cfg(test)]
    pub fn interact(
        &mut self,
        target: &str,
        kind: Kind,
        device: &[u8],
        wall_ms: u64,
    ) -> Result<Option<Interaction>, Error> {
        if kind == Kind::View && !self.send_profile_views()? {
            return Ok(None);
        }
        let mut event_id = [0; 16];
        getrandom::fill(&mut event_id).map_err(|_| Error::Random)?;
        let tx = self.db.transaction()?;
        let hlc = HybridClock::resume(load_hlc(&tx)?).now_at(wall_ms);
        save_hlc(&tx, hlc)?;
        if kind == Kind::View && !record_view(&tx, &self.owner_peer_id, target, hlc)? {
            return Ok(None);
        }
        let event = Interaction {
            kind,
            event_id,
            hlc: hlc.into(),
        };
        let owner = self.owner_peer_id.as_str();
        Self::apply(&tx, owner, owner, target, device, event)?;
        tx.commit()?;
        Ok(Some(event))
    }

    /// Apply an event `sender` sent to this profile from `device` after the 60-second
    /// future-skew guard; a rejected timestamp changes nothing. A view is recorded for its UTC
    /// day whatever the local send setting. Returns what [`Self::apply_interaction`] returns;
    /// an event from a blocked sender is dropped, clock included, and returns false.
    pub fn receive_interaction(
        &mut self,
        sender: &str,
        device: &[u8],
        event: Interaction,
        wall_ms: u64,
    ) -> Result<bool, Error> {
        if self.blocked(sender)? {
            return Ok(false);
        }
        let tx = self.db.transaction()?;
        let hlc = Timestamp::from(event.hlc);
        let clock = HybridClock::resume(load_hlc(&tx)?);
        save_hlc(&tx, clock.observe_at(hlc, wall_ms)?)?;
        let owner = self.owner_peer_id.as_str();
        let changed = Self::apply(&tx, owner, sender, owner, device, event)?;
        if event.kind == Kind::View {
            record_view(&tx, sender, owner, hlc)?;
        }
        tx.commit()?;
        Ok(changed)
    }

    /// Whether `sender` viewed `target` on the UTC day of `wall_ms`.
    pub fn viewed_on(&self, sender: &str, target: &str, wall_ms: u64) -> Result<bool, Error> {
        Ok(self.db.query_row(
            "SELECT EXISTS (SELECT 1 FROM `view_days`
             WHERE `sender` = ?1 AND `target` = ?2 AND `day` = ?3)",
            params![sender, target, utc_day(wall_ms)?],
            |row| row.get(0),
        )?)
    }

    /// Apply one event inside `tx`; see [`Self::apply_interaction`].
    fn apply(
        tx: &Transaction<'_>,
        owner: &str,
        sender: &str,
        target: &str,
        device: &[u8],
        event: Interaction,
    ) -> Result<bool, Error> {
        if (sender == owner) == (target == owner) {
            return Err(Error::InteractionConflict);
        }
        let hlc = i64::try_from(event.hlc).map_err(|_| Error::ClockRange)?;
        let payload = event.encode();
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
            Kind::View => return Ok(false),
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

/// The last own or observed private interaction or chat operation HLC; absent means zero.
pub(super) fn load_hlc(db: &Connection) -> Result<Timestamp, Error> {
    let value: Option<(i64, Vec<u8>)> = db
        .query_row(
            "SELECT `format_version`, `value` FROM `settings` WHERE `key` = ?1",
            [HLC_KEY],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    match value {
        None => Ok(Timestamp::default()),
        Some((1, value)) => {
            Ok(u64::from_be_bytes(value.try_into().map_err(|_| Error::InvalidSetting)?).into())
        }
        Some(_) => Err(Error::InvalidSetting),
    }
}

pub(super) fn save_hlc(tx: &Transaction<'_>, hlc: Timestamp) -> Result<(), Error> {
    tx.execute(
        "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, 1, ?2)
         ON CONFLICT (`key`) DO UPDATE SET `format_version` = 1, `value` = excluded.`value`",
        params![HLC_KEY, u64::from(hlc).to_be_bytes()],
    )?;
    Ok(())
}

/// Record that `sender` viewed `target` on the UTC day of `hlc`; false if already recorded.
fn record_view(
    tx: &Transaction<'_>,
    sender: &str,
    target: &str,
    hlc: Timestamp,
) -> Result<bool, Error> {
    Ok(tx.execute(
        "INSERT INTO `view_days` (`sender`, `target`, `day`) VALUES (?1, ?2, ?3)
         ON CONFLICT DO NOTHING",
        params![sender, target, utc_day(hlc.wall_ms())?],
    )? != 0)
}

fn utc_day(wall_ms: u64) -> Result<i64, Error> {
    i64::try_from(wall_ms / DAY_MS).map_err(|_| Error::ClockRange)
}

#[cfg(test)]
mod tests {
    use super::Likes;
    use crate::interactions::{Interaction, Kind};
    use crate::store::tests::temp_db_path;
    use crate::store::{Error, Store};
    use crate::{prepare_put, PreparedEnvelope};
    use dyapp_identity::{Domain, Identity, SignedRecord};
    use dyapp_messaging::hlc::Timestamp;
    use dyapp_p2p_net::proto;
    use prost::Message as _;
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
    fn staged_interaction_rolls_back_with_outbox_and_survives_restart() {
        let path = temp_db_path();
        let alice = Identity::generate();
        let bob = Identity::generate().peer_id();
        let mut store = Store::open(&path, &alice).unwrap();
        let write = prepare_put(&[7; 32], vec![5]).unwrap();
        let first = store
            .draft_interaction(&bob, Kind::Like, 1_000)
            .unwrap()
            .unwrap();
        assert!(matches!(
            store.stage_interaction(&bob, b"device", first, b"", std::slice::from_ref(&write)),
            Err(Error::EmptyMlsState)
        ));
        assert!(!store.likes(&bob).unwrap().ours);
        assert!(store.due_outbound(0).unwrap().is_empty());
        assert!(store.sealed_device().unwrap().is_none());
        let retry = store
            .draft_interaction(&bob, Kind::Like, 1_000)
            .unwrap()
            .unwrap();
        assert_eq!(retry.hlc, first.hlc);
        store
            .stage_interaction(
                &bob,
                b"device",
                retry,
                b"sealed",
                std::slice::from_ref(&write),
            )
            .unwrap();
        let unlike = store
            .draft_interaction(&bob, Kind::Unlike, 1_000)
            .unwrap()
            .unwrap();
        let record = SignedRecord::decode(write.as_bytes()).unwrap();
        let mut envelope = proto::Envelope::decode(record.payload.as_slice()).unwrap();
        envelope.ciphertext = vec![9];
        let conflicting = Identity::generate().sign(Domain::Envelope, envelope.encode_to_vec());
        let conflicting = PreparedEnvelope::from_bytes(conflicting.encode_to_vec()).unwrap();
        assert!(matches!(
            store.stage_interaction(&bob, b"device", unlike, b"changed", &[conflicting]),
            Err(Error::OutboxConflict)
        ));
        assert!(store.likes(&bob).unwrap().ours);
        assert_eq!(store.sealed_device().unwrap(), Some(b"sealed".to_vec()));
        assert_eq!(
            store
                .draft_interaction(&bob, Kind::Unlike, 1_000)
                .unwrap()
                .unwrap()
                .hlc,
            unlike.hlc
        );
        drop(store);

        let mut store = Store::open(&path, &alice).unwrap();
        assert!(store.likes(&bob).unwrap().ours);
        assert_eq!(store.sealed_device().unwrap(), Some(b"sealed".to_vec()));
        assert_eq!(
            store.due_outbound(0).unwrap()[0].as_bytes(),
            write.as_bytes()
        );
        assert!(
            store
                .draft_interaction(&bob, Kind::Like, 1_000)
                .unwrap()
                .unwrap()
                .hlc
                > retry.hlc
        );
        assert!(matches!(
            store.stage_interaction(&bob, b"device", retry, b"sealed", &[write]),
            Err(Error::InteractionConflict)
        ));
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
            "DROP TABLE `interaction_ledger`; DROP TABLE `like_state`; DROP TABLE `view_days`;
             DROP TABLE `held_tokens`; DROP TABLE `issued_tokens`;
             ALTER TABLE `outbox` DROP COLUMN `awaits_receipt`;
             ALTER TABLE `outbox` DROP COLUMN `accepted_at_ms`;
             ALTER TABLE `outbox` DROP COLUMN `accepted_by`;
             ALTER TABLE `outbox` DROP COLUMN `last_accepted_ms`; DROP TABLE `peers`;
             DROP TABLE `presence`; PRAGMA user_version = 2;",
        )
        .unwrap();
        drop(db);
        let (a, b) = (alice.peer_id(), Identity::generate().peer_id());
        let mut store = Store::open(&path, &alice).unwrap();
        assert!(put(&mut store, &a, &b, b"a1", event(Kind::Like, 1, 1)));
        assert!(store.likes(&b).unwrap().ours);
    }

    fn hlc(wall_ms: u64, counter: u16) -> u64 {
        Timestamp::new(wall_ms, counter).into()
    }

    fn own(store: &mut Store, target: &str, kind: Kind, wall_ms: u64) -> Option<Interaction> {
        store.interact(target, kind, b"a1", wall_ms).unwrap()
    }

    #[test]
    fn own_clock_survives_restart_and_skewed_events_change_nothing() {
        let path = temp_db_path();
        let alice = Identity::generate();
        let b = Identity::generate().peer_id();
        let mut store = Store::open(&path, &alice).unwrap();
        let like = own(&mut store, &b, Kind::Like, 1_000).unwrap();
        assert_eq!(like.hlc, hlc(1_000, 0));
        drop(store);

        let mut store = Store::open(&path, &alice).unwrap();
        let unlike = own(&mut store, &b, Kind::Unlike, 500).unwrap();
        assert_eq!(unlike.hlc, hlc(1_000, 1));
        assert_ne!(unlike.event_id, like.event_id);
        assert!(!store.likes(&b).unwrap().ours);

        let ahead = event(Kind::Like, 1, hlc(61_001, 0));
        assert!(matches!(
            store.receive_interaction(&b, b"b1", ahead, 1_000),
            Err(Error::ClockSkew(_))
        ));
        assert!(!store.likes(&b).unwrap().theirs);
        assert_eq!(
            own(&mut store, &b, Kind::Like, 1_000).unwrap().hlc,
            hlc(1_000, 2)
        );

        let near = event(Kind::Like, 1, hlc(61_000, 0));
        assert!(store.receive_interaction(&b, b"b1", near, 1_000).unwrap());
        assert!(store.likes(&b).unwrap().matched());
        assert_eq!(
            own(&mut store, &b, Kind::Like, 2_000).unwrap().hlc,
            hlc(61_000, 2)
        );
    }

    #[test]
    fn a_blocked_sender_changes_nothing() {
        let alice = Identity::generate();
        let (a, b) = (alice.peer_id(), Identity::generate().peer_id());
        let mut store = Store::open(&temp_db_path(), &alice).unwrap();
        store.set_blocked(&b, true).unwrap();
        let like = event(Kind::Like, 1, hlc(5_000, 0));
        assert!(!store.receive_interaction(&b, b"b1", like, 5_000).unwrap());
        let view = event(Kind::View, 2, hlc(5_000, 0));
        assert!(!store.receive_interaction(&b, b"b1", view, 5_000).unwrap());
        assert_eq!(store.likes(&b).unwrap(), Likes::default());
        assert!(!store.viewed_on(&b, &a, 5_000).unwrap());
        assert_eq!(
            own(&mut store, &b, Kind::Like, 1_000).unwrap().hlc,
            hlc(1_000, 0)
        );
        store.set_blocked(&b, false).unwrap();
        assert!(store.receive_interaction(&b, b"b1", like, 5_000).unwrap());
    }

    #[test]
    fn views_are_sent_once_per_peer_per_utc_day_and_always_received() {
        const DAY: u64 = 86_400_000;
        let alice = Identity::generate();
        let (a, b, c) = (
            alice.peer_id(),
            Identity::generate().peer_id(),
            Identity::generate().peer_id(),
        );
        let mut store = Store::open(&temp_db_path(), &alice).unwrap();
        assert_eq!(own(&mut store, &b, Kind::View, DAY), None);
        assert!(!store.viewed_on(&a, &b, DAY).unwrap());

        store.set_send_profile_views(true).unwrap();
        assert!(own(&mut store, &b, Kind::View, DAY).is_some());
        assert_eq!(own(&mut store, &b, Kind::View, 2 * DAY - 1), None);
        assert!(own(&mut store, &c, Kind::View, 2 * DAY - 1).is_some());
        assert!(own(&mut store, &b, Kind::View, 2 * DAY).is_some());
        assert!(store.viewed_on(&a, &b, DAY).unwrap());
        assert!(store.viewed_on(&a, &b, 2 * DAY).unwrap());
        assert_eq!(store.likes(&b).unwrap(), Likes::default());

        store.set_send_profile_views(false).unwrap();
        let view = event(Kind::View, 1, hlc(3 * DAY, 0));
        assert!(!store.receive_interaction(&b, b"b1", view, 3 * DAY).unwrap());
        assert!(!store.receive_interaction(&b, b"b1", view, 3 * DAY).unwrap());
        assert!(store.viewed_on(&b, &a, 3 * DAY).unwrap());
        assert!(!store.viewed_on(&b, &a, 2 * DAY).unwrap());
    }
}
