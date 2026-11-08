//! A user-requested fresh answer replaces only the observed generation, with its exact outbox.
use super::{download::PackDownloadJob, pending, ready, stage_outbox, PendingPack};
use crate::{
    store::{Error, Store},
    PreparedEnvelope,
};
use rusqlite::{params, TransactionBehavior};

impl Store {
    /// Discard an old answer only while durably staging a fresh request for the same clicked hash.
    /// Already queued mailbox writes and the sender's daily answer limit remain in effect.
    ///
    /// # Errors
    ///
    /// Returns an error if writes are empty, the chat is no longer eligible, the
    /// request generation is stale or inconsistent, token randomness fails,
    /// outbox state conflicts, or database access or transaction commit fails.
    pub fn stage_fresh_sticker_pack_request(
        &mut self,
        sealed: &[u8],
        writes: &[PreparedEnvelope],
        group: &[u8],
        job: &PackDownloadJob,
    ) -> Result<PendingPack, Error> {
        if writes.is_empty() {
            return Err(Error::OutboxConflict);
        }
        let owner = self.owner_peer_id.clone();
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        ready(&tx, &owner, group, &job.peer)?;
        let current = pending(&tx, &job.peer, &job.pack_id)?.ok_or(Error::InboxConflict)?;
        if current.token != job.token || current.sticker_hash != job.sticker_hash {
            return Err(Error::InboxConflict);
        }
        let mut token = [0; 16];
        getrandom::fill(&mut token).map_err(|_| Error::Random)?;
        tx.execute(
            "UPDATE `sticker_pack_requests` SET `answer` = NULL, `attempts` = 0,
             `failure` = NULL, `retry_at_ms` = 0, `request_token` = ?3
             WHERE `peer_id` = ?1 AND `pack_id` = ?2",
            params![&job.peer, job.pack_id.as_slice(), token.as_slice()],
        )?;
        stage_outbox(&tx, sealed, writes, group)?;
        tx.commit()?;
        Ok(PendingPack {
            token,
            sticker_hash: job.sticker_hash,
        })
    }
}
