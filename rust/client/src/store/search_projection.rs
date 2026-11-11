//! Serialized search queue updates and a durable version high-water mark.
use super::{search_publish::State, Error, Store};
use dyapp_identity::{sha256, Domain, SignedRecord};
use prost::Message;
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use std::time::Duration;

const HEAD_KEY: &str = "search_profile_head";

#[derive(Clone, Copy)]
pub(super) struct Head {
    version: u64,
    digest: [u8; 32],
}

impl Store {
    fn record_head(&self, record: &SignedRecord) -> Result<Head, Error> {
        let owner = record
            .verify(Domain::Profile)
            .map_err(|_| Error::InvalidSetting)?;
        if owner != self.owner_peer_id {
            return Err(Error::WrongIdentity);
        }
        Ok(Head {
            version: dyapp_profile::stored_version(record).map_err(|_| Error::InvalidSetting)?,
            digest: sha256(&record.encode_to_vec()),
        })
    }

    fn search_head(&self) -> Result<Option<Head>, Error> {
        let value: Option<(i64, Vec<u8>)> = self
            .db
            .query_row(
                "SELECT `format_version`, `value` FROM `settings` WHERE `key` = ?1",
                [HEAD_KEY],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((format, value)) = value else {
            return Ok(None);
        };
        if format != 1 || value.len() != 40 {
            return Err(Error::InvalidSetting);
        }
        let (version, digest) = value.split_first_chunk().ok_or(Error::InvalidSetting)?;
        Ok(Some(Head {
            version: u64::from_be_bytes(*version),
            digest: digest.try_into().map_err(|_| Error::InvalidSetting)?,
        }))
    }

    fn set_search_head(&self, head: Head) -> Result<(), Error> {
        let mut value = head.version.to_be_bytes().to_vec();
        value.extend(head.digest);
        self.db.execute(
            "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, 1, ?2)
             ON CONFLICT (`key`) DO UPDATE SET `format_version` = 1, `value` = excluded.`value`",
            params![HEAD_KEY, value],
        )?;
        Ok(())
    }

    pub(super) fn search_projection(&self) -> Result<(State, Option<Head>), Error> {
        // Both settings must come from one SQLite snapshot, including read-only callers.
        let snapshot = if self.db.is_autocommit() {
            Some(Transaction::new_unchecked(
                &self.db,
                TransactionBehavior::Deferred,
            )?)
        } else {
            None
        };
        let state = self.search_publish()?;
        let mut head = self.search_head()?;
        if let Some(record) = &state.pending {
            let pending = self.record_head(record)?;
            match head {
                Some(saved) if pending.version < saved.version => {
                    return Err(Error::InvalidSetting)
                }
                Some(saved)
                    if pending.version == saved.version && pending.digest != saved.digest =>
                {
                    return Err(Error::SearchProfileConflict)
                }
                Some(saved) if pending.version == saved.version => {}
                _ => head = Some(pending), // Derive the marker for legacy format-1 queues.
            }
        }
        if let Some(snapshot) = snapshot {
            snapshot.commit()?;
        }
        Ok((state, head))
    }

    fn update_search(
        &self,
        update: impl FnOnce(&mut State, &mut Option<Head>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let transaction = Transaction::new_unchecked(&self.db, TransactionBehavior::Immediate)?;
        let (mut state, mut head) = self.search_projection()?;
        update(&mut state, &mut head)?;
        self.set_search_publish(&state)?;
        if let Some(head) = head {
            self.set_search_head(head)?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Hold a same-owner signed profile, ignoring older or already acknowledged versions.
    ///
    /// # Errors
    /// Returns invalid records, conflicting equal versions or database errors.
    pub fn hold_search_profile(&self, record: &SignedRecord) -> Result<(), Error> {
        let transaction = Transaction::new_unchecked(&self.db, TransactionBehavior::Immediate)?;
        self.hold_search_profile_in_tx(record)?;
        transaction.commit()?;
        Ok(())
    }

    // Caller owns the writer transaction, so atomic hide needs no nested BEGIN.
    pub(super) fn hold_search_profile_in_tx(&self, record: &SignedRecord) -> Result<(), Error> {
        let incoming = self.record_head(record)?;
        let (mut state, head) = self.search_projection()?;
        if let Some(saved) = head {
            if incoming.version == saved.version {
                return if incoming.digest == saved.digest {
                    Ok(())
                } else {
                    Err(Error::SearchProfileConflict)
                };
            }
            if incoming.version < saved.version {
                return Ok(());
            }
        }
        if self
            .profile_draft()?
            .is_some_and(|draft| incoming.version < draft.version)
        {
            return Ok(());
        }
        state.pending = Some(record.clone());
        self.set_search_publish(&state)?;
        self.set_search_head(incoming)
    }

    pub(super) fn known_search_version(&self) -> Result<u64, Error> {
        Ok(self.search_projection()?.1.map_or(0, |head| head.version))
    }

    /// Record an accepted profile without clearing a newer pending version.
    ///
    /// # Errors
    /// Returns invalid records, equal-version conflicts or database errors.
    pub fn search_profile_sent(&self, record: &SignedRecord, now: u64) -> Result<(), Error> {
        let incoming = self.record_head(record)?;
        self.update_search(|state, head| {
            if let Some(saved) = *head {
                if incoming.version == saved.version && incoming.digest != saved.digest {
                    return Err(Error::SearchProfileConflict);
                }
                if incoming.version < saved.version {
                    state.last_sent = state.last_sent.max(now);
                    return Ok(());
                }
            }
            state.pending = None;
            *head = Some(incoming);
            state.last_sent = state.last_sent.max(now);
            Ok(())
        })
    }

    /// Extend the retry deadline without shortening a concurrent node's backoff.
    ///
    /// # Errors
    /// Returns malformed persisted state or database errors.
    pub fn search_profile_rate_limited(
        &self,
        now: u64,
        retry_after: Duration,
    ) -> Result<(), Error> {
        let seconds = retry_after
            .as_secs()
            .saturating_add(u64::from(retry_after.subsec_nanos() != 0));
        self.update_search(|state, _| {
            state.hold_until = state.hold_until.max(now.saturating_add(seconds));
            Ok(())
        })
    }
}

#[cfg(test)]
#[path = "search_projection_tests.rs"]
mod tests;
