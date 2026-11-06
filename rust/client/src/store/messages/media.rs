//! Shared identity, replay and cross-kind checks for durable media rows.

use super::{Error, MessageKey, VerifiedChatSender};
use rusqlite::{params, OptionalExtension, Transaction};

pub(super) struct MediaRecord {
    pub id: [u8; 16],
    pub hlc: u64,
    pub kind: i64,
    pub descriptor: Vec<u8>,
    pub forwarded: bool,
}

pub(super) fn validate_sender(
    tx: &Transaction<'_>,
    owner: &str,
    sender: VerifiedChatSender<'_>,
    unread: bool,
) -> Result<(), Error> {
    if sender.group.is_empty() || sender.device.len() != 32 || sender.author.is_empty() {
        return Err(Error::InboxConflict);
    }
    let peer: String = tx
        .query_row(
            "SELECT `peer_id` FROM `conversation_groups` WHERE `group_id` = ?1",
            [sender.group],
            |row| row.get(0),
        )
        .optional()?
        .ok_or(Error::GroupMismatch)?;
    if (sender.author != owner && sender.author != peer) || (sender.author == owner && unread) {
        return Err(Error::InboxConflict);
    }
    Ok(())
}

/// Callers validate their typed descriptor before entering this common row boundary.
pub(super) fn record(
    tx: &Transaction<'_>,
    owner: &str,
    sender: VerifiedChatSender<'_>,
    media: MediaRecord,
    unread: bool,
) -> Result<bool, Error> {
    let hlc = i64::try_from(media.hlc).map_err(|_| Error::ClockRange)?;
    validate_sender(tx, owner, sender, unread)?;
    let key = MessageKey {
        group: sender.group.to_vec(),
        author: sender.author.to_owned(),
        id: media.id,
    };
    let text_exists: bool = tx
        .query_row(
            "SELECT 1 FROM `chat_messages` WHERE `group_id` = ?1 AND `author` = ?2 AND `message_id` = ?3",
            params![sender.group, sender.author, media.id.as_slice()],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .is_some();
    if text_exists {
        return Err(Error::InboxConflict);
    }
    let saved: Option<(i64, Vec<u8>, i64, i64)> = tx
        .query_row(
            "SELECT `original_hlc`, `descriptor`, `forwarded`, `media_kind` FROM `chat_attachments`
             WHERE `group_id` = ?1 AND `author` = ?2 AND `message_id` = ?3",
            params![&key.group, &key.author, key.id.as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    if let Some((old_hlc, old_descriptor, old_forwarded, old_kind)) = saved {
        if old_hlc != hlc
            || old_descriptor != media.descriptor
            || old_forwarded != i64::from(media.forwarded)
            || old_kind != media.kind
        {
            return Err(Error::InboxConflict);
        }
        return Ok(false);
    }
    tx.execute(
        "INSERT INTO `chat_attachments` (`group_id`, `author`, `message_id`, `original_hlc`,
         `descriptor`, `forwarded`, `unread`, `media_kind`)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            &key.group,
            &key.author,
            key.id.as_slice(),
            hlc,
            media.descriptor,
            i64::from(media.forwarded),
            i64::from(unread),
            media.kind
        ],
    )?;
    Ok(true)
}
