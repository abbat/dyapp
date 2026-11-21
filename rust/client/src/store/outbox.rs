//! Outbox retry bookkeeping: back off a failed write, then hold an accepted one until the
//! recipient's receipt, resending it at a slower pace.

use super::messages::MessageKey;
use super::{Error, Store};
use crate::PreparedEnvelope;
use rusqlite::{params, OptionalExtension};

/// An accepted write with no receipt for this long is given up.
const GIVE_UP_MS: i64 = 30 * 86_400_000;

/// Replication lag allowed before a receipt for a later write proves an earlier one was dropped.
const GRACE_MS: i64 = 60_000;
type RecoveryText = (PreparedEnvelope, MessageKey, Option<u64>, Vec<u8>);
type RecoveryRow = (Vec<u8>, Vec<u8>, Option<i64>, Vec<u8>);

struct AcceptedWrite {
    accepted_at: Option<i64>,
}

impl Store {
    /// Like [`Store::stage_outbound`], for a receipt: node acceptance is enough to drop it.
    ///
    /// # Errors
    ///
    /// Returns an error if the sealed state is empty, a write conflicts with an
    /// existing outbox entry, or database access or transaction commit fails.
    pub fn stage_receipt(
        &mut self,
        sealed_device: &[u8],
        writes: &[PreparedEnvelope],
    ) -> Result<(), Error> {
        self.stage_outbound_inner(None, sealed_device, writes, false)
    }

    /// Schedule a retry after a failed attempt: 1 s after the first, doubling up to 1 h.
    ///
    /// # Errors
    ///
    /// Returns an error if the database update fails or exactly one queued write
    /// does not match the supplied envelope.
    pub fn schedule_retry(&self, write: &PreparedEnvelope, now_ms: i64) -> Result<(), Error> {
        let changed = self.db.execute(
            "UPDATE `outbox` SET `attempts` = `attempts` + 1,
             `retry_at_ms` = ?4 + MIN(1000 << MIN(`attempts`, 12), 3600000)
             WHERE `mailbox` = ?1 AND `envelope_id` = ?2 AND `signed_envelope` = ?3",
            params![
                write.mailbox().as_slice(),
                write.id().as_slice(),
                write.as_bytes(),
                now_ms
            ],
        )?;
        exactly_one(changed)
    }

    /// Node `peer` accepted `write`: drop it if it needs no receipt, else resend it 1 h after the
    /// first acceptance, doubling up to a day.
    ///
    /// # Errors
    ///
    /// Returns an error if the queued write no longer matches the envelope, or
    /// database updates or transaction commit fail.
    pub fn accepted(
        &mut self,
        write: &PreparedEnvelope,
        now_ms: i64,
        peer: &str,
    ) -> Result<(), Error> {
        let tx = self.db.transaction()?;
        let deleted = tx.execute(
            "DELETE FROM `outbox` WHERE `mailbox` = ?1 AND `envelope_id` = ?2
             AND `signed_envelope` = ?3 AND `awaits_receipt` = 0",
            params![
                write.mailbox().as_slice(),
                write.id().as_slice(),
                write.as_bytes()
            ],
        )?;
        if deleted == 1 {
            tx.commit()?;
            return Ok(());
        }
        // SET reads the old row, so the first acceptance starts the pace over at 1 h.
        let changed = tx.execute(
            "UPDATE `outbox` SET
             `retry_at_ms` = ?4 + MIN(3600000 << CASE WHEN `accepted_at_ms` IS NULL THEN 0
                 ELSE MIN(`attempts`, 5) END, 86400000),
             `attempts` = CASE WHEN `accepted_at_ms` IS NULL THEN 1 ELSE `attempts` + 1 END,
             `accepted_at_ms` = COALESCE(`accepted_at_ms`, ?4),
             `accepted_by` = ?5, `last_accepted_ms` = ?4
             WHERE `mailbox` = ?1 AND `envelope_id` = ?2 AND `signed_envelope` = ?3",
            params![
                write.mailbox().as_slice(),
                write.id().as_slice(),
                write.as_bytes(),
                now_ms,
                peer
            ],
        )?;
        exactly_one(changed)?;
        advance_chat_delivery(&tx, &self.owner_peer_id, write.mailbox(), write.id(), 1)?;
        tx.commit()?;
        Ok(())
    }

