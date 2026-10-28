//! Current chat text projection. Inbox envelope IDs remain a separate transport dedupe key.

use super::likes::load_hlc;
use super::{Error, InboxKey, InboxKind, Store};
use crate::chat::{Body, MAX_TEXT_BYTES};
use crate::PreparedEnvelope;
use dyapp_identity::sha256;
use dyapp_messaging::hlc::{HybridClock, Timestamp};
use rusqlite::{params, OptionalExtension, Transaction};

#[cfg(test)]
mod inbound_tests;
mod operations;
#[cfg(test)]
mod outbound_tests;

pub(super) const SCHEMA: &str = r"
CREATE TABLE `chat_messages` (
    `group_id` BLOB NOT NULL CHECK (length(`group_id`) > 0),
    `author` TEXT NOT NULL CHECK (length(`author`) > 0),
    `message_id` BLOB NOT NULL CHECK (length(`message_id`) = 16),
    `text` BLOB,
    `original_digest` BLOB CHECK (`original_digest` IS NULL OR length(`original_digest`) = 32),
    `edited` INTEGER NOT NULL CHECK (`edited` IN (0, 1)),
    `deleted` INTEGER NOT NULL CHECK (`deleted` IN (0, 1)),
    `last_hlc` INTEGER,
    `last_device` BLOB,
    `last_operation` BLOB CHECK (`last_operation` IS NULL OR length(`last_operation`) = 16),
    CHECK ((`last_hlc` IS NULL AND `last_device` IS NULL AND `last_operation` IS NULL)
        OR (`last_hlc` IS NOT NULL AND `last_device` IS NOT NULL AND `last_operation` IS NOT NULL)),
    CHECK ((`deleted` = 0 AND `text` IS NOT NULL AND `original_digest` IS NOT NULL)
        OR (`deleted` = 1 AND `text` IS NULL)),
    PRIMARY KEY (`group_id`, `author`, `message_id`)
) STRICT;
CREATE TABLE `chat_operations` (
    `group_id` BLOB NOT NULL,
    `author` TEXT NOT NULL,
    `operation_id` BLOB NOT NULL CHECK (length(`operation_id`) = 16),
    `target_id` BLOB NOT NULL CHECK (length(`target_id`) = 16),
    `device` BLOB NOT NULL CHECK (length(`device`) = 32),
    `digest` BLOB NOT NULL CHECK (length(`digest`) = 32),
    `hlc` INTEGER NOT NULL,
    `pending` INTEGER NOT NULL CHECK (`pending` IN (0, 1)),
    `pending_text` BLOB CHECK (`pending_text` IS NULL OR length(`pending_text`) <= 65536),
    CHECK (`pending` = 1 OR `pending_text` IS NULL),
    PRIMARY KEY (`group_id`, `author`, `operation_id`)
) STRICT;
";

/// One logical message, shared by recipient devices and independent of envelope IDs.
#[derive(Clone, PartialEq, Eq)]
pub struct MessageKey {
    /// MLS group ID.
    pub group: Vec<u8>,
    /// Verified MLS author Peer ID.
    pub author: String,
    /// Random ID carried by chat tag 1.
    pub id: [u8; 16],
}

/// Sender metadata that the caller verified through MLS before staging a chat body.
#[derive(Clone, Copy)]
pub struct VerifiedChatSender<'a> {
    /// Group ID from the processed MLS application message.
    pub group: &'a [u8],
    /// Verified sender Peer ID.
    pub author: &'a str,
    /// Verified sender device key used for operation ordering.
    pub device: &'a [u8],
}

/// The sole current content of one message; prior revisions are not app history.
#[derive(Clone, PartialEq, Eq)]
pub struct CurrentMessage {
    /// Present text, or `None` for a deletion placeholder.
    pub text: Option<Vec<u8>>,
    /// Whether a later edit replaced the original text.
    pub edited: bool,
    /// Whether deletion is terminal for this message.
    pub deleted: bool,
}

