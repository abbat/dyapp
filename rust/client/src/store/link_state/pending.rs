//! One pending link owns its exact private-state import progress.
use super::{progress, Error, Store, LINKED, PENDING};
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};

impl Store {
    /// Keeps a received record's `state` until [`crate::session::Session::finish_link`] ends: the
    /// mailbox no longer holds the record, so a failed step must not lose it.
    ///
    /// # Errors
    ///
    /// Returns an error if reading prior pending state or any atomic write fails.
    /// Exact pending bytes preserve import progress; a new/replaced link resets it.
    pub fn set_link_pending(&self, state: &[u8]) -> Result<(), Error> {
        let transaction = Transaction::new_unchecked(&self.db, TransactionBehavior::Immediate)?;
        if self.link_pending()?.as_deref() != Some(state) {
            progress::save(&self.db, state, false)?;
        }
        self.db.execute(
            "INSERT OR IGNORE INTO `settings` (`key`, `format_version`, `value`)
             VALUES (?1, 1, x'')",
            [LINKED],
        )?;
        self.db.execute(
            "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, 1, ?2)
             ON CONFLICT (`key`) DO UPDATE SET `format_version` = 1, `value` = excluded.`value`",
            params![PENDING, state],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// The state of a link this device has not finished.
    ///
    /// # Errors
    ///
    /// Returns an error if querying or decoding the pending row fails, or its
    /// format version is unsupported.
    pub fn link_pending(&self) -> Result<Option<Vec<u8>>, Error> {
        let row: Option<(i64, Vec<u8>)> = self
            .db
            .query_row(
                "SELECT `format_version`, `value` FROM `settings` WHERE `key` = ?1",
                [PENDING],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        match row {
            None => Ok(None),
            Some((1, state)) => Ok(Some(state)),
            Some(_) => Err(Error::InvalidSetting),
        }
    }

    /// Whether this device joined by a link, so holds no history from before that.
    ///
    /// # Errors
    ///
    /// Returns an error if querying or decoding the linked-device marker fails.
    pub fn is_linked_device(&self) -> Result<bool, Error> {
        Ok(self.db.query_row(
            "SELECT EXISTS (SELECT 1 FROM `settings` WHERE `key` = ?1)",
            [LINKED],
            |row| row.get(0),
        )?)
    }

    // A network completion may clear only its own already-applied private-state prefix.
    pub(crate) fn clear_completed_link(&self, state: &[u8]) -> Result<(), Error> {
        let transaction = Transaction::new_unchecked(&self.db, TransactionBehavior::Immediate)?;
        if self.link_pending()?.as_deref() != Some(state)
            || progress::read(&self.db, state)? != Some(true)
        {
            return Err(Error::LinkStateChanged);
        }
        self.clear_link_pending()?;
        transaction.commit()?;
        Ok(())
    }

    /// Ends a link: the state now lives in the tables it was imported to.
    ///
    /// # Errors
    ///
    /// Returns an error if atomically clearing pending state and its import progress fails.
    pub fn clear_link_pending(&self) -> Result<(), Error> {
        self.db.execute(
            "DELETE FROM `settings` WHERE `key` IN (?1, ?2)",
            [PENDING, progress::KEY],
        )?;
        Ok(())
    }
}