    /// The node that last accepted `write`, unless a receipt blamed it since.
    ///
    /// # Errors
    ///
    /// Returns an error if querying or decoding the accepting node fails.
    pub fn accepted_by(&self, write: &PreparedEnvelope) -> Result<Option<String>, Error> {
        Ok(self
            .db
            .query_row(
                "SELECT `accepted_by` FROM `outbox` WHERE `mailbox` = ?1 AND `envelope_id` = ?2",
                params![write.mailbox().as_slice(), write.id().as_slice()],
                |row| row.get(0),
            )
            .optional()?
            .flatten())
    }

    /// Current unchanged text that can be encrypted again after a recipient's negative receipt.
    ///
    /// # Errors
    /// Returns an error if the stored write or text is malformed, cannot be read, or uses an
    /// unrepresentable timestamp.
    pub fn text_for_recovery(
        &self,
        mailbox: &[u8; 32],
        group: &[u8],
        id: &[u8; 16],
    ) -> Result<Option<RecoveryText>, Error> {
        let row: Option<RecoveryRow> = self
            .db
            .query_row(
                "SELECT `o`.`signed_envelope`, `m`.`message_id`, `m`.`original_hlc`, `m`.`text`
                 FROM `outbox` AS `o` JOIN `chat_messages` AS `m`
                 ON `m`.`group_id` = `o`.`chat_group`
                 AND `m`.`message_id` = `o`.`chat_message_id`
                 WHERE `o`.`mailbox` = ?1 AND `o`.`envelope_id` = ?2 AND `o`.`chat_group` = ?3
                 AND `m`.`author` = ?4 AND `m`.`deleted` = 0
                 AND `m`.`delivery_generation` = `o`.`chat_generation_id`",
                params![mailbox.as_slice(), id.as_slice(), group, self.owner_peer_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        row.map(|(bytes, message_id, hlc, text)| {
            let id: [u8; 16] = message_id.try_into().map_err(|_| Error::OutboxConflict)?;
            let hlc = hlc
                .map(|value| u64::try_from(value).map_err(|_| Error::ClockRange))
                .transpose()?;
            let key = MessageKey {
                group: group.to_vec(),
                author: self.owner_peer_id.clone(),
                id,
            };
            let old = PreparedEnvelope::from_bytes(bytes).map_err(|_| Error::OutboxConflict)?;
            Ok((old, key, hlc, text))
        })
        .transpose()
    }

    /// The owner of `mailbox` received writes `ids`: drop those still waiting. Returns how many,
    /// and the nodes that dropped earlier writes to `mailbox`: those writes still wait though
    /// their latest copy was accepted before the received ones. They become due at once.
    ///
    /// # Errors
    ///
    /// Returns an error if querying, decoding, or updating stored delivery and
    /// outbox state fails, or the transaction cannot be committed.
    ///
    pub fn receipted(
        &mut self,
        mailbox: &[u8; 32],
        ids: &[[u8; 16]],
    ) -> Result<(usize, Vec<String>), Error> {
        self.receipted_inner(mailbox, ids, None)
    }

    /// Apply receipts only to writes from the MLS group that carried them.
    ///
    /// # Errors
    /// Returns an error if stored delivery or outbox state cannot be read or updated.
    pub fn receipted_in_group(
        &mut self,
        mailbox: &[u8; 32],
        group: &[u8],
        ids: &[[u8; 16]],
    ) -> Result<(usize, Vec<String>), Error> {
        self.receipted_inner(mailbox, ids, Some(group))
    }

    fn receipted_inner(
        &mut self,
        mailbox: &[u8; 32],
        ids: &[[u8; 16]],
        group: Option<&[u8]>,
    ) -> Result<(usize, Vec<String>), Error> {
        let tx = self.db.transaction()?;
        let mut fetched_after: Option<i64> = None;
        let mut dropped = 0usize;
        for id in ids {
            let Some(write) = accepted_for_group(&tx, mailbox, id, group)? else {
                continue;
            };
            if let Some(at) = write.accepted_at {
                fetched_after = Some(fetched_after.map_or(at, |t| t.min(at)));
            }
            advance_chat_delivery(&tx, &self.owner_peer_id, mailbox, id, 2)?;
            dropped = dropped.saturating_add(tx.execute(
                "DELETE FROM `outbox`
                 WHERE `mailbox` = ?1 AND `envelope_id` = ?2 AND `awaits_receipt` = 1",
                params![mailbox.as_slice(), id.as_slice()],
            )?);
        }
        let Some(fetched_after) = fetched_after else {
            tx.commit()?;
            return Ok((dropped, Vec::new()));
        };
        let overtaken =
            "WHERE `mailbox` = ?1 AND `awaits_receipt` = 1 AND `accepted_by` IS NOT NULL
             AND `last_accepted_ms` < ?2";
        let row = params![mailbox.as_slice(), fetched_after.saturating_sub(GRACE_MS)];
        let faulty = super::collect_bounded(
            tx.prepare(&format!("SELECT `accepted_by` FROM `outbox` {overtaken}"))?
                .query_map(row, |row| row.get(0))?,
            super::MAX_RESULT_ROWS,
        )?;
        // Clearing `accepted_by` faults a node once per copy; the resend records the next node.
        tx.execute(
            &format!("UPDATE `outbox` SET `retry_at_ms` = 0, `accepted_by` = NULL {overtaken}"),
            row,
        )?;
        tx.commit()?;
        Ok((dropped, faulty))
    }

    /// Give up writes first accepted 30 or more days before `now_ms` and writes past their own
    /// expiry, such as a presence. Returns how many.
    ///
    /// # Errors
    ///
    /// Returns an error if deleting expired outbox entries from the database fails.
    pub fn expire_outbound(&self, now_ms: i64) -> Result<usize, Error> {
        Ok(self.db.execute(
            "DELETE FROM `outbox` WHERE `accepted_at_ms` <= ?1 OR `expires_at_ms` <= ?2",
            [now_ms.saturating_sub(GIVE_UP_MS), now_ms],
        )?)
    }
}

fn accepted_for_group(
    tx: &rusqlite::Transaction<'_>,
    mailbox: &[u8; 32],
    id: &[u8; 16],
    group: Option<&[u8]>,
) -> Result<Option<AcceptedWrite>, Error> {
    let row: Option<(Option<i64>, Option<Vec<u8>>)> = tx
        .query_row(
            "SELECT `accepted_at_ms`, `chat_group` FROM `outbox`
             WHERE `mailbox` = ?1 AND `envelope_id` = ?2 AND `awaits_receipt` = 1",
            params![mailbox.as_slice(), id.as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    Ok(row.and_then(|(accepted, write_group)| {
        group
            .is_none_or(|expected| write_group.as_deref() == Some(expected))
            .then_some(AcceptedWrite {
                accepted_at: accepted,
            })
    }))
}

/// Advance only the still-current local generation; old text/operation receipts cannot repaint it.
fn advance_chat_delivery(
    tx: &rusqlite::Transaction<'_>,
    owner: &str,
    mailbox: &[u8; 32],
    envelope_id: &[u8; 16],
    state: i64,
) -> Result<(), Error> {
    tx.execute(
        "UPDATE `chat_messages` SET `delivery_state` = MAX(`delivery_state`, ?4)
         WHERE `author` = ?3 AND EXISTS (
             SELECT 1 FROM `outbox` WHERE `mailbox` = ?1 AND `envelope_id` = ?2
             AND `awaits_receipt` = 1
             AND `chat_group` = `chat_messages`.`group_id`
             AND `chat_message_id` = `chat_messages`.`message_id`
             AND `chat_generation_id` = `chat_messages`.`delivery_generation`
         )",
        params![mailbox.as_slice(), envelope_id.as_slice(), owner, state],
    )?;
    tx.execute(
        "UPDATE `chat_attachments` SET `delivery_state` = MAX(`delivery_state`, ?4)
         WHERE `author` = ?3 AND EXISTS (
             SELECT 1 FROM `outbox` WHERE `mailbox` = ?1 AND `envelope_id` = ?2
             AND `awaits_receipt` = 1
             AND `chat_group` = `chat_attachments`.`group_id`
             AND `chat_message_id` = `chat_attachments`.`message_id`
             AND `chat_generation_id` = `chat_attachments`.`delivery_generation`
         )",
        params![mailbox.as_slice(), envelope_id.as_slice(), owner, state],
    )?;
    Ok(())
}

fn exactly_one(changed: usize) -> Result<(), Error> {
    if changed != 1 {
        return Err(Error::OutboxConflict);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::tests::temp_db_path;
    use super::super::{Error, Store};
    use super::{GIVE_UP_MS, GRACE_MS};
    use crate::prepare_put;
    use dyapp_identity::Identity;
    use rusqlite::params;

    #[test]
    fn failed_writes_back_off_up_to_an_hour() {
        let mut store = Store::open(&temp_db_path(), &Identity::try_generate().unwrap()).unwrap();
        let write = prepare_put(&[1; 32], vec![9]).unwrap();
        store
            .stage_outbound(b"mls", std::slice::from_ref(&write))
            .unwrap();
        let mut now = 0;
        for delay in [1_000, 2_000, 4_000, 8_000] {
            store.schedule_retry(&write, now).unwrap();
            assert!(store.due_outbound(now + delay - 1).unwrap().is_empty());
            assert_eq!(store.due_outbound(now + delay).unwrap().len(), 1);
            now += delay;
        }
        for _ in 0..60 {
            store.schedule_retry(&write, now).unwrap();
        }
        assert!(store.due_outbound(now + 3_599_999).unwrap().is_empty());
        assert_eq!(store.due_outbound(now + 3_600_000).unwrap().len(), 1);

        let other = prepare_put(&[1; 32], vec![8]).unwrap();
        assert!(matches!(
            store.schedule_retry(&other, now),
            Err(Error::OutboxConflict)
        ));
    }
    #[test]
    fn accepted_writes_wait_for_a_receipt_and_receipts_do_not() {
        let mut store = Store::open(&temp_db_path(), &Identity::try_generate().unwrap()).unwrap();
        let (mailbox, other) = ([1; 32], [2; 32]);
        let message = prepare_put(&mailbox, vec![9]).unwrap();
        let receipt = prepare_put(&other, vec![8]).unwrap();
        store
            .stage_outbound(b"mls", std::slice::from_ref(&message))
            .unwrap();
        store
            .stage_receipt(b"mls", std::slice::from_ref(&receipt))
            .unwrap();
        store.schedule_retry(&message, 0).unwrap();
        store.accepted(&receipt, 0, "a").unwrap();
        let mut now = 10_000;
        store.accepted(&message, now, "a").unwrap();
        assert!(store.due_outbound(now + 3_599_999).unwrap().is_empty());
        for delay in [
            3_600_000, 7_200_000, 14_400_000, 28_800_000, 57_600_000, 86_400_000,
        ] {
            assert_eq!(due_bytes(&store, now + delay), [message.as_bytes()]);
            assert!(store.due_outbound(now + delay - 1).unwrap().is_empty());
            now += delay;
            store.accepted(&message, now, "a").unwrap();
        }
        assert!(store.due_outbound(now + 86_399_999).unwrap().is_empty());

        assert_eq!(store.receipted(&other, &[*message.id()]).unwrap().0, 0);
        assert_eq!(store.receipted(&mailbox, &[*message.id()]).unwrap().0, 1);
        assert_eq!(store.receipted(&mailbox, &[*message.id()]).unwrap().0, 0);
        assert!(store.due_outbound(i64::MAX).unwrap().is_empty());
    }

    #[test]
    fn accepted_writes_without_a_receipt_expire_after_30_days() {
        let mut store = Store::open(&temp_db_path(), &Identity::try_generate().unwrap()).unwrap();
        let (sent, fresh) = (
            prepare_put(&[1; 32], vec![9]).unwrap(),
            prepare_put(&[1; 32], vec![7]).unwrap(),
        );
        store
            .stage_outbound(b"mls", &[sent.clone(), fresh.clone()])
            .unwrap();
        store.accepted(&sent, 0, "a").unwrap();
        store.accepted(&sent, 3_600_000, "b").unwrap();
        assert_eq!(store.expire_outbound(GIVE_UP_MS - 1).unwrap(), 0);
        assert_eq!(store.expire_outbound(GIVE_UP_MS).unwrap(), 1);
        assert_eq!(due_bytes(&store, i64::MAX), [fresh.as_bytes()]);
    }

    #[test]
    fn a_receipt_for_a_later_write_blames_the_node_of_an_earlier_one() {
        fn blamed(store: &mut Store, ids: &[[u8; 16]]) -> (usize, Vec<String>) {
            store.receipted(&[1; 32], ids).unwrap()
        }
        let mut store = Store::open(&temp_db_path(), &Identity::try_generate().unwrap()).unwrap();
        let mailbox = [1; 32];
        let [early, close, recent, fresh, later] =
            [9, 8, 7, 6, 5].map(|byte| prepare_put(&mailbox, vec![byte]).unwrap());
        let elsewhere = prepare_put(&[2; 32], vec![9]).unwrap();
        let writes = [&early, &close, &recent, &elsewhere].map(std::clone::Clone::clone);
        store.stage_outbound(b"mls", &writes).unwrap();
        store.accepted(&early, 0, "x").unwrap();
        store.accepted(&elsewhere, 0, "z").unwrap();
        store.accepted(&close, 180_000 - GRACE_MS, "c").unwrap();
        store.accepted(&recent, 180_000, "y").unwrap();
        assert_eq!(store.accepted_by(&early).unwrap().as_deref(), Some("x"));
        assert_eq!(
            blamed(&mut store, &[*recent.id()]),
            (1, vec!["x".to_owned()])
        );
        assert_eq!(due_bytes(&store, 0), [early.as_bytes()]);
        assert_eq!(store.accepted_by(&early).unwrap(), None);
        assert_eq!(blamed(&mut store, &[*recent.id()]), (0, vec![]));

        // x is blamed once for its copy; the resend's node only for its own.
        store
            .stage_outbound(b"mls", &[fresh.clone(), later.clone()])
            .unwrap();
        store.accepted(&fresh, 400_000, "v").unwrap();
        assert_eq!(
            blamed(&mut store, &[*fresh.id()]),
            (1, vec!["c".to_owned()])
        );
        store.accepted(&early, 500_000, "u").unwrap();
        store.accepted(&later, 700_000, "w").unwrap();
        assert_eq!(
            blamed(&mut store, &[*later.id()]),
            (1, vec!["u".to_owned()])
        );
    }

    #[test]
    fn a_receipt_applies_only_to_its_mls_group() {
        let mut store = Store::open(&temp_db_path(), &Identity::try_generate().unwrap()).unwrap();
        let write = prepare_put(&[1; 32], vec![9]).unwrap();
        store
            .stage_outbound(b"mls", std::slice::from_ref(&write))
            .unwrap();
        store
            .db
            .execute(
                "UPDATE `outbox` SET `chat_group` = ?3 WHERE `mailbox` = ?1 AND `envelope_id` = ?2",
                params![
                    write.mailbox().as_slice(),
                    write.id().as_slice(),
                    [1u8].as_slice()
                ],
            )
            .unwrap();
        store.accepted(&write, 0, "node").unwrap();
        assert_eq!(
            store
                .receipted_in_group(&[1; 32], &[2], &[*write.id()])
                .unwrap()
                .0,
            0
        );
        assert_eq!(
            store
                .receipted_in_group(&[1; 32], &[1], &[*write.id()])
                .unwrap()
                .0,
            1
        );
    }

    #[test]
    fn recovery_refuses_a_superseded_delivery_generation() {
        use crate::store::messages::MessageKey;

        let owner = Identity::try_generate().unwrap();
        let mut store = Store::open(&temp_db_path(), &owner).unwrap();
        let key = MessageKey {
            group: vec![1],
            author: owner.peer_id(),
            id: [2; 16],
        };
        store.record_chat_text(&key, b"current").unwrap();
        let write = prepare_put(&[3; 32], vec![9]).unwrap();
        store
            .stage_outbound(b"mls", std::slice::from_ref(&write))
            .unwrap();
        store
            .db
            .execute(
                "UPDATE `outbox` SET `chat_group` = ?3, `chat_message_id` = ?4,
                 `chat_generation_id` = ?4 WHERE `mailbox` = ?1 AND `envelope_id` = ?2",
                params![
                    write.mailbox().as_slice(),
                    write.id().as_slice(),
                    key.group.as_slice(),
                    key.id.as_slice()
                ],
            )
            .unwrap();
        store
            .db
            .execute(
                "UPDATE `chat_messages` SET `delivery_generation` = ?1, `delivery_state` = 0",
                [key.id.as_slice()],
            )
            .unwrap();
        assert!(store
            .text_for_recovery(&[3; 32], &[1], write.id())
            .unwrap()
            .is_some());
        store
            .db
            .execute(
                "UPDATE `chat_messages` SET `delivery_generation` = zeroblob(16)",
                [],
            )
            .unwrap();
        assert!(store
            .text_for_recovery(&[3; 32], &[1], write.id())
            .unwrap()
            .is_none());
    }

    #[test]
    fn equal_retry_deadlines_keep_staging_order() {
        let mut store = Store::open(&temp_db_path(), &Identity::try_generate().unwrap()).unwrap();
        let mailbox = [1; 32];
        let writes = [
            prepare_put(&mailbox, vec![1]).unwrap(),
            prepare_put(&mailbox, vec![2]).unwrap(),
            prepare_put(&mailbox, vec![3]).unwrap(),
        ];
        store.stage_outbound(b"mls", &writes).unwrap();
        assert_eq!(
            due_bytes(&store, 0),
            writes
                .iter()
                .map(|write| write.as_bytes().to_vec())
                .collect::<Vec<_>>()
        );
    }

    fn due_bytes(store: &Store, now_ms: i64) -> Vec<Vec<u8>> {
        let due = store.due_outbound(now_ms).unwrap();
        due.iter().map(|write| write.as_bytes().to_vec()).collect()
    }
}
