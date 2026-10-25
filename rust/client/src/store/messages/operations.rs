//! Verified message operations, including bounded work received before its original.

use super::MessageKey;
use crate::chat::{Body, MAX_TEXT_BYTES};
use crate::store::likes::{load_hlc, save_hlc};
use crate::store::{Error, Store};
use dyapp_identity::sha256;
use dyapp_messaging::hlc::{HybridClock, Timestamp};
use rusqlite::{params, OptionalExtension, Transaction};

/// Most unresolved operations this profile retains across all target IDs.
pub const MAX_PENDING_OPERATIONS: i64 = 64;
type PendingEdit = (i64, Vec<u8>, Vec<u8>, Vec<u8>);

type OrderRow = (
    i64,
    Option<Vec<u8>>,
    Option<i64>,
    Option<Vec<u8>>,
    Option<Vec<u8>>,
);

impl Store {
    /// Apply an MLS-authenticated author's edit or delete. The caller
    /// supplies the verified sender device, not an untrusted body field. A replay is a no-op;
    /// reuse of an operation ID with other bytes or another device is a conflict. A deletion is
    /// terminal even against a later edit. Unknown originals consume a bounded pending slot.
    pub fn apply_chat_operation(
        &mut self,
        key: &MessageKey,
        device: &[u8],
        body: &Body,
        wall_ms: u64,
    ) -> Result<bool, Error> {
        let tx = self.db.transaction()?;
        let changed = Self::apply_chat_operation_tx(&tx, key, device, body, wall_ms)?;
        tx.commit()?;
        Ok(changed)
    }

