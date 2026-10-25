//! The profile publish window for search nodes, which take at most one profile update per key
//! every 10 minutes. The newest edit is held until the window ends, also across app restarts.

use super::{Error, Store};
use dyapp_identity::SignedRecord;
use prost::Message;
use rusqlite::{params, OptionalExtension};
use std::time::Duration;

const KEY: &str = "search_publish";
/// Seconds between two profile publishes to search.
const WINDOW: u64 = 600;

// `last_sent` and `hold_until` in Unix seconds, and the held record, if any.
#[derive(Default)]
struct State {
    last_sent: u64,
    hold_until: u64,
    pending: Option<SignedRecord>,
}

impl State {
    fn ready_at(&self) -> u64 {
        self.hold_until.max(self.last_sent.saturating_add(WINDOW))
    }
}

impl Store {
    /// Hold `record`, the newest signed profile, for search; it replaces any held one. The caller
    /// then sends [`Self::due_search_profile`], if any.
    pub fn hold_search_profile(&self, record: &SignedRecord) -> Result<(), Error> {
        let mut state = self.search_publish()?;
        state.pending = Some(record.clone());
        self.set_search_publish(&state)
    }

    /// The held profile once the window has ended at Unix second `now`; ask again after an edit
    /// and on app start.
    pub fn due_search_profile(&self, now: u64) -> Result<Option<SignedRecord>, Error> {
        let state = self.search_publish()?;
        let ready = now >= state.ready_at();
        Ok(state.pending.filter(|_| ready))
    }

    /// Record that search nodes accepted `record` at `now`. A newer edit made meanwhile stays
    /// held.
    pub fn search_profile_sent(&self, record: &SignedRecord, now: u64) -> Result<(), Error> {
        let mut state = self.search_publish()?;
        if state.pending.as_ref() == Some(record) {
            state.pending = None;
        }
        state.last_sent = now;
        self.set_search_publish(&state)
    }

    /// A search node answered `RATE_LIMITED` at `now`: keep the profile held for `retry_after`.
    pub fn search_profile_rate_limited(
        &self,
        now: u64,
        retry_after: Duration,
    ) -> Result<(), Error> {
        let mut state = self.search_publish()?;
        let wait = retry_after.as_secs() + u64::from(retry_after.subsec_nanos() > 0);
        state.hold_until = now.saturating_add(wait);
        self.set_search_publish(&state)
    }

    /// Minutes, rounded up, until held changes reach search, or `None` with nothing held.
    pub fn search_profile_minutes_left(&self, now: u64) -> Result<Option<u64>, Error> {
        let state = self.search_publish()?;
        let left = state.ready_at().saturating_sub(now).div_ceil(60);
        Ok(state.pending.map(|_| left))
    }

