//! Coarse presence with matches: the local audience setting, when presence was last sent to each
//! peer and the latest presence received from it ([presence](../../../docs/design/presence.md)).

use super::likes::{load_hlc, save_hlc};
use super::{Error, Store};
use crate::PreparedEnvelope;
use dyapp_messaging::hlc::{HybridClock, Timestamp};
use rusqlite::{params, OptionalExtension};

/// A match is shown as active recently this long after its latest presence.
pub const PRESENCE_WINDOW_MS: u64 = 2 * 3_600_000;
/// At most one presence per match in this interval.
pub const PRESENCE_THROTTLE_MS: u64 = 3_600_000;
const SETTING: &str = "send_presence";

pub(super) const SCHEMA_V11: &str = r"
CREATE TABLE `presence` (
    `peer_id` TEXT PRIMARY KEY NOT NULL,
    `sent_ms` INTEGER CHECK (`sent_ms` >= 0),
    `received_hlc` INTEGER CHECK (`received_hlc` >= 0)
) STRICT;
PRAGMA user_version = 11;
";

impl Store {
    /// Whether presence goes to matches (the default) rather than nobody.
    pub fn send_presence(&self) -> Result<bool, Error> {
        let value: Option<(i64, Vec<u8>)> = self
            .db
            .query_row(
                "SELECT `format_version`, `value` FROM `settings` WHERE `key` = ?1",
                [SETTING],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        match value {
            None => Ok(true),
            Some((1, value)) if value == [0] => Ok(false),
            Some((1, value)) if value == [1] => Ok(true),
            _ => Err(Error::InvalidSetting),
        }
    }

    /// Persist whether presence goes to matches (`true`) or nobody.
    pub fn set_send_presence(&self, enabled: bool) -> Result<(), Error> {
        self.db.execute(
            "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, 1, ?2)
             ON CONFLICT (`key`) DO UPDATE SET `format_version` = 1, `value` = excluded.`value`",
            params![SETTING, [u8::from(enabled)]],
        )?;
        Ok(())
    }

    /// The HLC of a presence for `target` if one is due at `wall_ms`: the setting is on, the peer
    /// is an unblocked match and nothing was sent to it in the last hour. Changes nothing; commit
    /// the encrypted presence with [`Self::stage_presence`].
    pub fn draft_presence(&self, target: &str, wall_ms: u64) -> Result<Option<u64>, Error> {
        if !self.presence_allowed(target)? {
            return Ok(None);
        }
        let sent: Option<i64> = self
            .db
            .query_row(
                "SELECT `sent_ms` FROM `presence` WHERE `peer_id` = ?1",
                [target],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        let sent = sent
            .map(u64::try_from)
            .transpose()
            .map_err(|_| Error::ClockRange)?;
        if sent.is_some_and(|sent| wall_ms < sent.saturating_add(PRESENCE_THROTTLE_MS)) {
            return Ok(None);
        }
        Ok(Some(
            HybridClock::resume(load_hlc(&self.db)?)
                .now_at(wall_ms)
                .into(),
        ))
    }

    /// Commit a drafted presence, its send time and the encrypted outbound writes in one
    /// transaction. A stale draft or a peer no longer allowed changes nothing.
    pub fn stage_presence(
        &mut self,
        target: &str,
        hlc: u64,
        sealed_device: &[u8],
        writes: &[PreparedEnvelope],
    ) -> Result<(), Error> {
        if writes.is_empty() || !self.presence_allowed(target)? {
            return Err(Error::InteractionConflict);
        }
        let tx = self.db.transaction()?;
        let hlc = Timestamp::from(hlc);
        if hlc <= load_hlc(&tx)? {
            return Err(Error::InteractionConflict);
        }
        save_hlc(&tx, hlc)?;
        let sent = i64::try_from(hlc.wall_ms()).map_err(|_| Error::ClockRange)?;
        tx.execute(
            "INSERT INTO `presence` (`peer_id`, `sent_ms`) VALUES (?1, ?2)
             ON CONFLICT (`peer_id`) DO UPDATE SET `sent_ms` = excluded.`sent_ms`",
            params![target, sent],
        )?;
        Self::stage_outbound_tx(&tx, None, sealed_device, writes, false)?;
        tx.commit()?;
        Ok(())
    }

    /// Keep the latest presence from `sender` after the 60-second future-skew guard. Presence
    /// from a blocked peer or a non-match is dropped before its HLC is observed. Returns whether
    /// it became the latest.
    pub fn receive_presence(
        &mut self,
        sender: &str,
        hlc: u64,
        wall_ms: u64,
    ) -> Result<bool, Error> {
        if self.blocked(sender)? || !self.likes(sender)?.matched() {
            return Ok(false);
        }
        let tx = self.db.transaction()?;
        let observed = HybridClock::resume(load_hlc(&tx)?).observe_at(hlc.into(), wall_ms)?;
        save_hlc(&tx, observed)?;
        let value = i64::try_from(hlc).map_err(|_| Error::ClockRange)?;
        let changed = tx.execute(
            "INSERT INTO `presence` (`peer_id`, `received_hlc`) VALUES (?1, ?2)
             ON CONFLICT (`peer_id`) DO UPDATE SET `received_hlc` = excluded.`received_hlc`
             WHERE `presence`.`received_hlc` IS NULL
                OR excluded.`received_hlc` > `presence`.`received_hlc`",
            params![sender, value],
        )?;
        tx.commit()?;
        Ok(changed != 0)
    }

    /// Whether to show `peer_id` as active recently at `wall_ms`: an unblocked match whose latest
    /// presence is at most two hours old.
    pub fn active_recently(&self, peer_id: &str, wall_ms: u64) -> Result<bool, Error> {
        if self.blocked(peer_id)? || !self.likes(peer_id)?.matched() {
            return Ok(false);
        }
        let received: Option<i64> = self
            .db
            .query_row(
                "SELECT `received_hlc` FROM `presence` WHERE `peer_id` = ?1",
                [peer_id],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        let Some(received) = received else {
            return Ok(false);
        };
        let at = Timestamp::from(u64::try_from(received).map_err(|_| Error::ClockRange)?);
        Ok(wall_ms.saturating_sub(at.wall_ms()) <= PRESENCE_WINDOW_MS)
    }

    fn presence_allowed(&self, target: &str) -> Result<bool, Error> {
        Ok(target != self.owner_peer_id
            && self.send_presence()?
            && !self.blocked(target)?
            && self.likes(target)?.matched())
    }
}

#[cfg(test)]
mod tests {
    use super::{PRESENCE_THROTTLE_MS, PRESENCE_WINDOW_MS};
    use crate::interactions::{Interaction, Kind};
    use crate::prepare_put;
    use crate::store::tests::temp_db_path;
    use crate::store::{Error, Store};
    use dyapp_identity::Identity;
    use dyapp_messaging::hlc::Timestamp;

    const T: u64 = 1_800_000_000_000;

    fn like(store: &mut Store, sender: &str, target: &str, id: u8) {
        let event = Interaction {
            kind: Kind::Like,
            event_id: [id; 16],
            hlc: 1,
        };
        store
            .apply_interaction(sender, target, b"d", event)
            .unwrap();
    }

    fn hlc(wall_ms: u64) -> u64 {
        Timestamp::new(wall_ms, 0).into()
    }

    #[test]
    fn presence_goes_only_to_unblocked_matches_hourly_and_survives_restart() {
        let path = temp_db_path();
        let alice = Identity::generate();
        let (a, b) = (alice.peer_id(), Identity::generate().peer_id());
        let mut store = Store::open(&path, &alice).unwrap();
        assert!(store.send_presence().unwrap());
        assert_eq!(store.draft_presence(&b, T).unwrap(), None);
        like(&mut store, &a, &b, 1);
        assert_eq!(store.draft_presence(&b, T).unwrap(), None);
        like(&mut store, &b, &a, 2);
        let first = store.draft_presence(&b, T).unwrap().unwrap();
        let write = prepare_put(&[7; 32], vec![5]).unwrap();
        assert!(matches!(
            store.stage_presence(&b, first, b"", std::slice::from_ref(&write)),
            Err(Error::EmptyMlsState)
        ));
        assert!(store.draft_presence(&b, T).unwrap().is_some());
        store
            .stage_presence(&b, first, b"sealed", std::slice::from_ref(&write))
            .unwrap();
        assert!(matches!(
            store.stage_presence(&b, first, b"sealed", std::slice::from_ref(&write)),
            Err(Error::InteractionConflict)
        ));
        drop(store);

        let store = Store::open(&path, &alice).unwrap();
        let (hour, later) = (T + PRESENCE_THROTTLE_MS, T + 2 * PRESENCE_THROTTLE_MS);
        assert_eq!(store.draft_presence(&b, hour - 1).unwrap(), None);
        assert!(store.draft_presence(&b, hour).unwrap().is_some());
        store.set_send_presence(false).unwrap();
        assert!(!store.send_presence().unwrap());
        assert_eq!(store.draft_presence(&b, later).unwrap(), None);
        store.set_send_presence(true).unwrap();
        store.set_blocked(&b, true).unwrap();
        assert_eq!(store.draft_presence(&b, later).unwrap(), None);
    }

    #[test]
    fn received_presence_is_latest_from_unblocked_matches_and_expires() {
        let path = temp_db_path();
        let alice = Identity::generate();
        let (a, b) = (alice.peer_id(), Identity::generate().peer_id());
        let mut store = Store::open(&path, &alice).unwrap();
        // A non-match is dropped before its clock is observed.
        assert!(!store.receive_presence(&b, hlc(T + 3_600_000), T).unwrap());
        like(&mut store, &a, &b, 1);
        like(&mut store, &b, &a, 2);
        assert!(!store.active_recently(&b, T).unwrap());
        assert!(matches!(
            store.receive_presence(&b, hlc(T + 61_000), T),
            Err(Error::ClockSkew(_))
        ));
        assert!(store.receive_presence(&b, hlc(T), T).unwrap());
        assert!(!store.receive_presence(&b, hlc(T - 1), T).unwrap());
        drop(store);

        let mut store = Store::open(&path, &alice).unwrap();
        let edge = T + PRESENCE_WINDOW_MS;
        assert!(store.active_recently(&b, edge).unwrap());
        assert!(!store.active_recently(&b, edge + 1).unwrap());
        store.set_blocked(&b, true).unwrap();
        assert!(!store.active_recently(&b, T).unwrap());
        assert!(!store.receive_presence(&b, hlc(T + 1), T + 1).unwrap());
    }
}