    pub(super) fn apply_chat_operation_tx(
        tx: &Transaction<'_>,
        key: &MessageKey,
        device: &[u8],
        body: &Body,
        wall_ms: u64,
    ) -> Result<bool, Error> {
        let (target, operation, hlc, replacement) = match body {
            Body::Edit {
                target,
                operation,
                hlc,
                text,
            } => (target, operation, *hlc, Some(text.as_slice())),
            Body::Delete {
                target,
                operation,
                hlc,
            } => (target, operation, *hlc, None),
            _ => return Err(Error::InboxConflict),
        };
        if key.group.is_empty()
            || key.author.is_empty()
            || device.len() != 32
            || target != &key.id
            || replacement.is_some_and(|text| text.len() > MAX_TEXT_BYTES)
        {
            return Err(Error::InboxConflict);
        }
        // Observe the verified operation in the shared clock in this transaction. A rollback
        // also rolls back the clock, so an invalid envelope cannot advance future drafts.
        let observed =
            HybridClock::resume(load_hlc(tx)?).observe_at(Timestamp::from(hlc), wall_ms)?;
        save_hlc(tx, observed)?;
        let hlc = i64::try_from(hlc).map_err(|_| Error::ClockRange)?;
        let digest = sha256(&body.encode());
        let row: Option<OrderRow> = tx
            .query_row(
                "SELECT `deleted`, `original_digest`, `last_hlc`, `last_device`, `last_operation`
             FROM `chat_messages` WHERE `group_id` = ?1 AND `author` = ?2 AND `message_id` = ?3",
                params![&key.group, &key.author, key.id.as_slice()],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .optional()?;
        let pending = row
            .as_ref()
            .is_none_or(|(_, original, ..)| original.is_none());
        let tombstone = row.as_ref().is_some_and(|(deleted, ..)| *deleted == 1);
        let pending_text = if pending && !tombstone {
            replacement
        } else {
            None
        };
        let inserted = tx.execute(
            "INSERT INTO `chat_operations` (`group_id`, `author`, `operation_id`, `target_id`, `device`, `digest`, `hlc`, `pending`, `pending_text`)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT (`group_id`, `author`, `operation_id`) DO NOTHING",
            params![&key.group, &key.author, operation.as_slice(), target.as_slice(), device, digest.as_slice(), hlc, i64::from(pending), pending_text],
        )?;
        if inserted == 0 {
            let same: bool = tx.query_row(
                "SELECT `target_id` = ?4 AND `device` = ?5 AND `digest` = ?6
                 FROM `chat_operations` WHERE `group_id` = ?1 AND `author` = ?2 AND `operation_id` = ?3",
                params![&key.group, &key.author, operation.as_slice(), target.as_slice(), device, digest.as_slice()],
                |row| row.get(0),
            )?;
            return if same {
                Ok(false)
            } else {
                Err(Error::InboxConflict)
            };
        }
        if pending {
            let pending_count: i64 = tx.query_row(
                "SELECT COUNT(*) FROM `chat_operations` WHERE `pending` = 1",
                [],
                |row| row.get(0),
            )?;
            if pending_count > MAX_PENDING_OPERATIONS {
                return Err(Error::InboxConflict);
            }
            if row.is_none() && replacement.is_none() {
                tx.execute(
                    "INSERT INTO `chat_messages` (`group_id`, `author`, `message_id`, `text`, `original_digest`, `edited`, `deleted`, `last_hlc`, `last_device`, `last_operation`)
                     VALUES (?1, ?2, ?3, NULL, NULL, 0, 1, ?4, ?5, ?6)",
                    params![&key.group, &key.author, key.id.as_slice(), hlc, device, operation.as_slice()],
                )?;
            }
            if replacement.is_none() || tombstone {
                // A deletion makes all earlier pending replacement text unnecessary.
                tx.execute(
                    "UPDATE `chat_operations` SET `pending_text` = NULL
                     WHERE `group_id` = ?1 AND `author` = ?2 AND `target_id` = ?3 AND `pending` = 1",
                    params![&key.group, &key.author, key.id.as_slice()],
                )?;
            }
            return Ok(true);
        }
        let (deleted, _, last_hlc, last_device, last_operation) =
            row.ok_or(Error::InboxConflict)?;
        if deleted == 1 {
            return Ok(false);
        }
        if deleted != 0 {
            return Err(Error::InboxConflict);
        }
        let last = match (last_hlc, last_device, last_operation) {
            (None, None, None) => None,
            (Some(h), Some(d), Some(o)) if o.len() == 16 => Some((h, d, o)),
            _ => return Err(Error::InboxConflict),
        };
        if replacement.is_some()
            && last.as_ref().is_some_and(|(h, d, o)| {
                (hlc, device, operation.as_slice()) <= (*h, d.as_slice(), o.as_slice())
            })
        {
            return Ok(false);
        }
        match replacement {
            Some(text) => tx.execute(
                "UPDATE `chat_messages` SET `text` = ?4, `edited` = 1, `last_hlc` = ?5,
                 `last_device` = ?6, `last_operation` = ?7
                 WHERE `group_id` = ?1 AND `author` = ?2 AND `message_id` = ?3",
                params![
                    &key.group,
                    &key.author,
                    key.id.as_slice(),
                    text,
                    hlc,
                    device,
                    operation.as_slice()
                ],
            )?,
            None => tx.execute(
                "UPDATE `chat_messages` SET `text` = NULL, `deleted` = 1, `last_hlc` = ?4,
                 `last_device` = ?5, `last_operation` = ?6
                 WHERE `group_id` = ?1 AND `author` = ?2 AND `message_id` = ?3",
                params![
                    &key.group,
                    &key.author,
                    key.id.as_slice(),
                    hlc,
                    device,
                    operation.as_slice()
                ],
            )?,
        };
        Ok(true)
    }
}

/// Apply the greatest early edit after an original is durably inserted, then erase every
/// pending replacement. A preceding delete already made a terminal placeholder.
pub(super) fn resolve_pending(
    tx: &Transaction<'_>,
    key: &MessageKey,
    deleted: bool,
) -> Result<(), Error> {
    if !deleted {
        let best: Option<PendingEdit> = tx
            .query_row(
                "SELECT `hlc`, `device`, `operation_id`, `pending_text` FROM `chat_operations`
             WHERE `group_id` = ?1 AND `author` = ?2 AND `target_id` = ?3
             AND `pending` = 1 AND `pending_text` IS NOT NULL
             ORDER BY `hlc` DESC, `device` DESC, `operation_id` DESC LIMIT 1",
                params![&key.group, &key.author, key.id.as_slice()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        if let Some((hlc, device, operation, text)) = best {
            tx.execute(
                "UPDATE `chat_messages` SET `text` = ?4, `edited` = 1, `last_hlc` = ?5,
                 `last_device` = ?6, `last_operation` = ?7
                 WHERE `group_id` = ?1 AND `author` = ?2 AND `message_id` = ?3",
                params![
                    &key.group,
                    &key.author,
                    key.id.as_slice(),
                    text,
                    hlc,
                    device,
                    operation
                ],
            )?;
        }
    }
    tx.execute(
        "UPDATE `chat_operations` SET `pending` = 0, `pending_text` = NULL
         WHERE `group_id` = ?1 AND `author` = ?2 AND `target_id` = ?3 AND `pending` = 1",
        params![&key.group, &key.author, key.id.as_slice()],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests;
