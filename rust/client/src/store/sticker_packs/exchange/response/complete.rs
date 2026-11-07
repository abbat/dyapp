//! Advance cooldown only with the current job's exact durable response outbox.
use super::{time, PackResponseJob, COOLDOWN_MS};
use crate::chat::pack_control::PackAnswer;
use crate::store::{sticker_packs, Error, Store};
use crate::PreparedEnvelope;
use rusqlite::{params, OptionalExtension, TransactionBehavior};

impl Store {
    /// Commit one current response generation and its rolling daily cooldown with exact writes.
    /// Recheck saved-file presence under the pack writer lock: Remove may run during upload.
    pub fn stage_sticker_pack_response(
        &mut self,
        sealed: &[u8],
        writes: &[PreparedEnvelope],
        job: &PackResponseJob,
        answer: &PackAnswer,
        now_ms: u64,
    ) -> Result<(), Error> {
        answer.encode().map_err(|_| Error::InboxConflict)?;
        if answer.pack_id != job.pack_id || writes.is_empty() {
            return Err(Error::InboxConflict);
        }
        let now = time(now_ms)?;
        let owner = self.owner_peer_id.clone();
        let directory =
            sticker_packs::directory(self).map_err(|_| Error::StickerPackUnavailable)?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        super::super::ready(&tx, &owner, &job.group, &job.peer)?;
        let shared: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM `sticker_shared_packs` WHERE `peer_id` = ?1 AND `pack_id` = ?2)",
            params![&job.peer, job.pack_id.as_slice()], |row| row.get(0))?;
        if !shared {
            return Err(Error::InboxConflict);
        }
        let previous: Option<Option<i64>> = tx
            .query_row(
                "SELECT `last_answered_ms` FROM `sticker_pack_responses`
             WHERE `peer_id` = ?1 AND `pack_id` = ?2 AND `pending_group` = ?3 AND `job_token` = ?4",
                params![
                    &job.peer,
                    job.pack_id.as_slice(),
                    &job.group,
                    job.token.as_slice()
                ],
                |row| row.get(0),
            )
            .optional()?;
        let previous = previous.ok_or(Error::InboxConflict)?;
        if previous.is_some_and(|last| now.saturating_sub(last) < COOLDOWN_MS) {
            return Err(Error::ClockRange);
        }
        if !sticker_packs::valid_directory(&directory).map_err(|_| Error::StickerPackUnavailable)? {
            return Err(Error::StickerPackUnavailable);
        }
        let metadata = std::fs::symlink_metadata(directory.join(sticker_packs::hex(&job.pack_id)))
            .map_err(|_| Error::StickerPackUnavailable)?;
        if !metadata.is_file() || metadata.is_symlink() {
            return Err(Error::StickerPackUnavailable);
        }
        super::super::stage_outbox(&tx, sealed, writes, &job.group)?;
        tx.execute(
            "UPDATE `sticker_pack_responses` SET `last_answered_ms` = ?5,
             `pending_group` = NULL, `job_token` = NULL, `retry_at_ms` = 0, `attempts` = 0
             WHERE `peer_id` = ?1 AND `pack_id` = ?2 AND `pending_group` = ?3 AND `job_token` = ?4",
            params![
                &job.peer,
                job.pack_id.as_slice(),
                &job.group,
                job.token.as_slice(),
                now
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Retire a job for an unavailable saved pack without pretending a response was sent.
    pub fn retire_sticker_pack_response(&self, job: &PackResponseJob) -> Result<bool, Error> {
        Ok(self.db.execute(
            "UPDATE `sticker_pack_responses` SET `pending_group` = NULL, `job_token` = NULL,
             `retry_at_ms` = 0, `attempts` = 0
             WHERE `peer_id` = ?1 AND `pack_id` = ?2 AND `pending_group` = ?3 AND `job_token` = ?4",
            params![
                &job.peer,
                job.pack_id.as_slice(),
                &job.group,
                job.token.as_slice()
            ],
        )? == 1)
    }
}

#[cfg(test)]
mod tests;