impl Store {
    /// Draft a fresh operation ID and HLC without changing durable state. Encrypt this exact
    /// operation before committing it with `stage_chat_outbound`.
    pub fn draft_chat_operation(&self, wall_ms: u64) -> Result<([u8; 16], u64), Error> {
        let hlc = HybridClock::resume(load_hlc(&self.db)?).now_at(wall_ms);
        let mut operation = [0; 16];
        getrandom::fill(&mut operation).map_err(|_| Error::Random)?;
        Ok((operation, hlc.into()))
    }

    /// Stage an own chat body, current projection, sealed MLS state and exact writes together.
    /// The caller supplies its certified device key; the author must be this store's owner.
    pub fn stage_chat_outbound(
        &mut self,
        sealed_device: &[u8],
        writes: &[PreparedEnvelope],
        sender: VerifiedChatSender<'_>,
        body: &Body,
        wall_ms: u64,
    ) -> Result<(), Error> {
        if sender.author != self.owner_peer_id
            || sender.group.is_empty()
            || sender.device.len() != 32
        {
            return Err(Error::InboxConflict);
        }
        let id = match body {
            Body::Text { id, .. } => id,
            Body::Edit { target, .. } | Body::Delete { target, .. } => target,
            _ => return Err(Error::InboxConflict),
        };
        let key = MessageKey {
            group: sender.group.to_vec(),
            author: sender.author.to_owned(),
            id: *id,
        };
        let tx = self.db.transaction()?;
        if let Body::Edit { hlc, .. } | Body::Delete { hlc, .. } = body {
            if Timestamp::from(*hlc) <= load_hlc(&tx)? {
                return Err(Error::InboxConflict);
            }
        }
        Self::stage_outbound_tx(&tx, None, sealed_device, writes, true)?;
        let changed = match body {
            Body::Text { text, .. } => Self::record_chat_text_tx(&tx, &key, text)?,
            Body::Edit { .. } | Body::Delete { .. } => {
                Self::apply_chat_operation_tx(&tx, &key, sender.device, body, wall_ms)?
            }
            _ => unreachable!(),
        };
        // An outbound action must change local state; replay uses the exact existing outbox write.
        if !changed {
            return Err(Error::InboxConflict);
        }
        tx.commit()?;
        Ok(())
    }

    /// Commit one authenticated chat body, sealed MLS state, inbox dedupe and ACK intent together.
    /// The inbox retains only a body digest, never a second copy of chat text.
    pub fn stage_chat_inbound(
        &mut self,
        sealed_device: &[u8],
        inbox: &InboxKey,
        sender: VerifiedChatSender<'_>,
        body: &Body,
        wall_ms: u64,
    ) -> Result<bool, Error> {
        let id = match body {
            Body::Text { id, .. } => id,
            Body::Edit { target, .. } | Body::Delete { target, .. } => target,
            _ => return Err(Error::InboxConflict),
        };
        if sender.group.is_empty() || sender.author.is_empty() || sender.device.len() != 32 {
            return Err(Error::InboxConflict);
        }
        let key = MessageKey {
            group: sender.group.to_vec(),
            author: sender.author.to_owned(),
            id: *id,
        };
        let digest = sha256(&body.encode());
        self.stage_inbound_with(
            sealed_device,
            inbox,
            InboxKind::Message,
            &digest,
            None,
            |tx| {
                match body {
                    Body::Text { text, .. } => {
                        Self::record_chat_text_tx(tx, &key, text)?;
                    }
                    Body::Edit { .. } | Body::Delete { .. } => {
                        Self::apply_chat_operation_tx(tx, &key, sender.device, body, wall_ms)?;
                    }
                    _ => unreachable!(),
                }
                Ok(())
            },
        )
    }

