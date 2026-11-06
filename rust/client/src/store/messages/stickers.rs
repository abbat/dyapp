//! Sticker history uses the common media rows and originals lifecycle.

use super::{clear, media, DeliveryState, Error, MessageKey, Store, VerifiedChatSender};
use crate::chat::sticker::Sticker;

/// One verified sticker, including fallback metadata when no media is available.
#[derive(Clone, PartialEq, Eq)]
pub struct ChatSticker {
    /// Group, authenticated author and logical message ID.
    pub key: MessageKey,
    /// Validated metadata and optional descriptor.
    pub sticker: Sticker,
    /// Whether this device has not viewed the incoming sticker.
    pub unread: bool,
    /// Absent for incoming messages and own-device copies.
    pub delivery: Option<DeliveryState>,
}

impl Store {
    /// Persist verified metadata once; a replay does not restore local unread state.
    /// Session/inbox atomic integration is not enabled yet.
    pub fn record_chat_sticker(
        &mut self,
        sender: VerifiedChatSender<'_>,
        sticker: &Sticker,
        unread: bool,
    ) -> Result<bool, Error> {
        let owner = self.owner_peer_id.clone();
        let tx = self.db.transaction()?;
        let inserted = Self::record_chat_sticker_tx(&tx, &owner, sender, sticker, unread)?;
        tx.commit()?;
        Ok(inserted)
    }

    pub(super) fn record_chat_sticker_tx(
        tx: &rusqlite::Transaction<'_>,
        owner: &str,
        sender: VerifiedChatSender<'_>,
        sticker: &Sticker,
        unread: bool,
    ) -> Result<bool, Error> {
        let descriptor = sticker.encode().map_err(|_| Error::InboxConflict)?;
        media::validate_sender(tx, owner, sender, unread)?;
        let key = MessageKey {
            group: sender.group.to_vec(),
            author: sender.author.to_owned(),
            id: sticker.id,
        };
        if clear::is_cleared(tx, &key)? {
            return Ok(false);
        }
        if super::sticker_operations::bind_deleted(tx, &key, sticker, &descriptor)? {
            return Ok(false);
        }
        let inserted = media::record(
            tx,
            owner,
            sender,
            media::MediaRecord {
                id: sticker.id,
                hlc: sticker.hlc,
                kind: 2,
                descriptor,
                forwarded: false,
            },
            unread,
        )?;
        super::operations::resolve_pending(tx, &key, true)?;
        Ok(inserted)
    }

    /// Read a pair's stickers in original sender-time order, including unavailable media.
    /// Merged history and platform exposure follow in a separate checkpoint.
    pub fn current_chat_stickers(&self, peer_id: &str) -> Result<Vec<ChatSticker>, Error> {
        if peer_id.is_empty() || peer_id == self.owner_peer_id {
            return Err(Error::InboxConflict);
        }
        let mut query = self.db.prepare(
            "SELECT `a`.`group_id`, `a`.`author`, `a`.`message_id`, `a`.`original_hlc`,
                    `a`.`descriptor`, `a`.`unread`, `a`.`forwarded`,
                    `a`.`delivery_state`, `a`.`delivery_generation`
             FROM `chat_attachments` AS `a`
             JOIN `conversation_groups` AS `g` ON `a`.`group_id` = `g`.`group_id`
             WHERE `g`.`peer_id` = ?1 AND `a`.`media_kind` = 2
             ORDER BY `a`.`original_hlc`, `a`.`author`, `a`.`message_id`, `a`.`group_id`",
        )?;
        let rows = query.query_map([peer_id], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Vec<u8>>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, Vec<u8>>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, Option<i64>>(7)?,
                row.get::<_, Option<Vec<u8>>>(8)?,
            ))
        })?;
        rows.map(|row| {
            let (group, author, id, hlc, descriptor, unread, forwarded, delivery, generation) =
                row?;
            let id: [u8; 16] = id.try_into().map_err(|_| Error::InboxConflict)?;
            let sticker = Sticker::decode(&descriptor).map_err(|_| Error::InboxConflict)?;
            let delivery = DeliveryState::from_stored(delivery)?;
            if (author != self.owner_peer_id && author != peer_id)
                || !matches!(unread, 0 | 1)
                || (author == self.owner_peer_id && unread != 0)
                || forwarded != 0
                || sticker.id != id
                || i64::try_from(sticker.hlc).ok() != Some(hlc)
                || sticker.encode().map_err(|_| Error::InboxConflict)? != descriptor
                || generation.as_deref().is_some_and(|value| value != id)
                || generation.is_some() != delivery.is_some()
                || (author != self.owner_peer_id && delivery.is_some())
            {
                return Err(Error::InboxConflict);
            }
            Ok(ChatSticker {
                key: MessageKey { group, author, id },
                sticker,
                unread: unread == 1,
                delivery,
            })
        })
        .collect()
    }
}

#[cfg(test)]
mod tests;
