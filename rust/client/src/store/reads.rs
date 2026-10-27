//! Read receipts in one-to-one chats: the local opt-in setting, the latest peer text receipted
//! and the latest own text the peer has read ([read receipts](../../../docs/design/read-receipts.md)).
//! Order is `chat_messages` rowid order: texts are never removed, and own texts are recorded
//! when they are sent.

use super::{Error, Store};
use crate::PreparedEnvelope;
use rusqlite::{params, Connection, OptionalExtension};

const SETTING: &str = "send_read_receipts";

pub(super) const SCHEMA_V12: &str = r"
CREATE TABLE `read_marks` (
    `group_id` BLOB NOT NULL CHECK (length(`group_id`) > 0),
    `peer_id` TEXT NOT NULL CHECK (length(`peer_id`) > 0),
    `sent_up_to` BLOB CHECK (`sent_up_to` IS NULL OR length(`sent_up_to`) = 16),
    `received_up_to` BLOB CHECK (`received_up_to` IS NULL OR length(`received_up_to`) = 16),
    PRIMARY KEY (`group_id`, `peer_id`)
) STRICT;
PRAGMA user_version = 12;
";

impl Store {
    /// Whether read receipts are sent; absent means off.
    pub fn send_read_receipts(&self) -> Result<bool, Error> {
        let value: Option<(i64, Vec<u8>)> = self
            .db
            .query_row(
                "SELECT `format_version`, `value` FROM `settings` WHERE `key` = ?1",
                [SETTING],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        match value {
            None => Ok(false),
            Some((1, value)) if value == [0] => Ok(false),
            Some((1, value)) if value == [1] => Ok(true),
            _ => Err(Error::InvalidSetting),
        }
    }

    /// Persist whether read receipts are sent.
    pub fn set_send_read_receipts(&self, enabled: bool) -> Result<(), Error> {
        self.db.execute(
            "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, 1, ?2)
             ON CONFLICT (`key`) DO UPDATE SET `format_version` = 1, `value` = excluded.`value`",
            params![SETTING, [u8::from(enabled)]],
        )?;
        Ok(())
    }

    /// Whether a read receipt for `seen`, the latest text from `peer` the app shows in `group`,
    /// is due: the setting is on, the peer is unblocked and `seen` is later than the text last
    /// receipted. Changes nothing; commit the encrypted receipt with [`Self::stage_read`].
    pub fn draft_read(&self, group: &[u8], peer: &str, seen: [u8; 16]) -> Result<bool, Error> {
        if peer == self.owner_peer_id || !self.send_read_receipts()? || self.blocked(peer)? {
            return Ok(false);
        }
        later_than_mark(&self.db, group, peer, peer, seen, "sent_up_to")
    }

    /// Commit a drafted read receipt, its mark and the encrypted outbound writes in one
    /// transaction. The receipt has no expiry and gets no delivery receipt. A stale draft or a
    /// peer no longer allowed changes nothing.
    pub fn stage_read(
        &mut self,
        group: &[u8],
        peer: &str,
        seen: [u8; 16],
        sealed_device: &[u8],
        writes: &[PreparedEnvelope],
    ) -> Result<(), Error> {
        if writes.is_empty() || !self.draft_read(group, peer, seen)? {
            return Err(Error::InteractionConflict);
        }
        let tx = self.db.transaction()?;
        tx.execute(
            "INSERT INTO `read_marks` (`group_id`, `peer_id`, `sent_up_to`) VALUES (?1, ?2, ?3)
             ON CONFLICT (`group_id`, `peer_id`) DO UPDATE SET `sent_up_to` = excluded.`sent_up_to`",
            params![group, peer, seen.as_slice()],
        )?;
        Self::stage_outbound_tx(&tx, None, sealed_device, writes, false)?;
        tx.commit()?;
        Ok(())
    }

    /// Move the mark of own texts `sender` has read in `group` up to `up_to`. The caller verified
    /// the sender as a group member through MLS. A blocked sender, an ID that is not an own text
    /// in the group and one not later than the mark change nothing. Returns whether it moved.
    pub fn receive_read(
        &mut self,
        group: &[u8],
        sender: &str,
        up_to: [u8; 16],
    ) -> Result<bool, Error> {
        if sender == self.owner_peer_id
            || self.blocked(sender)?
            || !later_than_mark(
                &self.db,
                group,
                sender,
                &self.owner_peer_id,
                up_to,
                "received_up_to",
            )?
        {
            return Ok(false);
        }
        self.db.execute(
            "INSERT INTO `read_marks` (`group_id`, `peer_id`, `received_up_to`) VALUES (?1, ?2, ?3)
             ON CONFLICT (`group_id`, `peer_id`) DO UPDATE SET `received_up_to` = excluded.`received_up_to`",
            params![group, sender, up_to.as_slice()],
        )?;
        Ok(true)
    }

    /// The latest own text in `group` that `peer` has read; it and every earlier own text are read.
    pub fn read_up_to(&self, group: &[u8], peer: &str) -> Result<Option<[u8; 16]>, Error> {
        let mark: Option<Option<Vec<u8>>> = self
            .db
            .query_row(
                "SELECT `received_up_to` FROM `read_marks` WHERE `group_id` = ?1 AND `peer_id` = ?2",
                params![group, peer],
                |row| row.get(0),
            )
            .optional()?;
        mark.flatten()
            .map(|id| id.try_into().map_err(|_| Error::InboxConflict))
            .transpose()
    }
}

/// Whether `id` is a text by `author` in `group` recorded after the one named by the `column`
/// mark for `peer`, or the first such text when there is no mark.
fn later_than_mark(
    db: &Connection,
    group: &[u8],
    peer: &str,
    author: &str,
    id: [u8; 16],
    column: &str,
) -> Result<bool, Error> {
    let rowid = |id: &[u8]| -> Result<Option<i64>, Error> {
        Ok(db
            .query_row(
                "SELECT `rowid` FROM `chat_messages`
                 WHERE `group_id` = ?1 AND `author` = ?2 AND `message_id` = ?3",
                params![group, author, id],
                |row| row.get(0),
            )
            .optional()?)
    };
    let Some(new) = rowid(&id)? else {
        return Ok(false);
    };
    let mark: Option<Vec<u8>> = db
        .query_row(
            &format!(
                "SELECT `{column}` FROM `read_marks` WHERE `group_id` = ?1 AND `peer_id` = ?2"
            ),
            params![group, peer],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    Ok(match mark {
        None => true,
        Some(mark) => rowid(&mark)?.is_none_or(|old| new > old),
    })
}

#[cfg(test)]
mod tests {
    use crate::prepare_put;
    use crate::store::messages::MessageKey;
    use crate::store::tests::temp_db_path;
    use crate::store::{Error, Store};
    use dyapp_identity::Identity;

    const GROUP: &[u8] = &[3];

    fn text(store: &mut Store, author: &str, id: u8) {
        let key = MessageKey {
            group: GROUP.to_vec(),
            author: author.to_owned(),
            id: [id; 16],
        };
        assert!(store.record_chat_text(&key, b"hi").unwrap());
    }

    #[test]
    fn read_receipts_are_opt_in_skip_duplicates_and_blocked_peers() {
        let path = temp_db_path();
        let alice = Identity::generate();
        let b = Identity::generate().peer_id();
        let mut store = Store::open(&path, &alice).unwrap();
        text(&mut store, &b, 1);
        text(&mut store, &b, 2);
        assert!(!store.send_read_receipts().unwrap());
        assert!(!store.draft_read(GROUP, &b, [2; 16]).unwrap());
        store.set_send_read_receipts(true).unwrap();
        // Unknown texts and other groups are never receipted.
        assert!(!store.draft_read(GROUP, &b, [9; 16]).unwrap());
        assert!(!store.draft_read(&[4], &b, [2; 16]).unwrap());
        assert!(store.draft_read(GROUP, &b, [2; 16]).unwrap());
        let write = prepare_put(&[7; 32], vec![5]).unwrap();
        store
            .stage_read(GROUP, &b, [2; 16], b"sealed", std::slice::from_ref(&write))
            .unwrap();
        drop(store);

        let mut store = Store::open(&path, &alice).unwrap();
        // The same latest text, or an earlier one after scrolling back, sends nothing.
        assert!(!store.draft_read(GROUP, &b, [2; 16]).unwrap());
        assert!(!store.draft_read(GROUP, &b, [1; 16]).unwrap());
        assert!(matches!(
            store.stage_read(GROUP, &b, [2; 16], b"sealed", std::slice::from_ref(&write)),
            Err(Error::InteractionConflict)
        ));
        text(&mut store, &b, 3);
        assert!(store.draft_read(GROUP, &b, [3; 16]).unwrap());
        store.set_blocked(&b, true).unwrap();
        assert!(!store.draft_read(GROUP, &b, [3; 16]).unwrap());
    }

    #[test]
    fn received_read_mark_moves_forward_over_own_texts_only() {
        let path = temp_db_path();
        let alice = Identity::generate();
        let (a, b) = (alice.peer_id(), Identity::generate().peer_id());
        let mut store = Store::open(&path, &alice).unwrap();
        text(&mut store, &a, 1);
        text(&mut store, &b, 2);
        text(&mut store, &a, 3);
        assert_eq!(store.read_up_to(GROUP, &b).unwrap(), None);
        // The peer's own text and unknown IDs are not own texts.
        assert!(!store.receive_read(GROUP, &b, [2; 16]).unwrap());
        assert!(!store.receive_read(GROUP, &b, [9; 16]).unwrap());
        assert!(store.receive_read(GROUP, &b, [3; 16]).unwrap());
        assert!(!store.receive_read(GROUP, &b, [1; 16]).unwrap());
        assert!(!store.receive_read(GROUP, &b, [3; 16]).unwrap());
        drop(store);

        let mut store = Store::open(&path, &alice).unwrap();
        assert_eq!(store.read_up_to(GROUP, &b).unwrap(), Some([3; 16]));
        text(&mut store, &a, 4);
        store.set_blocked(&b, true).unwrap();
        assert!(!store.receive_read(GROUP, &b, [4; 16]).unwrap());
        assert_eq!(store.read_up_to(GROUP, &b).unwrap(), Some([3; 16]));
    }

    #[test]
    fn migrates_v11_database_to_read_marks() {
        let path = temp_db_path();
        let alice = Identity::generate();
        let (a, b) = (alice.peer_id(), Identity::generate().peer_id());
        let mut store = Store::open(&path, &alice).unwrap();
        text(&mut store, &a, 1);
        drop(store);
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch("DROP TABLE `read_marks`; PRAGMA user_version = 11;")
            .unwrap();
        drop(db);
        let mut store = Store::open(&path, &alice).unwrap();
        assert!(!store.send_read_receipts().unwrap());
        assert!(store.receive_read(GROUP, &b, [1; 16]).unwrap());
    }
}
