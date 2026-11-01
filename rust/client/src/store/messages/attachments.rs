//! Durable media descriptors; encrypted blob bytes remain on the media node.

use super::{Body, DeliveryState, Error, MessageKey, Store, VerifiedChatSender};
use crate::chat::AttachmentMessage;
use rusqlite::{params, OptionalExtension, Transaction};

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
    /// Session receive/send will call the transaction form in the next checkpoint.
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
        let forwarded = matches!(body, Body::Forwarded(_));
        let hlc = i64::try_from(message.hlc).map_err(|_| Error::ClockRange)?;
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
        let key = MessageKey {
            group: sender.group.to_vec(),
            author: sender.author.to_owned(),
            id: message.id,
        };
        let text_exists: bool = tx
            .query_row(
                "SELECT 1 FROM `chat_messages` WHERE `group_id` = ?1 AND `author` = ?2 AND `message_id` = ?3",
                params![sender.group, sender.author, message.id.as_slice()],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .is_some();
        if text_exists {
            return Err(Error::InboxConflict);
        }
        let descriptor = body.encode();
        if Body::decode(&descriptor).map_err(|_| Error::InboxConflict)? != *body {
            return Err(Error::InboxConflict);
        }
        let saved: Option<(i64, Vec<u8>, i64)> = tx
            .query_row(
                "SELECT `original_hlc`, `descriptor`, `forwarded` FROM `chat_attachments`
                 WHERE `group_id` = ?1 AND `author` = ?2 AND `message_id` = ?3",
                params![&key.group, &key.author, key.id.as_slice()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        if let Some((old_hlc, old_descriptor, old_forwarded)) = saved {
            if old_hlc != hlc
                || old_descriptor != descriptor
                || old_forwarded != i64::from(forwarded)
            {
                return Err(Error::InboxConflict);
            }
            return Ok(false);
        }
        tx.execute(
            "INSERT INTO `chat_attachments` (`group_id`, `author`, `message_id`, `original_hlc`, `descriptor`, `forwarded`, `unread`)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![&key.group, &key.author, key.id.as_slice(), hlc, descriptor, i64::from(forwarded), i64::from(unread)],
        )?;
        Ok(true)
    }

    /// Read descriptors across the pair's cataloged groups in stable sender-time order.
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
             WHERE `g`.`peer_id` = ?1
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
        })
        .collect()
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
