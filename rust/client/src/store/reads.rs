//! Read receipts in one-to-one chats: the local opt-in setting, the latest peer text receipted
//! and the latest own text the peer has read ([read receipts](../../../docs/design/read-receipts.md)).
//! Receipt order uses `chat_messages` rowid, now a non-reused local arrival ordinal. Clear chat
//! may remove the marked text; the next text then starts a new receipt sequence for that group.

use super::read_projection::ReadCoverage;
use super::{collect_bounded, Conversation, Error, Store, MAX_RESULT_ROWS};
use crate::PreparedEnvelope;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::BTreeMap;

const SETTING: &str = "send_read_receipts";

// A group and the exact peer message that the owner can explicitly receipt.
type ReadReceiptTarget = (Vec<u8>, [u8; 16]);

pub(super) const SCHEMA: &str = r"
CREATE TABLE `read_marks` (
    `group_id` BLOB NOT NULL CHECK (length(`group_id`) > 0),
    `peer_id` TEXT NOT NULL CHECK (length(`peer_id`) > 0),
    `sent_up_to` BLOB CHECK (`sent_up_to` IS NULL OR length(`sent_up_to`) = 16),
    `received_up_to` BLOB CHECK (`received_up_to` IS NULL OR length(`received_up_to`) = 16),
    PRIMARY KEY (`group_id`, `peer_id`)
) STRICT;
";

