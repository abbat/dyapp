//! Outbox retry bookkeeping: back off a failed write, then hold an accepted one until the
//! recipient's receipt, resending it at a slower pace.

use super::{Error, Store};
use crate::PreparedEnvelope;
use rusqlite::{params, OptionalExtension};

/// An accepted write with no receipt for this long is given up.
const GIVE_UP_MS: i64 = 30 * 86_400_000;

/// Replication lag allowed before a receipt for a later write proves an earlier one was dropped.
const GRACE_MS: i64 = 60_000;

/// Rows staged before receipts keep waiting for one; `accepted_at_ms` is NULL until a node takes
/// the write.
pub(super) const SCHEMA_V6: &str = r"
ALTER TABLE `outbox` ADD COLUMN `awaits_receipt` INTEGER NOT NULL DEFAULT 1
    CHECK (`awaits_receipt` IN (0, 1));
ALTER TABLE `outbox` ADD COLUMN `accepted_at_ms` INTEGER;
PRAGMA user_version = 6;
";

/// The latest node to accept each write and when; NULL once the node is faulted for it.
pub(super) const SCHEMA_V7: &str = r"
ALTER TABLE `outbox` ADD COLUMN `accepted_by` TEXT;
ALTER TABLE `outbox` ADD COLUMN `last_accepted_ms` INTEGER;
PRAGMA user_version = 7;
";

impl Store {
    /// Like [`Store::stage_outbound`], for a receipt: node acceptance is enough to drop it.
    pub fn stage_receipt(
        &mut self,
        sealed_device: &[u8],
        writes: &[PreparedEnvelope],
    ) -> Result<(), Error> {
        self.stage_outbound_inner(None, sealed_device, writes, false)
    }

    /// Schedule a retry after a failed attempt: 1 s after the first, doubling up to 1 h.
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
    pub fn accepted(&self, write: &PreparedEnvelope, now_ms: i64, peer: &str) -> Result<(), Error> {
        let row = params![
            write.mailbox().as_slice(),
            write.id().as_slice(),
            write.as_bytes()
        ];
        let deleted = self.db.execute(
            "DELETE FROM `outbox` WHERE `mailbox` = ?1 AND `envelope_id` = ?2
             AND `signed_envelope` = ?3 AND `awaits_receipt` = 0",
            row,
        )?;
        if deleted == 1 {
            return Ok(());
        }
        // SET reads the old row, so the first acceptance starts the pace over at 1 h.
        let changed = self.db.execute(
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
        exactly_one(changed)
    }

    /// The owner of `mailbox` received writes `ids`: drop those still waiting. Returns how many,
    /// and the nodes that dropped earlier writes to `mailbox`: those writes still wait though
    /// their latest copy was accepted before the received ones. They become due at once.
    pub fn receipted(
        &self,
        mailbox: &[u8; 32],
        ids: &[[u8; 16]],
    ) -> Result<(usize, Vec<String>), Error> {
        let mut fetched_after: Option<i64> = None;
        let mut dropped = 0;
        for id in ids {
            let at: Option<i64> = self
                .db
                .query_row(
                    "SELECT `accepted_at_ms` FROM `outbox`
                     WHERE `mailbox` = ?1 AND `envelope_id` = ?2 AND `awaits_receipt` = 1",
                    params![mailbox.as_slice(), id.as_slice()],
                    |row| row.get(0),
                )
                .optional()?
                .flatten();
            if let Some(at) = at {
                fetched_after = Some(fetched_after.map_or(at, |t| t.min(at)));
            }
            dropped += self.db.execute(
                "DELETE FROM `outbox`
                 WHERE `mailbox` = ?1 AND `envelope_id` = ?2 AND `awaits_receipt` = 1",
                params![mailbox.as_slice(), id.as_slice()],
            )?;
        }
        let Some(fetched_after) = fetched_after else {
            return Ok((dropped, Vec::new()));
        };
        let overtaken =
            "WHERE `mailbox` = ?1 AND `awaits_receipt` = 1 AND `accepted_by` IS NOT NULL
             AND `last_accepted_ms` < ?2";
        let row = params![mailbox.as_slice(), fetched_after - GRACE_MS];
        let faulty = self
            .db
            .prepare(&format!("SELECT `accepted_by` FROM `outbox` {overtaken}"))?
            .query_map(row, |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        // Clearing `accepted_by` faults a node once per copy; the resend records the next node.
        self.db.execute(
            &format!("UPDATE `outbox` SET `retry_at_ms` = 0, `accepted_by` = NULL {overtaken}"),
            row,
        )?;
        Ok((dropped, faulty))
    }

    /// Give up writes first accepted 30 or more days before `now_ms`. Returns how many.
    pub fn expire_outbound(&self, now_ms: i64) -> Result<usize, Error> {
        Ok(self.db.execute(
            "DELETE FROM `outbox` WHERE `accepted_at_ms` <= ?1",
            [now_ms.saturating_sub(GIVE_UP_MS)],
        )?)
    }
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

    #[test]
    fn failed_writes_back_off_up_to_an_hour() {
        let mut store = Store::open(&temp_db_path(), &Identity::generate()).unwrap();
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
        let mut store = Store::open(&temp_db_path(), &Identity::generate()).unwrap();
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
        let mut store = Store::open(&temp_db_path(), &Identity::generate()).unwrap();
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
        let mut store = Store::open(&temp_db_path(), &Identity::generate()).unwrap();
        let mailbox = [1; 32];
        let [early, close, recent, fresh, later] =
            [9, 8, 7, 6, 5].map(|byte| prepare_put(&mailbox, vec![byte]).unwrap());
        let elsewhere = prepare_put(&[2; 32], vec![9]).unwrap();
        let writes = [&early, &close, &recent, &elsewhere].map(|w| w.clone());
        store.stage_outbound(b"mls", &writes).unwrap();
        store.accepted(&early, 0, "x").unwrap();
        store.accepted(&elsewhere, 0, "z").unwrap();
        store.accepted(&close, 180_000 - GRACE_MS, "c").unwrap();
        store.accepted(&recent, 180_000, "y").unwrap();
        assert_eq!(blamed(&store, &[*recent.id()]), (1, vec!["x".to_owned()]));
        assert_eq!(due_bytes(&store, 0), [early.as_bytes()]);
        assert_eq!(blamed(&store, &[*recent.id()]), (0, vec![]));

        // x is blamed once for its copy; the resend's node only for its own.
        store
            .stage_outbound(b"mls", &[fresh.clone(), later.clone()])
            .unwrap();
        store.accepted(&fresh, 400_000, "v").unwrap();
        assert_eq!(blamed(&store, &[*fresh.id()]), (1, vec!["c".to_owned()]));
        store.accepted(&early, 500_000, "u").unwrap();
        store.accepted(&later, 700_000, "w").unwrap();
        assert_eq!(blamed(&store, &[*later.id()]), (1, vec!["u".to_owned()]));

        fn blamed(store: &Store, ids: &[[u8; 16]]) -> (usize, Vec<String>) {
            store.receipted(&[1; 32], ids).unwrap()
        }
    }

    fn due_bytes(store: &Store, now_ms: i64) -> Vec<Vec<u8>> {
        let due = store.due_outbound(now_ms).unwrap();
        due.iter().map(|write| write.as_bytes().to_vec()).collect()
    }
}