    /// Read one logical message from this profile's current projection.
    pub fn chat_message(&self, key: &MessageKey) -> Result<Option<CurrentMessage>, Error> {
        let row: Option<(Option<Vec<u8>>, i64, i64)> = self
            .db
            .query_row(
                "SELECT `text`, `edited`, `deleted` FROM `chat_messages`
             WHERE `group_id` = ?1 AND `author` = ?2 AND `message_id` = ?3",
                params![&key.group, &key.author, key.id.as_slice()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        row.map(|(text, edited, deleted)| {
            if !matches!(
                (edited, deleted, text.is_some()),
                (0 | 1, 0, true) | (0 | 1, 1, false)
            ) {
                return Err(Error::InboxConflict);
            }
            Ok(CurrentMessage {
                text,
                edited: edited == 1,
                deleted: deleted == 1,
            })
        })
        .transpose()
    }

    /// Store first text once. A repeated body is harmless; a different body with the same
    /// logical key is a conflict. The caller has already verified the author through MLS.
    pub fn record_chat_text(&mut self, key: &MessageKey, text: &[u8]) -> Result<bool, Error> {
        let tx = self.db.transaction()?;
        let changed = Self::record_chat_text_tx(&tx, key, text)?;
        tx.commit()?;
        Ok(changed)
    }

    pub(super) fn record_chat_text_tx(
        tx: &Transaction<'_>,
        key: &MessageKey,
        text: &[u8],
    ) -> Result<bool, Error> {
        if key.group.is_empty() || key.author.is_empty() || text.len() > MAX_TEXT_BYTES {
            return Err(Error::InboxConflict);
        }
        let digest = sha256(text);
        let saved: Option<Option<Vec<u8>>> = tx
            .query_row(
                "SELECT `original_digest` FROM `chat_messages`
             WHERE `group_id` = ?1 AND `author` = ?2 AND `message_id` = ?3",
                params![&key.group, &key.author, key.id.as_slice()],
                |row| row.get(0),
            )
            .optional()?;
        match saved {
            Some(Some(original_digest)) => {
                if original_digest != digest {
                    return Err(Error::InboxConflict);
                }
                // An original delivered after edit/delete must not replace the projection.
                Ok(false)
            }
            Some(None) => {
                // A delete reached us before its original; record its digest, keep the tombstone.
                tx.execute(
                    "UPDATE `chat_messages` SET `original_digest` = ?4
                     WHERE `group_id` = ?1 AND `author` = ?2 AND `message_id` = ?3",
                    params![
                        &key.group,
                        &key.author,
                        key.id.as_slice(),
                        digest.as_slice()
                    ],
                )?;
                operations::resolve_pending(tx, key, true)?;
                Ok(false)
            }
            None => {
                tx.execute(
                    "INSERT INTO `chat_messages` (`group_id`, `author`, `message_id`, `text`, `original_digest`, `edited`, `deleted`)
                     VALUES (?1, ?2, ?3, ?4, ?5, 0, 0)",
                    params![&key.group, &key.author, key.id.as_slice(), text, digest.as_slice()],
                )?;
                operations::resolve_pending(tx, key, false)?;
                Ok(true)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MessageKey, Store};
    use crate::store::Error;
    use dyapp_identity::Identity;

    #[test]
    fn text_projection_survives_restart_and_refuses_conflicts() {
        let mut random = [0; 16];
        getrandom::fill(&mut random).unwrap();
        let path = std::env::temp_dir().join(format!(
            "dyapp-chat-{:032x}.db",
            u128::from_be_bytes(random)
        ));
        let alice = Identity::generate();
        let bob = Identity::generate();
        let key = MessageKey {
            group: vec![3],
            author: alice.peer_id(),
            id: [4; 16],
        };
        let mut store = Store::open(&path, &alice).unwrap();
        assert!(store.chat_message(&key).unwrap().is_none());
        assert!(store.record_chat_text(&key, b"hello").unwrap());
        assert!(!store.record_chat_text(&key, b"hello").unwrap());
        assert!(matches!(
            store.record_chat_text(&key, b"other"),
            Err(Error::InboxConflict)
        ));
        drop(store);
        assert!(matches!(
            Store::open(&path, &bob),
            Err(Error::WrongIdentity)
        ));
        let store = Store::open(&path, &alice).unwrap();
        let message = store.chat_message(&key).unwrap().unwrap();
        assert_eq!(message.text.as_deref(), Some(&b"hello"[..]));
        assert!(!message.edited && !message.deleted);
    }
}