impl Store {
    /// Snapshot own unmodified messages covered by this peer's group-bound read marks.
    /// Coverage uses persisted arrival ordinals, never HLC or presentation order.
    ///
    /// # Errors
    /// Returns storage, malformed-message-key or bounded-result errors.
    pub fn read_coverage(&self, peer: &str) -> Result<ReadCoverage, Error> {
        let mut query = self.db.prepare(
            "SELECT `m`.`group_id`, `m`.`message_id`
             FROM `chat_messages` AS `m`
             JOIN `read_marks` AS `r`
               ON `r`.`group_id` = `m`.`group_id` AND `r`.`peer_id` = ?1
             JOIN `chat_messages` AS `marked`
               ON `marked`.`group_id` = `r`.`group_id`
              AND `marked`.`author` = `m`.`author`
              AND `marked`.`message_id` = `r`.`received_up_to`
             WHERE `m`.`author` = ?2
               AND `m`.`arrival_order` <= `marked`.`arrival_order`
               AND `m`.`edited` = 0 AND `m`.`deleted` = 0",
        )?;
        let rows = query.query_map(params![peer, &self.owner_peer_id], |row| {
            Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?))
        })?;
        let keys = collect_bounded(
            rows.map(|row| -> Result<_, Error> {
                let (group, id) = row?;
                Ok((group, id.try_into().map_err(|_| Error::InboxConflict)?))
            }),
            MAX_RESULT_ROWS,
        )?;
        Ok(ReadCoverage::new(&self.owner_peer_id, keys))
    }

    /// Latest peer text per group that currently owes a receipt, in stable group order.
    /// Uses canonical chat-history anchor selection and existing opt-in/block/mark checks.
    /// Reading this list neither stages a receipt nor connects to the network.
    ///
    /// # Errors
    /// Returns existing conversation, history or read-admission storage errors.
    pub fn due_read_receipts(&self, peer: &str) -> Result<Vec<ReadReceiptTarget>, Error> {
        if self.conversation(peer)? != Conversation::Open {
            return Ok(Vec::new());
        }
        let latest: BTreeMap<_, _> = self
            .current_chat_messages(peer)?
            .into_iter()
            .filter(|message| message.key.author == peer)
            .map(|message| (message.key.group, message.key.id))
            .collect();
        let mut due = Vec::new();
        for (group, id) in latest {
            if self.draft_read(&group, peer, id)? {
                due.push((group, id));
            }
        }
        Ok(due)
    }

    /// Whether read receipts are sent; absent means off.
    ///
    /// # Errors
    ///
    /// Returns an error if reading the setting fails or its format or value is invalid.
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
    ///
    /// # Errors
    ///
    /// Returns an error if writing the setting to the database fails.
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
    ///
    /// # Errors
    ///
    /// Returns an error if the receipt setting is invalid or reading settings,
    /// block state, message ordering, or the previous receipt mark fails.
    pub fn draft_read(&self, group: &[u8], peer: &str, seen: [u8; 16]) -> Result<bool, Error> {
        if peer == self.owner_peer_id || !self.send_read_receipts()? || self.blocked(peer)? {
            return Ok(false);
        }
        later_than_mark(&self.db, group, peer, peer, seen, "sent_up_to")
    }

    /// Commit a drafted read receipt, its mark and the encrypted outbound writes in one
    /// transaction. The receipt has no expiry and gets no delivery receipt. A stale draft or a
    /// peer no longer allowed changes nothing.
    ///
    /// # Errors
    ///
    /// Returns an error if there are no writes, the draft is no longer eligible,
    /// eligibility checks fail as described in [`Self::draft_read`], staging the
    /// outbound state conflicts, or database access or transaction commit fails.
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
    ///
    /// # Errors
    ///
    /// Returns an error if reading block state, message ordering, or the previous
    /// mark fails, or writing the new receipt mark fails.
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
    ///
    /// # Errors
    ///
    /// Returns an error if querying or decoding the receipt mark fails, or its
    /// message ID has an invalid length.
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
        let test_dir = dyapp_test_support::test_dir().unwrap();
        let path = test_dir.path().join("client-1.db");
        let alice = Identity::try_generate().unwrap();
        let b = Identity::try_generate().unwrap().peer_id();
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

    fn set_display_hlc(store: &Store, group: &[u8], own: &str, id: u8, hlc: i64) {
        store
            .db
            .execute(
                "UPDATE `chat_messages` SET `original_hlc` = ?1
             WHERE `group_id` = ?2 AND `author` = ?3 AND `message_id` = ?4",
                rusqlite::params![hlc, group, own, [id; 16].as_slice()],
            )
            .unwrap();
    }

    fn assert_read_flag(store: &Store, peer: &str, own: &str, id: u8, expected: bool) {
        assert_eq!(
            store
                .read_coverage(peer)
                .unwrap()
                .read_flag(GROUP, own, &[id; 16], false, false),
            Some(expected),
            "logical ID {id}",
        );
    }

    #[test]
    fn read_coverage_uses_arrival_when_a_late_text_has_an_older_hlc() {
        let dir = dyapp_test_support::test_dir().unwrap();
        let owner = Identity::try_generate().unwrap();
        let own = owner.peer_id();
        let peer = Identity::try_generate().unwrap().peer_id();
        let mut store = Store::open(&dir.path().join("coverage.db"), &owner).unwrap();
        crate::store::tests::catalog_group(&mut store, GROUP, &peer);
        text(&mut store, &own, 9);
        text(&mut store, &own, 1);
        set_display_hlc(&store, GROUP, &own, 9, 200);
        set_display_hlc(&store, GROUP, &own, 1, 100);
        assert_eq!(
            store
                .current_chat_messages(&peer)
                .unwrap()
                .iter()
                .map(|message| message.key.id[0])
                .collect::<Vec<_>>(),
            [1, 9]
        );
        assert!(store.receive_read(GROUP, &peer, [9; 16]).unwrap());
        assert_read_flag(&store, &peer, &own, 9, true);
        assert_read_flag(&store, &peer, &own, 1, false);
        store
            .db
            .execute(
                "UPDATE `chat_messages` SET `edited` = 1 WHERE `message_id` = ?1",
                [[9u8; 16].as_slice()],
            )
            .unwrap();
        assert_read_flag(&store, &peer, &own, 9, false);
        store.clear_chat_pair(&peer).unwrap();
        text(&mut store, &own, 2);
        assert_read_flag(&store, &peer, &own, 2, false);
    }

    fn open_for_reads(store: &mut Store, own: &str, peer: &str) {
        use crate::interactions::{Interaction, Kind};
        for (sender, target, id) in [(own, peer, 1u8), (peer, own, 2u8)] {
            assert!(store
                .apply_interaction(
                    sender,
                    target,
                    &[1; 32],
                    Interaction {
                        kind: Kind::Like,
                        event_id: [id; 16],
                        hlc: u64::from(id) << 16,
                    }
                )
                .unwrap());
        }
    }

    fn ordered_peer_text(store: &mut Store, group: &[u8], peer: &str, id: u8, hlc: i64) {
        crate::store::tests::catalog_group(store, group, peer);
        let key = MessageKey {
            group: group.to_vec(),
            author: peer.to_owned(),
            id: [id; 16],
        };
        assert!(store.record_chat_text(&key, b"text").unwrap());
        set_display_hlc(store, group, peer, id, hlc);
    }

    #[test]
    fn due_receipts_preserve_history_anchors_group_order_and_privacy_gates() {
        let dir = dyapp_test_support::test_dir().unwrap();
        let owner = Identity::try_generate().unwrap();
        let own = owner.peer_id();
        let peer = Identity::try_generate().unwrap().peer_id();
        let mut store = Store::open(&dir.path().join("due.db"), &owner).unwrap();
        let other = &[4u8][..];
        for (group, id, hlc) in [(GROUP, 9, 200), (GROUP, 1, 100), (other, 2, 50)] {
            ordered_peer_text(&mut store, group, &peer, id, hlc);
        }
        assert_eq!(store.due_read_receipts(&peer).unwrap(), []);
        open_for_reads(&mut store, &own, &peer);
        assert_eq!(store.due_read_receipts(&peer).unwrap(), []);
        store.set_send_read_receipts(true).unwrap();
        let expected = vec![(GROUP.to_vec(), [9; 16]), (other.to_vec(), [2; 16])];
        assert_eq!(store.due_read_receipts(&peer).unwrap(), expected);
        assert_eq!(store.due_read_receipts(&peer).unwrap(), expected);
        let write = prepare_put(&[7; 32], vec![5]).unwrap();
        store
            .stage_read(
                GROUP,
                &peer,
                [9; 16],
                b"sealed",
                std::slice::from_ref(&write),
            )
            .unwrap();
        assert_eq!(
            store.due_read_receipts(&peer).unwrap(),
            [(other.to_vec(), [2; 16])]
        );
        store.set_blocked(&peer, true).unwrap();
        assert!(store.due_read_receipts(&peer).unwrap().is_empty());
        store.set_blocked(&peer, false).unwrap();
        store.set_send_read_receipts(false).unwrap();
        assert!(store.due_read_receipts(&peer).unwrap().is_empty());
    }

    #[test]
    fn received_read_mark_moves_forward_over_own_texts_only() {
        let test_dir = dyapp_test_support::test_dir().unwrap();
        let path = test_dir.path().join("client-1.db");
        let alice = Identity::try_generate().unwrap();
        let (a, b) = (alice.peer_id(), Identity::try_generate().unwrap().peer_id());
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
}
