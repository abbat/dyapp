//! Current chat text projection. Inbox envelope IDs remain a separate transport dedupe key.

use super::{Error, Store};
use crate::chat::MAX_TEXT_BYTES;
use rusqlite::{params, OptionalExtension};

// This table is part of the fresh pre-release schema; no migration/version bump is needed.
pub(super) const SCHEMA: &str = r"
CREATE TABLE `chat_messages` (
    `group_id` BLOB NOT NULL CHECK (length(`group_id`) > 0),
    `author` TEXT NOT NULL CHECK (length(`author`) > 0),
    `message_id` BLOB NOT NULL CHECK (length(`message_id`) = 16),
    `text` BLOB,
    `edited` INTEGER NOT NULL CHECK (`edited` IN (0, 1)),
    `deleted` INTEGER NOT NULL CHECK (`deleted` IN (0, 1)),
    CHECK ((`deleted` = 0 AND `text` IS NOT NULL) OR (`deleted` = 1 AND `text` IS NULL)),
    PRIMARY KEY (`group_id`, `author`, `message_id`)
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
        if key.group.is_empty() || key.author.is_empty() || text.len() > MAX_TEXT_BYTES {
            return Err(Error::InboxConflict);
        }
        let tx = self.db.transaction()?;
        let inserted = tx.execute(
            "INSERT INTO `chat_messages` (`group_id`, `author`, `message_id`, `text`, `edited`, `deleted`)
             VALUES (?1, ?2, ?3, ?4, 0, 0) ON CONFLICT (`group_id`, `author`, `message_id`) DO NOTHING",
            params![&key.group, &key.author, key.id.as_slice(), text],
        )?;
        if inserted == 0 {
            let old: (Option<Vec<u8>>, i64) = tx.query_row(
                "SELECT `text`, `deleted` FROM `chat_messages`
                 WHERE `group_id` = ?1 AND `author` = ?2 AND `message_id` = ?3",
                params![&key.group, &key.author, key.id.as_slice()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            if old.1 != 0 || old.0.as_deref() != Some(text) {
                return Err(Error::InboxConflict);
            }
            return Ok(false);
        }
        tx.commit()?;
        Ok(true)
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
