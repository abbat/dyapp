//! Durable KeyPackage stock and local consume-before-reply allocation.

use super::{lock, storage_error, BootstrapStore};
use crate::error::Result;
use dyapp_identity::SignedRecord;
use dyapp_p2p_net::proto::KeyPackageRecord;
use prost::Message;
use rusqlite::{params, OptionalExtension, TransactionBehavior};

const MAX_ORDINARY: i64 = 20;
const MAX_BYTES: i64 = 512 * 1024;
const MAX_CONSUMED: i64 = 1024;

pub(super) const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS `keypackages` (
             `hash` BLOB NOT NULL,
             `owner` BLOB NOT NULL,
             `mailbox` BLOB NOT NULL,
             `record` BLOB NOT NULL,
             `slot` INTEGER NOT NULL,
             `last_resort` INTEGER NOT NULL,
             `revision` INTEGER NOT NULL,
             `expires` INTEGER NOT NULL,
             `consumed` INTEGER NOT NULL DEFAULT 0,
             `size` INTEGER NOT NULL,
             PRIMARY KEY (`owner`, `mailbox`, `hash`)
         );
         CREATE INDEX IF NOT EXISTS `keypackages_mailbox`
             ON `keypackages` (`owner`, `mailbox`, `slot`, `last_resort`, `consumed`, `expires`);
         CREATE INDEX IF NOT EXISTS `keypackages_expiry` ON `keypackages` (`expires`);";

impl BootstrapStore {
    /// Stores a signed package without resetting an earlier consume. False means quota is full.
    pub fn put_keypackage(
        &self,
        package: &KeyPackageRecord,
        hash: &[u8],
        slot: u8,
        record: &SignedRecord,
    ) -> Result<bool> {
        let owner = package.owner.as_slice();
        let mailbox = package.mailbox.as_slice();
        let last_resort = package.last_resort;
        let revision = package.revision;
        let expires = i64::try_from(package.expires).unwrap_or(i64::MAX);
        let bytes = record.encode_to_vec();
        let size = i64::try_from(bytes.len()).map_err(storage_error)?;
        let mut db = lock(&self.profiles)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        tx.execute(
            "DELETE FROM `keypackages` WHERE `expires` <= ?1",
            [chrono::Utc::now().timestamp()],
        )
        .map_err(storage_error)?;
        let exists: bool = tx
            .query_row("SELECT EXISTS(SELECT 1 FROM `keypackages` WHERE `owner` = ?1 AND `mailbox` = ?2 AND `hash` = ?3)", params![owner, mailbox, hash], |row| row.get(0))
            .map_err(storage_error)?;
        if exists {
            tx.commit().map_err(storage_error)?;
            return Ok(true);
        }
        if last_resort {
            let previous: Option<i64> = tx.query_row(
                "SELECT MAX(`revision`) FROM `keypackages` WHERE `owner` = ?1 AND `mailbox` = ?2 AND `last_resort` = 1",
                params![owner, mailbox],
                |row| row.get(0),
            ).map_err(storage_error)?;
            if previous.is_some_and(|previous| i64::from(revision) <= previous) {
                return Ok(false);
            }
            tx.execute(
                "UPDATE `keypackages` SET `consumed` = 1, `record` = X'', `size` = 0
                 WHERE `owner` = ?1 AND `mailbox` = ?2 AND `last_resort` = 1 AND `consumed` = 0",
                params![owner, mailbox],
            )
            .map_err(storage_error)?;
        }
        let (count, used): (i64, i64) = tx
            .query_row(
                "SELECT COUNT(*), COALESCE(SUM(`size`), 0) FROM `keypackages` WHERE `owner` = ?1 AND `mailbox` = ?2 AND `consumed` = 0",
                params![owner, mailbox],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(storage_error)?;
        let ordinary: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM `keypackages` WHERE `owner` = ?1 AND `mailbox` = ?2 AND `last_resort` = 0 AND `consumed` = 0",
                params![owner, mailbox],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        let consumed: i64 = tx
            .query_row("SELECT COUNT(*) FROM `keypackages` WHERE `owner` = ?1 AND `mailbox` = ?2 AND `consumed` = 1", params![owner, mailbox], |row| row.get(0))
            .map_err(storage_error)?;
        if used + size > MAX_BYTES
            || (!last_resort && ordinary >= MAX_ORDINARY)
            || count > MAX_ORDINARY
            || consumed >= MAX_CONSUMED
        {
            return Ok(false);
        }
        tx.execute(
            "INSERT INTO `keypackages` (`hash`, `owner`, `mailbox`, `record`, `slot`, `last_resort`, `revision`, `expires`, `size`)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![hash, owner, mailbox, bytes, i64::from(slot), i64::from(last_resort), i64::from(revision), expires, size],
        )
        .map_err(storage_error)?;
        tx.commit().map_err(storage_error)?;
        Ok(true)
    }

    /// Allocates once on this holder. The fallback record remains reusable.
    pub fn allocate_keypackage(
        &self,
        owner: &[u8],
        mailbox: &[u8],
        slot: u8,
        now: i64,
    ) -> Result<Option<SignedRecord>> {
        let mut db = lock(&self.profiles)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        tx.execute("DELETE FROM `keypackages` WHERE `expires` <= ?1", [now])
            .map_err(storage_error)?;
        let ordinary: Option<(Vec<u8>, Vec<u8>)> = tx
            .query_row(
                "SELECT `hash`, `record` FROM `keypackages`
                 WHERE `owner` = ?1 AND `mailbox` = ?2 AND `slot` = ?3 AND `last_resort` = 0 AND `consumed` = 0
                 ORDER BY `expires`, `hash` LIMIT 1",
                params![owner, mailbox, i64::from(slot)],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(storage_error)?;
        let bytes = if let Some((hash, bytes)) = ordinary {
            tx.execute("UPDATE `keypackages` SET `consumed` = 1, `record` = X'', `size` = 0 WHERE `owner` = ?1 AND `mailbox` = ?2 AND `hash` = ?3 AND `consumed` = 0", params![owner, mailbox, hash])
                .map_err(storage_error)?;
            Some(bytes)
        } else {
            tx.query_row(
                "SELECT `record` FROM `keypackages` WHERE `owner` = ?1 AND `mailbox` = ?2 AND `last_resort` = 1 AND `consumed` = 0
                 ORDER BY `revision` DESC LIMIT 1",
                params![owner, mailbox],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage_error)?
        };
        tx.commit().map_err(storage_error)?;
        bytes
            .map(|bytes: Vec<u8>| SignedRecord::decode(bytes.as_slice()).map_err(storage_error))
            .transpose()
    }

    /// Removes expired stock; called by the regular node cleanup.
    pub fn expire_keypackages(&self, now: i64) -> Result<usize> {
        lock(&self.profiles)?
            .execute("DELETE FROM `keypackages` WHERE `expires` <= ?1", [now])
            .map_err(storage_error)
    }
}
