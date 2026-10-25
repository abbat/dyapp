//! Durable mailbox inbox deduplication and ACK intent.

use super::{catalog, Error, InboxKey, InboxKind, Store};
use rusqlite::{params, OptionalExtension, Transaction};

impl Store {
    /// Check before processing MLS bytes so a repeated fetch cannot apply or display them twice.
    pub fn has_inbox(&self, key: &InboxKey) -> Result<bool, Error> {
        let found: Option<i64> = self
            .db
            .query_row(
                "SELECT 1 FROM `inbox` WHERE `mailbox` = ?1 AND `envelope_id` = ?2",
                params![key.mailbox.as_slice(), key.id.as_slice()],
                |row| row.get(0),
            )
            .optional()?;
        Ok(found.is_some())
    }

    /// Commit the new sealed MLS snapshot, processed delivery and pending ACK together, and the
    /// `(group ID, peer ID)` catalog entry of a group a Welcome joined.
    /// Returns false for an identical duplicate without replacing the saved MLS state.
    pub fn stage_inbound(
        &mut self,
        sealed_device: &[u8],
        key: &InboxKey,
        kind: InboxKind,
        payload: &[u8],
        joined: Option<(&[u8], &str)>,
    ) -> Result<bool, Error> {
        self.stage_inbound_with(sealed_device, key, kind, payload, joined, |_| Ok(()))
    }

    /// Run `project` only for a newly inserted envelope, inside the same MLS/inbox/ACK commit.
    pub(super) fn stage_inbound_with<F>(
        &mut self,
        sealed_device: &[u8],
        key: &InboxKey,
        kind: InboxKind,
        payload: &[u8],
        joined: Option<(&[u8], &str)>,
        project: F,
    ) -> Result<bool, Error>
    where
        F: FnOnce(&Transaction<'_>) -> Result<(), Error>,
    {
        if sealed_device.is_empty() {
            return Err(Error::EmptyMlsState);
        }
        if joined.is_some_and(|(id, peer_id)| id.is_empty() || peer_id == self.owner_peer_id) {
            return Err(Error::GroupConflict);
        }
        let tx = self.db.transaction()?;
        let inserted = tx.execute(
            "INSERT INTO `inbox` (`mailbox`, `envelope_id`, `kind`, `payload`, `ack_pending`)
             VALUES (?1, ?2, ?3, ?4, 1) ON CONFLICT (`mailbox`, `envelope_id`) DO NOTHING",
            params![
                key.mailbox.as_slice(),
                key.id.as_slice(),
                kind as i64,
                payload
            ],
        )?;
        if inserted == 0 {
            let saved: (i64, Vec<u8>) = tx.query_row(
                "SELECT `kind`, `payload` FROM `inbox` WHERE `mailbox` = ?1 AND `envelope_id` = ?2",
                params![key.mailbox.as_slice(), key.id.as_slice()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            if saved.0 != kind as i64 || saved.1 != payload {
                return Err(Error::InboxConflict);
            }
            return Ok(false);
        }
        if let Some((group_id, peer_id)) = joined {
            catalog(&tx, group_id, peer_id)?;
        }
        project(&tx)?;
        tx.execute(
            "INSERT INTO `mls_state` (`slot`, `sealed`, `revision`) VALUES (1, ?1, 1)
             ON CONFLICT (`slot`) DO UPDATE SET `sealed` = excluded.`sealed`,
             `revision` = `mls_state`.`revision` + 1",
            [sealed_device],
        )?;
        tx.commit()?;
        Ok(true)
    }

    /// Delivery IDs to acknowledge after the MLS snapshot and deduplication row are durable.
    pub fn pending_acks(&self) -> Result<Vec<InboxKey>, Error> {
        let mut query = self.db.prepare(
            "SELECT `mailbox`, `envelope_id` FROM `inbox` WHERE `ack_pending` = 1
             ORDER BY `mailbox`, `envelope_id`",
        )?;
        let rows = query.query_map([], |row| {
            Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?))
        })?;
        rows.map(|row| {
            let (mailbox, id) = row?;
            Ok(InboxKey {
                mailbox: mailbox.try_into().map_err(|_| Error::InboxConflict)?,
                id: id.try_into().map_err(|_| Error::InboxConflict)?,
            })
        })
        .collect()
    }

    /// Mark an ACK completed only after the node confirms it; repeated calls are harmless.
    pub fn mark_acked(&self, key: &InboxKey) -> Result<bool, Error> {
        Ok(self.db.execute(
            "UPDATE `inbox` SET `ack_pending` = 0 WHERE `mailbox` = ?1
             AND `envelope_id` = ?2 AND `ack_pending` = 1",
            params![key.mailbox.as_slice(), key.id.as_slice()],
        )? == 1)
    }
}

#[cfg(test)]
mod tests;
