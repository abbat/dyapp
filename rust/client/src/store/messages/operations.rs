//! Verified message operations for originals already present in the local projection.

use super::MessageKey;
use crate::chat::{Body, MAX_TEXT_BYTES};
use crate::store::{Error, Store};
use dyapp_identity::sha256;
use dyapp_messaging::hlc::{HybridClock, Timestamp};
use rusqlite::{params, OptionalExtension};

type OrderRow = (i64, Option<i64>, Option<Vec<u8>>, Option<Vec<u8>>);

impl Store {
    /// Apply an MLS-authenticated author's edit or delete to an existing original. The caller
    /// supplies the verified sender device, not an untrusted body field. A replay is a no-op;
    /// reuse of an operation ID with other bytes or another device is a conflict. A deletion is
    /// terminal even against a later edit. Unknown originals are handled by a later checkpoint.
    pub fn apply_chat_operation(
        &mut self,
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
        // Reject future events before opening a write transaction or changing any state.
        HybridClock::new().observe_at(Timestamp::from(hlc), wall_ms)?;
        let hlc = i64::try_from(hlc).map_err(|_| Error::ClockRange)?;
        let digest = sha256(&body.encode());
        let tx = self.db.transaction()?;
        let row: Option<OrderRow> = tx
            .query_row(
                "SELECT `deleted`, `last_hlc`, `last_device`, `last_operation`
             FROM `chat_messages` WHERE `group_id` = ?1 AND `author` = ?2 AND `message_id` = ?3",
                params![&key.group, &key.author, key.id.as_slice()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        let (deleted, last_hlc, last_device, last_operation) = row.ok_or(Error::InboxConflict)?;
        let inserted = tx.execute(
            "INSERT INTO `chat_operations` (`group_id`, `author`, `operation_id`, `target_id`, `device`, `digest`)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT (`group_id`, `author`, `operation_id`) DO NOTHING",
            params![&key.group, &key.author, operation.as_slice(), target.as_slice(), device, digest.as_slice()],
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
        if deleted == 1 {
            tx.commit()?;
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
            tx.commit()?;
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
        tx.commit()?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests;
