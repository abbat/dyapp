//! Durable media descriptors; encrypted blob bytes remain on the media node.

use super::{Body, ChatMessage, DeliveryState, Error, MessageKey, Store, VerifiedChatSender};
use crate::chat::AttachmentMessage;
use dyapp_messaging::hlc::Timestamp;
use rusqlite::{params, OptionalExtension, Row, Transaction};
use std::collections::HashMap;

/// One media message in a pair's local history.
#[derive(Clone, PartialEq, Eq)]
pub struct ChatAttachment {
    /// The MLS group, verified sender Peer ID and message-scoped logical ID.
    pub key: MessageKey,
    /// Fetch descriptor and authenticated original time.
    pub message: AttachmentMessage,
    /// This device has not yet viewed the incoming attachment.
    pub unread: bool,
    /// The sender forwarded this attachment; no original author is asserted.
    pub forwarded: bool,
    /// Absent for peer media or an own attachment received from another device.
    pub delivery: Option<DeliveryState>,
}

/// One entry of a pair's merged history.
#[derive(Clone, PartialEq, Eq)]
pub enum ChatItem {
    /// A text or its deletion placeholder.
    Text(ChatMessage),
    /// A media message.
    Attachment(ChatAttachment),
    /// A sticker, even when its media descriptor is unavailable.
    Sticker(super::ChatSticker),
}

/// Original HLC, author, logical ID and group ID: the order shared by texts and attachments.
pub(crate) type ItemOrder<'a> = (u64, &'a str, &'a [u8; 16], &'a [u8]);

impl ChatMessage {
    /// Absent for a legacy text, which has no sent time and sorts before any timed entry.
    pub(crate) fn order(&self) -> Option<ItemOrder<'_>> {
        let MessageKey { group, author, id } = &self.key;
        let hlc = u64::from(self.original_hlc?);
        Some((hlc, author.as_str(), id, group.as_slice()))
    }
}

impl ChatAttachment {
    /// The sender's authenticated wall time.
    #[must_use]
    pub fn sent_at_ms(&self) -> u64 {
        Timestamp::from(self.message.hlc).wall_ms()
    }

    pub(crate) fn order(&self) -> ItemOrder<'_> {
        let MessageKey { group, author, id } = &self.key;
        (self.message.hlc, author.as_str(), id, group.as_slice())
    }
}

impl ChatItem {
    fn order(&self) -> Option<ItemOrder<'_>> {
        match self {
            Self::Text(message) => message.order(),
            Self::Attachment(attachment) => Some(attachment.order()),
            Self::Sticker(sticker) => Some(sticker.order()),
        }
    }
}

/// Shared media columns consumed by the photo and sticker validators.
pub(super) type StoredMedia = (
    Vec<u8>,
    String,
    Vec<u8>,
    i64,
    Vec<u8>,
    i64,
    i64,
    Option<i64>,
    Option<Vec<u8>>,
);

pub(super) fn stored_media(row: &Row<'_>) -> rusqlite::Result<StoredMedia> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
    ))
}

