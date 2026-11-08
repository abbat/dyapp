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

mod list;

impl ChatSticker {
    /// Authenticated sender wall time, without the HLC logical counter.
    #[must_use]
    pub fn sent_at_ms(&self) -> u64 {
        dyapp_messaging::hlc::Timestamp::from(self.sticker.hlc).wall_ms()
    }

    pub(crate) fn order(&self) -> super::attachments::ItemOrder<'_> {
        (
            self.sticker.hlc,
            &self.key.author,
            &self.key.id,
            &self.key.group,
        )
    }
}

impl Store {
    /// Persist verified metadata once; a replay does not restore local unread state.
    /// Direct storage API; session receive uses the shared inbox/MLS transaction.
    ///
    /// # Errors
    ///
    /// Returns an error if sticker, sender, or group validation fails, the timestamp
    /// exceeds the storage range, message state conflicts, pending operations
    /// cannot be resolved, or database access or transaction commit fails.
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
        if inserted && sender.author == owner {
            // The peer already knows this pack, even after local history is cleared.
            tx.execute(
                "INSERT INTO `sticker_shared_packs` (`peer_id`, `pack_id`)
                 SELECT `peer_id`, ?2 FROM `conversation_groups`
                 WHERE `group_id` = ?1 AND `peer_id` != ?3
                 ON CONFLICT (`peer_id`, `pack_id`) DO NOTHING",
                rusqlite::params![sender.group, sticker.pack_id.as_slice(), owner],
            )?;
        }
        Ok(inserted)
    }

    /// Whether this owner explicitly shared the pack with this peer in a verified pair chat.
    /// Historical proof only: responses must also check open/unblocked chat and saved-pack existence.
    ///
    /// # Errors
    ///
    /// Returns an error if the peer is empty or identifies this owner, or querying
    /// or decoding the stored sharing evidence fails.
    pub fn shared_sticker_pack(&self, peer: &str, pack_id: &[u8; 32]) -> Result<bool, Error> {
        if peer.is_empty() || peer == self.owner_peer_id {
            return Err(Error::InboxConflict);
        }
        Ok(self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM `sticker_shared_packs` WHERE `peer_id` = ?1 AND `pack_id` = ?2)",
            rusqlite::params![peer, pack_id.as_slice()],
            |row| row.get(0),
        )?)
    }

    /// Whether this exact message is a stored sticker eligible for a terminal delete.
    pub(crate) fn has_chat_sticker(&self, key: &MessageKey) -> Result<bool, Error> {
        Ok(self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM `chat_attachments`
             WHERE `group_id` = ?1 AND `author` = ?2 AND `message_id` = ?3 AND `media_kind` = 2)",
            rusqlite::params![&key.group, &key.author, key.id.as_slice()],
            |row| row.get(0),
        )?)
    }

    /// Read a pair's stickers in original sender-time order, including unavailable media.
    ///
    /// # Errors
    ///
    /// Returns an error if the peer is empty or identifies this owner, querying or
    /// decoding fails, or stored sticker descriptors and metadata are inconsistent.
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
        let rows = query.query_map([peer_id], super::attachments::stored_media)?;
        rows.map(|row| self.sticker_from(peer_id, row?)).collect()
    }

    fn sticker_from(
        &self,
        peer_id: &str,
        stored: super::attachments::StoredMedia,
    ) -> Result<ChatSticker, Error> {
        let (group, author, id, hlc, descriptor, unread, forwarded, delivery, generation) = stored;
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
    }
}

#[cfg(test)]
mod tests;
