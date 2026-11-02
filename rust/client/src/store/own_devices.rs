//! When this device first saw the owner's newest profile: the clock of the turns that keep two own
//! devices from adding a linked device at once.

use super::{Error, Store};
use rusqlite::{params, OptionalExtension};

const KEY: &str = "own_profile_seen";

impl Store {
    /// Seconds since this device first saw own profile `version` or a newer one, 0 the first time.
    pub fn own_profile_age(&self, version: u64, now: u64) -> Result<u64, Error> {
        let row: Option<(u32, Vec<u8>)> = self
            .db
            .query_row(
                "SELECT `format_version`, `value` FROM `settings` WHERE `key` = ?1",
                [KEY],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let seen = match row {
            None => None,
            Some((1, value)) => {
                let value: [u8; 16] = value.try_into().map_err(|_| Error::InvalidSetting)?;
                let (held, at) = value.split_at(8);
                Some((
                    u64::from_be_bytes(held.try_into().unwrap_or_default()),
                    u64::from_be_bytes(at.try_into().unwrap_or_default()),
                ))
            }
            Some(_) => return Err(Error::InvalidSetting),
        };
        if let Some((held, at)) = seen {
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
}

#[cfg(test)]
mod tests {
    use dyapp_identity::Identity;

    #[test]
    fn only_a_newer_profile_restarts_the_clock() {
        let identity = Identity::generate();
        let path = std::env::temp_dir().join(format!("dyapp-own-seen-{}.db", identity.peer_id()));
        let store = super::Store::open(&path, &identity).unwrap();
        assert_eq!(store.own_profile_age(3, 1_000).unwrap(), 0);
        assert_eq!(store.own_profile_age(3, 1_400).unwrap(), 400);
        assert_eq!(store.own_profile_age(2, 1_500).unwrap(), 500);
        assert_eq!(store.own_profile_age(4, 1_600).unwrap(), 0);
        assert_eq!(store.own_profile_age(4, 1_601).unwrap(), 1);
    }
}
