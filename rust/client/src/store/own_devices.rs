//! When this device first saw the owner's newest profile or a dropped device in a group: the clocks
//! of the turns that keep two devices from committing to a group at once.

use super::{Error, Store};
use rusqlite::{params, OptionalExtension};

const KEY: &str = "own_profile_seen";
const DROPPED: &str = "dropped_leaf_seen/";
const LISTED: &str = "device_listed";

impl Store {
    /// Seconds since this device first saw own profile `version` or a newer one, 0 the first time.
    ///
    /// # Errors
    ///
    /// Returns an error if the stored profile clock has an unsupported format or
    /// invalid length, or reading or updating the database fails.
    pub fn own_profile_age(&self, version: u64, now: u64) -> Result<u64, Error> {
        if let Some((held, at)) = self.own_profile_row()? {
            if held >= version {
                return Ok(now.saturating_sub(at));
            }
        }
        let value = [version.to_be_bytes(), now.to_be_bytes()].concat();
        self.db.execute(
            "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, 1, ?2)
             ON CONFLICT (`key`) DO UPDATE SET `format_version` = 1, `value` = excluded.`value`",
            params![KEY, value],
        )?;
        Ok(0)
    }

    /// The newest own profile version this device has seen, 0 before the first.
    ///
    /// # Errors
    ///
    /// Returns an error if reading the profile clock fails or its stored format
    /// or length is invalid.
    pub fn own_profile_seen(&self) -> Result<u64, Error> {
        Ok(self.own_profile_row()?.map_or(0, |(version, _)| version))
    }

    /// Records that own profile `version` lists this device; a lower one changes nothing.
    ///
    /// # Errors
    ///
    /// Returns an error if writing the listing version to the database fails.
    pub fn note_device_listed(&self, version: u64) -> Result<(), Error> {
        // Big-endian bytes compare as the numbers do.
        self.db.execute(
            "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, 1, ?2)
             ON CONFLICT (`key`) DO UPDATE SET `value` = excluded.`value`
             WHERE excluded.`value` > `settings`.`value`",
            params![LISTED, version.to_be_bytes()],
        )?;
        Ok(())
    }

    /// The newest own profile version known to list this device, 0 before the first.
    ///
    /// # Errors
    ///
    /// Returns an error if querying or decoding the listing version fails or its
    /// stored value has an invalid length.
    pub fn device_listed(&self) -> Result<u64, Error> {
        let held: Option<Vec<u8>> = self
            .db
            .query_row(
                "SELECT `value` FROM `settings` WHERE `key` = ?1",
                [LISTED],
                |row| row.get(0),
            )
            .optional()?;
        let Some(held) = held else { return Ok(0) };
        let held: [u8; 8] = held.try_into().map_err(|_| Error::InvalidSetting)?;
        Ok(u64::from_be_bytes(held))
    }

    /// Seconds since this device first saw a dropped device in `group`, 0 the first time.
    ///
    /// # Errors
    ///
    /// Returns an error if the stored timestamp has an invalid length, or reading
    /// or inserting the dropped-device clock fails.
    pub fn dropped_leaf_age(&self, group: &[u8], now: u64) -> Result<u64, Error> {
        let key = format!("{DROPPED}{}", crate::link::hex(group));
        let held: Option<Vec<u8>> = self
            .db
            .query_row(
                "SELECT `value` FROM `settings` WHERE `key` = ?1",
                [&key],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(held) = held {
            let at: [u8; 8] = held.try_into().map_err(|_| Error::InvalidSetting)?;
            return Ok(now.saturating_sub(u64::from_be_bytes(at)));
        }
        self.db.execute(
            "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, 1, ?2)",
            params![key, now.to_be_bytes()],
        )?;
        Ok(0)
    }

    /// Stops the clock of `group`: its next dropped device starts one of its own.
    ///
    /// # Errors
    ///
    /// Returns an error if deleting the dropped-device clock from the database fails.
    pub fn forget_dropped_leaf(&self, group: &[u8]) -> Result<(), Error> {
        self.db.execute(
            "DELETE FROM `settings` WHERE `key` = ?1",
            [format!("{DROPPED}{}", crate::link::hex(group))],
        )?;
        Ok(())
    }

    fn own_profile_row(&self) -> Result<Option<(u64, u64)>, Error> {
        let row: Option<(u32, Vec<u8>)> = self
            .db
            .query_row(
                "SELECT `format_version`, `value` FROM `settings` WHERE `key` = ?1",
                [KEY],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        match row {
            None => Ok(None),
            Some((1, value)) => {
                let value: [u8; 16] = value.try_into().map_err(|_| Error::InvalidSetting)?;
                let (held, at) = value.split_at(8);
                Ok(Some((
                    u64::from_be_bytes(held.try_into().unwrap_or_default()),
                    u64::from_be_bytes(at.try_into().unwrap_or_default()),
                )))
            }
            Some(_) => Err(Error::InvalidSetting),
        }
    }
}

#[cfg(test)]
mod tests {
    use dyapp_identity::Identity;

    #[test]
    fn only_a_newer_profile_restarts_the_clock() {
        let identity = Identity::generate();
        let path = std::env::temp_dir().join(format!("dyapp-own-seen-{}.db", identity.peer_id()));
        let store = super::Store::open(&path, &identity).unwrap();
        assert_eq!(store.own_profile_seen().unwrap(), 0);
        assert_eq!(store.own_profile_age(3, 1_000).unwrap(), 0);
        assert_eq!(store.own_profile_seen().unwrap(), 3);
        assert_eq!(store.own_profile_age(3, 1_400).unwrap(), 400);
        assert_eq!(store.own_profile_age(2, 1_500).unwrap(), 500);
        assert_eq!(store.own_profile_age(4, 1_600).unwrap(), 0);
        assert_eq!(store.own_profile_age(4, 1_601).unwrap(), 1);
        let (one, two) = (b"one".as_slice(), b"two".as_slice());
        assert_eq!(store.dropped_leaf_age(one, 2_000).unwrap(), 0);
        assert_eq!(store.dropped_leaf_age(two, 2_100).unwrap(), 0);
        assert_eq!(store.dropped_leaf_age(one, 2_500).unwrap(), 500);
        store.forget_dropped_leaf(one).unwrap();
        assert_eq!(store.dropped_leaf_age(one, 2_600).unwrap(), 0);
        assert_eq!(store.dropped_leaf_age(two, 2_600).unwrap(), 500);
        assert_eq!(store.device_listed().unwrap(), 0);
        store.note_device_listed(256).unwrap();
        store.note_device_listed(5).unwrap();
        assert_eq!(store.device_listed().unwrap(), 256);
        store.note_device_listed(257).unwrap();
        assert_eq!(store.device_listed().unwrap(), 257);
    }
}
