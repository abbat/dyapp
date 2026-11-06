//! Stickers share the terminal placeholder and operation ledger used by text.

use super::{operations, Error, MessageKey};
use crate::chat::{sticker::Sticker, Body};
use dyapp_identity::sha256;
use rusqlite::{params, OptionalExtension, Transaction};

/// Convert a known sticker to a terminal placeholder before the normal ledger applies.
pub(super) fn delete(
    tx: &Transaction<'_>,
    key: &MessageKey,
    device: &[u8],
    body: &Body,
) -> Result<bool, Error> {
    let stored: Option<(i64, Vec<u8>)> = tx
        .query_row(
            "SELECT `original_hlc`, `descriptor` FROM `chat_attachments`
             WHERE `group_id` = ?1 AND `author` = ?2 AND `message_id` = ?3 AND `media_kind` = 2",
            params![&key.group, &key.author, key.id.as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((original_hlc, descriptor)) = stored else {
        return Ok(false);
    };
    let Body::Delete { operation, hlc, .. } = body else {
        return Err(Error::InboxConflict);
    };
    let sticker = Sticker::decode(&descriptor).map_err(|_| Error::InboxConflict)?;
    if sticker.id != key.id
        || i64::try_from(sticker.hlc).ok() != Some(original_hlc)
        || sticker.encode().map_err(|_| Error::InboxConflict)? != descriptor
    {
        return Err(Error::InboxConflict);
    }
    tx.execute(
        "INSERT INTO `chat_messages` (`group_id`, `author`, `message_id`, `text`,
         `original_digest`, `edited`, `deleted`, `original_hlc`, `last_hlc`,
         `last_device`, `last_operation`)
         VALUES (?1, ?2, ?3, NULL, ?4, 0, 1, ?5, ?6, ?7, ?8)",
        params![
            &key.group,
            &key.author,
            key.id.as_slice(),
            sha256(&descriptor).as_slice(),
            original_hlc,
            i64::try_from(*hlc).map_err(|_| Error::ClockRange)?,
            device,
            operation.as_slice()
        ],
    )?;
    tx.execute(
        "DELETE FROM `chat_attachments` WHERE `group_id` = ?1 AND `author` = ?2 AND `message_id` = ?3",
        params![&key.group, &key.author, key.id.as_slice()],
    )?;
    operations::resolve_pending(tx, key, true)?;
    Ok(true)
}

/// Bind a late original to an early deletion, or validate an already bound replay.
pub(super) fn bind_deleted(
    tx: &Transaction<'_>,
    key: &MessageKey,
    sticker: &Sticker,
    descriptor: &[u8],
) -> Result<bool, Error> {
    let row: Option<(i64, Option<Vec<u8>>, Option<i64>)> = tx
        .query_row(
            "SELECT `deleted`, `original_digest`, `original_hlc` FROM `chat_messages`
             WHERE `group_id` = ?1 AND `author` = ?2 AND `message_id` = ?3",
            params![&key.group, &key.author, key.id.as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((deleted, previous_digest, previous_hlc)) = row else {
        return Ok(false);
    };
    let digest = sha256(descriptor);
    let hlc = i64::try_from(sticker.hlc).map_err(|_| Error::ClockRange)?;
    if deleted != 1
        || previous_digest
            .as_deref()
            .is_some_and(|value| value != digest)
        || previous_hlc.is_some_and(|value| value != hlc)
    {
        return Err(Error::InboxConflict);
    }
    tx.execute(
        "UPDATE `chat_messages` SET `original_digest` = ?4, `original_hlc` = ?5
         WHERE `group_id` = ?1 AND `author` = ?2 AND `message_id` = ?3",
        params![
            &key.group,
            &key.author,
            key.id.as_slice(),
            digest.as_slice(),
            hlc
        ],
    )?;
    operations::resolve_pending(tx, key, true)?;
    Ok(true)
}

#[cfg(test)]
mod tests;