pub(super) fn exists(tx: &Transaction<'_>, key: &MessageKey) -> Result<bool, Error> {
    Ok(tx
        .query_row(
            "SELECT 1 FROM `chat_attachments` WHERE `group_id` = ?1 AND `author` = ?2 AND `message_id` = ?3",
            params![&key.group, &key.author, key.id.as_slice()],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .is_some())
}

impl Store {
    /// Record a verified media body once. A replay keeps local unread state unchanged.
    /// Session receive/send uses the shared inbox/outbox transaction.
    ///
    /// # Errors
    ///
    /// Returns an error if the body is not a valid attachment, sender or group
    /// metadata is invalid, the timestamp exceeds the storage range, a logical
    /// message key conflicts, or database access or transaction commit fails.
    pub fn record_chat_attachment(
        &mut self,
        sender: VerifiedChatSender<'_>,
        body: &Body,
        unread: bool,
    ) -> Result<bool, Error> {
        let owner = self.owner_peer_id.clone();
        let tx = self.db.transaction()?;
        let inserted = Self::record_chat_attachment_tx(&tx, &owner, sender, body, unread)?;
        tx.commit()?;
        Ok(inserted)
    }

    pub(super) fn record_chat_attachment_tx(
        tx: &Transaction<'_>,
        owner: &str,
        sender: VerifiedChatSender<'_>,
        body: &Body,
        unread: bool,
    ) -> Result<bool, Error> {
        let message = body.attachment_message().ok_or(Error::InboxConflict)?;
        let descriptor = body.encode();
        if Body::decode(&descriptor).map_err(|_| Error::InboxConflict)? != *body {
            return Err(Error::InboxConflict);
        }
        super::media::record(
            tx,
            owner,
            sender,
            super::media::MediaRecord {
                id: message.id,
                hlc: message.hlc,
                kind: 1,
                descriptor,
                forwarded: matches!(body, Body::Forwarded(_)),
            },
            unread,
        )
    }

    /// Read descriptors across the pair's cataloged groups in stable sender-time order.
    ///
    /// # Errors
    ///
    /// Returns an error if the peer is empty or identifies this owner, querying or
    /// decoding fails, or stored descriptors and message metadata are inconsistent.
    pub fn current_chat_attachments(&self, peer_id: &str) -> Result<Vec<ChatAttachment>, Error> {
        if peer_id.is_empty() || peer_id == self.owner_peer_id {
            return Err(Error::InboxConflict);
        }
        let mut query = self.db.prepare(
            "SELECT `a`.`group_id`, `a`.`author`, `a`.`message_id`, `a`.`original_hlc`,
                    `a`.`descriptor`, `a`.`unread`, `a`.`forwarded`,
                    `a`.`delivery_state`, `a`.`delivery_generation`
             FROM `chat_attachments` AS `a`
             JOIN `conversation_groups` AS `g` ON `a`.`group_id` = `g`.`group_id`
             WHERE `g`.`peer_id` = ?1 AND `a`.`media_kind` = 1
             ORDER BY `a`.`original_hlc`, `a`.`author`, `a`.`message_id`, `a`.`group_id`",
        )?;
        let rows = query.query_map([peer_id], stored_media)?;
        rows.map(|row| self.attachment_from(peer_id, row?))
            .collect()
    }

    /// Merge a pair's texts and attachments. Legacy texts come first in local arrival order;
    /// everything else follows original HLC, author, logical ID and group ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the peer is invalid, querying or decoding texts,
    /// attachments, or stickers fails, or their stored metadata is inconsistent.
    pub fn current_chat_items(&self, peer_id: &str) -> Result<Vec<ChatItem>, Error> {
        let texts = self.current_chat_messages(peer_id)?;
        let attachments = self.current_chat_attachments(peer_id)?;
        let mut items: Vec<_> = texts
            .into_iter()
            .map(ChatItem::Text)
            .chain(attachments.into_iter().map(ChatItem::Attachment))
            .chain(
                self.current_chat_stickers(peer_id)?
                    .into_iter()
                    .map(ChatItem::Sticker),
            )
            .collect();
        // The sort is stable, so legacy texts keep the arrival order they were read in.
        items.sort_by(|a, b| a.order().cmp(&b.order()));
        Ok(items)
    }

    /// Each peer's newest attachment and count of unread attachments, by Peer ID.
    pub(crate) fn latest_chat_attachments(
        &self,
    ) -> Result<HashMap<String, (ChatAttachment, usize)>, Error> {
        let mut query = self.db.prepare(
            "SELECT `group_id`, `author`, `message_id`, `original_hlc`, `descriptor`, `unread`,
                    `forwarded`, `delivery_state`, `delivery_generation`, `peer_id`,
                    `unread_count`
             FROM (
                 SELECT `a`.`group_id`, `a`.`author`, `a`.`message_id`, `a`.`original_hlc`,
                        `a`.`descriptor`, `a`.`unread`, `a`.`forwarded`, `a`.`delivery_state`,
                        `a`.`delivery_generation`, `g`.`peer_id`,
                        ROW_NUMBER() OVER (
                            PARTITION BY `g`.`peer_id`
                            ORDER BY `a`.`original_hlc` DESC, `a`.`author` DESC,
                                     `a`.`message_id` DESC, `a`.`group_id` DESC
                        ) AS `attachment_rank`,
                        SUM(`a`.`unread`) OVER (PARTITION BY `g`.`peer_id`) AS `unread_count`
                 FROM `chat_attachments` AS `a`
                 JOIN `conversation_groups` AS `g` ON `a`.`group_id` = `g`.`group_id`
                 WHERE `a`.`media_kind` = 1
             )
             WHERE `attachment_rank` = 1",
        )?;
        let rows = query.query_map([], |row| {
            Ok((
                stored_media(row)?,
                row.get::<_, String>(9)?,
                row.get::<_, i64>(10)?,
            ))
        })?;
        rows.map(|row| {
            let (stored, peer_id, unread) = row?;
            let attachment = self.attachment_from(&peer_id, stored)?;
            let unread = usize::try_from(unread).map_err(|_| Error::InboxConflict)?;
            Ok::<_, Error>((peer_id, (attachment, unread)))
        })
        .collect()
    }

    fn attachment_from(&self, peer_id: &str, stored: StoredMedia) -> Result<ChatAttachment, Error> {
        let (group, author, id, hlc, descriptor, unread, forwarded, delivery, generation) = stored;
        if (author != self.owner_peer_id && author != peer_id)
            || !matches!(unread, 0 | 1)
            || !matches!(forwarded, 0 | 1)
            || (author == self.owner_peer_id && unread != 0)
        {
            return Err(Error::InboxConflict);
        }
        let id: [u8; 16] = id.try_into().map_err(|_| Error::InboxConflict)?;
        let hlc = u64::try_from(hlc).map_err(|_| Error::InboxConflict)?;
        let delivery = DeliveryState::from_stored(delivery)?;
        if generation
            .as_deref()
            .is_some_and(|generation| generation != id.as_slice())
            || generation.is_some() != delivery.is_some()
            || (author != self.owner_peer_id && delivery.is_some())
        {
            return Err(Error::InboxConflict);
        }
        let body = Body::decode(&descriptor).map_err(|_| Error::InboxConflict)?;
        let message = body.attachment_message().ok_or(Error::InboxConflict)?;
        if message.id != id
            || message.hlc != hlc
            || matches!(&body, Body::Forwarded(_)) != (forwarded == 1)
            || body.encode() != descriptor
        {
            return Err(Error::InboxConflict);
        }
        Ok(ChatAttachment {
            key: MessageKey { group, author, id },
            message: message.clone(),
            unread: unread == 1,
            forwarded: forwarded == 1,
            delivery,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attachment::Attachment;
    use crate::store::tests::{catalog_group, temp_db_path};
    use dyapp_identity::Identity;
    use dyapp_messaging::hlc::Timestamp;
    use rusqlite::params;

    fn body(id: u8, time: u64) -> Body {
        Body::Attachment(AttachmentMessage {
            id: [id; 16],
            hlc: Timestamp::new(time, 0).into(),
            attachment: Attachment {
                key: [3; 32],
                plaintext_sha256: [4; 32],
                blob_sha256: [5; 32],
                release_secret: [6; 32],
                size: 12,
                preview: vec![7, 8],
                mime: "image/jpeg".into(),
                width: 20,
                height: 10,
            },
        })
    }

    #[test]
    fn descriptor_survives_restart_and_conflicts_across_body_kinds() {
        let path = temp_db_path();
        let owner = Identity::generate();
        let peer = Identity::generate().peer_id();
        let group = [9];
        let device = [8; 32];
        let mut store = Store::open(&path, &owner).unwrap();
        catalog_group(&mut store, &group, &peer);
        let sender = VerifiedChatSender {
            group: &group,
            author: &peer,
            device: &device,
        };
        let first = body(1, 100);
        assert!(store.record_chat_attachment(sender, &first, true).unwrap());
        assert!(!store.record_chat_attachment(sender, &first, false).unwrap());
        assert!(matches!(
            store.record_chat_attachment(sender, &body(1, 101), true),
            Err(Error::InboxConflict)
        ));
        assert!(matches!(
            store.record_chat_text(
                &MessageKey {
                    group: group.to_vec(),
                    author: peer.clone(),
                    id: [1; 16]
                },
                b"text"
            ),
            Err(Error::InboxConflict)
        ));
        let second_key = MessageKey {
            group: group.to_vec(),
            author: peer.clone(),
            id: [2; 16],
        };
        assert!(store.record_chat_text(&second_key, b"text").unwrap());
        assert!(matches!(
            store.record_chat_attachment(sender, &body(2, 102), true),
            Err(Error::InboxConflict)
        ));
        assert!(store
            .record_chat_attachment(sender, &Body::Forwarded(Box::new(body(3, 99))), true)
            .unwrap());
        drop(store);

        let store = Store::open(&path, &owner).unwrap();
        let history = store.current_chat_attachments(&peer).unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].key.id, [3; 16]);
        assert!(history[0].forwarded);
        assert_eq!(history[1].key.id, [1; 16]);
        assert!(history[1].message == *first.attachment_message().unwrap());
        assert!(history[1].unread);
        assert_eq!(
            store
                .current_chat_attachments(&Identity::generate().peer_id())
                .unwrap()
                .len(),
            0
        );
    }

    #[test]
    fn photo_queries_filter_other_media_before_ranking_and_unread_count() {
        let owner = Identity::generate();
        let peer = Identity::generate().peer_id();
        let mut store = Store::open(&temp_db_path(), &owner).unwrap();
        catalog_group(&mut store, &[9], &peer);
        let sender = VerifiedChatSender {
            group: &[9],
            author: &peer,
            device: &[8; 32],
        };
        store
            .record_chat_attachment(sender, &body(1, 100), true)
            .unwrap();
        store
            .record_chat_attachment(sender, &body(2, 101), true)
            .unwrap();
        // A later media kind must not replace the newest photo or enter its unread count.
        store
            .db
            .execute(
                "UPDATE `chat_attachments` SET `media_kind` = 2 WHERE `message_id` = ?1",
                [[2; 16].as_slice()],
            )
            .unwrap();
        let photos = store.current_chat_attachments(&peer).unwrap();
        assert_eq!(photos.len(), 1);
        assert_eq!(photos[0].key.id, [1; 16]);
        let latest = store.latest_chat_attachments().unwrap();
        assert_eq!(latest[&peer].0.key.id, [1; 16]);
        assert_eq!(latest[&peer].1, 1);
        assert!(store
            .db
            .execute("UPDATE `chat_attachments` SET `media_kind` = 3", [],)
            .is_err());
        assert!(matches!(
            store.record_chat_attachment(sender, &body(2, 101), true),
            Err(Error::InboxConflict)
        ));
    }

    #[test]
    fn corrupt_descriptor_or_metadata_fails_closed() {
        let owner = Identity::generate();
        let peer = Identity::generate().peer_id();
        let group = [9];
        let device = [8; 32];
        let mut store = Store::open(&temp_db_path(), &owner).unwrap();
        catalog_group(&mut store, &group, &peer);
        store
            .record_chat_attachment(
                VerifiedChatSender {
                    group: &group,
                    author: &peer,
                    device: &device,
                },
                &body(1, 100),
                true,
            )
            .unwrap();
        store
            .db
            .execute("UPDATE `chat_attachments` SET `original_hlc` = ?1", [101])
            .unwrap();
        assert!(matches!(
            store.current_chat_attachments(&peer),
            Err(Error::InboxConflict)
        ));
        store
            .db
            .execute(
                "UPDATE `chat_attachments` SET `original_hlc` = ?1, `descriptor` = ?2",
                params![
                    i64::try_from(u64::from(Timestamp::new(100, 0))).unwrap(),
                    b"bad".as_slice()
                ],
            )
            .unwrap();
        assert!(matches!(
            store.current_chat_attachments(&peer),
            Err(Error::InboxConflict)
        ));
    }
}