    fn search_publish(&self) -> Result<State, Error> {
        let value: Option<(i64, Vec<u8>)> = self
            .db
            .query_row(
                "SELECT `format_version`, `value` FROM `settings` WHERE `key` = ?1",
                [KEY],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((format, value)) = value else {
            return Ok(State::default());
        };
        if format != 1 || value.len() < 16 {
            return Err(Error::InvalidSetting);
        }
        let (times, record) = value.split_at(16);
        let pending = if record.is_empty() {
            None
        } else {
            Some(SignedRecord::decode(record).map_err(|_| Error::InvalidSetting)?)
        };
        Ok(State {
            last_sent: u64::from_be_bytes(times[..8].try_into().unwrap()),
            hold_until: u64::from_be_bytes(times[8..].try_into().unwrap()),
            pending,
        })
    }

    fn set_search_publish(&self, state: &State) -> Result<(), Error> {
        let mut value = [
            state.last_sent.to_be_bytes(),
            state.hold_until.to_be_bytes(),
        ]
        .concat();
        if let Some(record) = &state.pending {
            record.encode(&mut value).expect("a Vec grows");
        }
        self.db.execute(
            "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, 1, ?2)
             ON CONFLICT (`key`) DO UPDATE SET `format_version` = 1, `value` = excluded.`value`",
            params![KEY, value],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::store::tests::temp_db_path;
    use crate::store::{Error, Store};
    use dyapp_identity::{Identity, SignedRecord};
    use dyapp_profile::Profile;
    use std::time::Duration;

    fn profile(identity: &Identity, version: u64) -> SignedRecord {
        Profile {
            version,
            ..Profile::default()
        }
        .sign(identity)
    }

    #[test]
    fn holds_edits_within_the_window_and_merges_them_across_restart() {
        let (path, alice) = (temp_db_path(), Identity::generate());
        let store = Store::open(&path, &alice).unwrap();
        assert_eq!(store.search_profile_minutes_left(1000).unwrap(), None);
        let first = profile(&alice, 1);
        store.hold_search_profile(&first).unwrap();
        assert_eq!(store.due_search_profile(1000).unwrap(), Some(first.clone()));
        assert_eq!(store.search_profile_minutes_left(1000).unwrap(), Some(0));
        store.search_profile_sent(&first, 1000).unwrap();
        assert_eq!(store.due_search_profile(1000).unwrap(), None);

        for version in [2, 3] {
            store
                .hold_search_profile(&profile(&alice, version))
                .unwrap();
        }
        assert_eq!(store.due_search_profile(1599).unwrap(), None);
        assert_eq!(store.search_profile_minutes_left(1001).unwrap(), Some(10));
        assert_eq!(store.search_profile_minutes_left(1541).unwrap(), Some(1));
        drop(store);

        // Closed during the window: the newest edit is sent on the next start.
        let store = Store::open(&path, &alice).unwrap();
        let third = profile(&alice, 3);
        assert_eq!(store.due_search_profile(1600).unwrap(), Some(third.clone()));
        // An edit made while the third was in flight stays held.
        let fourth = profile(&alice, 4);
        store.hold_search_profile(&fourth).unwrap();
        store.search_profile_sent(&third, 1600).unwrap();
        assert_eq!(store.due_search_profile(2200).unwrap(), Some(fourth));
    }

    #[test]
    fn rate_limited_keeps_the_profile_held_for_retry_after() {
        let alice = Identity::generate();
        let store = Store::open(&temp_db_path(), &alice).unwrap();
        let first = profile(&alice, 1);
        store.hold_search_profile(&first).unwrap();
        let retry = Duration::from_millis(120_001);
        store.search_profile_rate_limited(1000, retry).unwrap();
        assert_eq!(store.due_search_profile(1120).unwrap(), None);
        assert_eq!(store.search_profile_minutes_left(1000).unwrap(), Some(3));
        assert_eq!(store.due_search_profile(1121).unwrap(), Some(first.clone()));
        // Once sent, the 10-minute window counts from then.
        store.search_profile_sent(&first, 1121).unwrap();
        store.hold_search_profile(&profile(&alice, 2)).unwrap();
        assert_eq!(store.due_search_profile(1720).unwrap(), None);
        assert!(store.due_search_profile(1721).unwrap().is_some());
        // Whole seconds are not rounded up.
        store
            .search_profile_rate_limited(1700, Duration::from_secs(100))
            .unwrap();
        assert_eq!(store.due_search_profile(1799).unwrap(), None);
        assert!(store.due_search_profile(1800).unwrap().is_some());
    }

    #[test]
    fn rejects_a_corrupt_setting() {
        let alice = Identity::generate();
        let store = Store::open(&temp_db_path(), &alice).unwrap();
        store
            .db
            .execute(
                "INSERT INTO `settings` (`key`, `format_version`, `value`)
                 VALUES ('search_publish', 1, x'00')",
                [],
            )
            .unwrap();
        assert!(matches!(
            store.due_search_profile(0),
            Err(Error::InvalidSetting)
        ));
    }
}
