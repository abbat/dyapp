//! Post-publication cleanup protects both remote references and the transaction-owned draft.
use super::profile_photos::{key, keys};
use super::{Error, Store};
use dyapp_profile::Profile;
use rusqlite::{Connection, Transaction, TransactionBehavior};
use std::collections::HashSet;

impl Store {
    /// Prune blobs unused by both the supplied published profile and the latest local draft.
    ///
    /// Reading current references and all deletions share SQLite writer ownership.
    ///
    /// # Errors
    /// Returns malformed draft/hash, transaction admission or database errors. Any failure
    /// rolls the whole cleanup back, rather than leaving earlier deletions applied.
    pub fn retain_profile_photos(&self, published: &[Vec<u8>]) -> Result<(), Error> {
        // This legacy &self API checks transaction nesting at BEGIN. Composition inside another
        // writer transaction uses retain() below instead of attempting a nested transaction.
        let transaction = Transaction::new_unchecked(&self.db, TransactionBehavior::Immediate)?;
        let draft = self.profile_draft()?.unwrap_or_default();
        retain(&transaction, published, &draft)?;
        transaction.commit()?;
        Ok(())
    }
}

/// Caller already owns an immediate writer transaction and its current draft.
pub(super) fn retain(db: &Connection, published: &[Vec<u8>], draft: &Profile) -> Result<(), Error> {
    let mut wanted = HashSet::new();
    let draft_hashes = draft
        .photos
        .iter()
        .flat_map(|photo| photo.full.iter().chain(std::iter::once(&photo.thumbnail)));
    for hash in published.iter().chain(draft_hashes) {
        if hash.len() != 32 {
            return Err(Error::InvalidSetting);
        }
        wanted.insert(key(hash));
    }
    for stored in keys(db)? {
        if !wanted.contains(&stored) {
            db.execute("DELETE FROM `settings` WHERE `key` = ?1", [stored])?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "profile_retention_tests.rs"]
mod tests;
