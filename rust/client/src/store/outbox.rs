//! Outbox retry bookkeeping: back off a failed write, drop an accepted one.

use super::{Error, Store};
use crate::PreparedEnvelope;
use rusqlite::params;

impl Store {
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

    /// Drop a write the node accepted.
    pub fn remove_outbound(&self, write: &PreparedEnvelope) -> Result<(), Error> {
        let changed = self.db.execute(
            "DELETE FROM `outbox`
             WHERE `mailbox` = ?1 AND `envelope_id` = ?2 AND `signed_envelope` = ?3",
            params![
                write.mailbox().as_slice(),
                write.id().as_slice(),
                write.as_bytes()
            ],
        )?;
        exactly_one(changed)
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
    use crate::prepare_put;
    use dyapp_identity::Identity;

    #[test]
    fn failed_writes_back_off_and_accepted_ones_leave() {
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

        store.remove_outbound(&write).unwrap();
        assert!(store.due_outbound(i64::MAX).unwrap().is_empty());
        assert!(matches!(
            store.remove_outbound(&write),
            Err(Error::OutboxConflict)
        ));
    }
}
