//! Persisted daily and MLS-gap trigger for the device's two-holder mailbox check.
use super::{Error, Store};
use rusqlite::{params, OptionalExtension};

const KEY: &str = "mailbox_cross_check_after";
const DAY: u64 = 24 * 60 * 60;

impl Store {
    /// Whether a cross-check is due. A missing row and a recorded gap are immediately due.
    ///
    /// # Errors
    /// Returns a storage error, or rejects a malformed or unsupported schedule record.
    pub fn mailbox_cross_check_due(&self, now: u64) -> Result<bool, Error> {
        let now = i64::try_from(now).map_err(|_| Error::ClockRange)?;
        let row: Option<(i64, Vec<u8>)> = self
            .db
            .query_row(
                "SELECT `format_version`, `value` FROM `settings` WHERE `key` = ?1",
                [KEY],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((1, value)) = row else {
            return Ok(row.is_none());
        };
        let bytes: [u8; 8] = value
            .as_slice()
            .try_into()
            .map_err(|_| Error::InvalidSetting)?;
        let after = i64::from_be_bytes(bytes);
        Ok(after <= now)
    }

    /// Keep or create an immediately due cross-check after an MLS gap.
    ///
    /// # Errors
    /// Returns an error when the schedule cannot be persisted.
    pub fn note_mailbox_gap(&self) -> Result<(), Error> {
        self.write_cross_check(0)
    }

    /// Schedule the next daily check only after both holders completed the current one.
    ///
    /// # Errors
    /// Returns a clock-range error or an error when the schedule cannot be persisted.
    pub fn mailbox_cross_checked(&self, now: u64) -> Result<(), Error> {
        let after = now.checked_add(DAY).ok_or(Error::ClockRange)?;
        self.write_cross_check(after)
    }

    fn write_cross_check(&self, after: u64) -> Result<(), Error> {
        let after = i64::try_from(after).map_err(|_| Error::ClockRange)?;
        self.db.execute(
            "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, 1, ?2)
             ON CONFLICT (`key`) DO UPDATE SET `format_version` = 1, `value` = excluded.`value`",
            params![KEY, after.to_be_bytes().as_slice()],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::tests::temp_db_path;
    use dyapp_identity::Identity;

    #[test]
    fn daily_and_gap_due_state_survives_restart() {
        let path = temp_db_path();
        let owner = Identity::try_generate().unwrap();
        let store = Store::open(&path, &owner).unwrap();
        assert!(store.mailbox_cross_check_due(100).unwrap());
        store.mailbox_cross_checked(100).unwrap();
        assert!(!store.mailbox_cross_check_due(100 + DAY - 1).unwrap());
        assert!(store.mailbox_cross_check_due(100 + DAY).unwrap());
        drop(store);

        let store = Store::open(&path, &owner).unwrap();
        store.mailbox_cross_checked(200).unwrap();
        store.note_mailbox_gap().unwrap();
        drop(store);
        let store = Store::open(&path, &owner).unwrap();
        assert!(store.mailbox_cross_check_due(200).unwrap());
        store.mailbox_cross_checked(200).unwrap();
        assert!(!store.mailbox_cross_check_due(200).unwrap());
    }

    #[test]
    fn malformed_schedule_fails_closed_as_an_error() {
        let store = Store::open(&temp_db_path(), &Identity::try_generate().unwrap()).unwrap();
        store
            .db
            .execute(
                "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, 2, ?2)",
                params![KEY, vec![0u8; 8]],
            )
            .unwrap();
        assert!(matches!(
            store.mailbox_cross_check_due(0),
            Err(Error::InvalidSetting)
        ));
    }
}
