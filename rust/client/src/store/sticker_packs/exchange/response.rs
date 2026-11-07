//! Authorized automatic answers wait durably without turning decoded requests into network work.
use super::inbound_allowed;
use crate::chat::pack_control::PackRequest;
use crate::store::{messages::VerifiedChatSender, Error, InboxKey, InboxKind, Store};
use rusqlite::{params, OptionalExtension};

const COOLDOWN_MS: i64 = 86_400_000;
pub(super) fn time(now_ms: u64) -> Result<i64, Error> {
    let now = i64::try_from(now_ms).map_err(|_| Error::ClockRange)?;
    now.checked_add(COOLDOWN_MS).ok_or(Error::ClockRange)?;
    Ok(now)
}

/// One authorized response generation; it contains no media key or release capability.
#[derive(Clone, PartialEq, Eq)]
pub struct PackResponseJob {
    /// Verified requester identity.
    pub peer: String,
    /// Manifest digest this owner previously shared with that peer.
    pub pack_id: [u8; 32],
    /// Authenticated group carrying the request.
    pub group: Vec<u8>,
    /// Compare before completing or deferring work.
    pub token: [u8; 16],
}

impl Store {
    /// Queue at most one response per authorized peer/pack, with a global bound of 64 pending jobs.
    /// Unauthorized, duplicate, cooling-down and over-cap requests keep only inbox digest and ACK.
    pub fn stage_sticker_pack_request_inbound(
        &mut self,
        sealed: &[u8],
        inbox: &InboxKey,
        sender: VerifiedChatSender<'_>,
        request: &PackRequest,
        now_ms: u64,
    ) -> Result<bool, Error> {
        if sender.device.len() != 32 || sender.author.is_empty() {
            return Err(Error::InboxConflict);
        }
        let now = time(now_ms)?;
        let mut tagged = vec![15];
        tagged.extend(request.encode());
        let digest = dyapp_identity::sha256(&tagged);
        let owner = self.owner_peer_id.clone();
        let mut queued = false;
        let inserted = self.stage_inbound_with(
            sealed, inbox, InboxKind::StateChange, &digest, None, |tx| {
                if !inbound_allowed(tx, &owner, sender.group, sender.author)? { return Ok(()); }
                let shared: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM `sticker_shared_packs` WHERE `peer_id` = ?1 AND `pack_id` = ?2)",
                    params![sender.author, request.pack_id.as_slice()], |row| row.get(0))?;
                if !shared { return Ok(()); }
                let previous: Option<(Option<i64>, bool)> = tx.query_row(
                    "SELECT `last_answered_ms`, `pending_group` IS NOT NULL FROM `sticker_pack_responses`
                     WHERE `peer_id` = ?1 AND `pack_id` = ?2",
                    params![sender.author, request.pack_id.as_slice()],
                    |row| Ok((row.get(0)?, row.get(1)?))).optional()?;
                if previous.is_some_and(|(last, pending)| pending || last.is_some_and(|last| now.saturating_sub(last) < COOLDOWN_MS)) {
                    return Ok(());
                }
                let count: i64 = tx.query_row(
                    "SELECT COUNT(*) FROM `sticker_pack_responses` WHERE `pending_group` IS NOT NULL", [], |row| row.get(0))?;
                if count >= 64 { return Ok(()); }
                let mut token = [0; 16];
                getrandom::fill(&mut token).map_err(|_| Error::Random)?;
                tx.execute(
                    "INSERT INTO `sticker_pack_responses` (`peer_id`, `pack_id`, `pending_group`, `job_token`)
                     VALUES (?1, ?2, ?3, ?4) ON CONFLICT (`peer_id`, `pack_id`) DO UPDATE SET
                     `pending_group` = excluded.`pending_group`, `job_token` = excluded.`job_token`,
                     `attempts` = 0, `retry_at_ms` = 0",
                    params![sender.author, request.pack_id.as_slice(), sender.group, token.as_slice()])?;
                queued = true;
                Ok(())
            })?;
        Ok(inserted && queued)
    }

    /// Read a bounded due batch. Listing never uploads bytes or mutates retry deadlines.
    pub fn due_sticker_pack_responses(&self, now_ms: u64) -> Result<Vec<PackResponseJob>, Error> {
        let now = time(now_ms)?;
        let mut query = self.db.prepare(
            "SELECT `peer_id`, `pack_id`, `pending_group`, `job_token` FROM `sticker_pack_responses`
             WHERE `pending_group` IS NOT NULL AND `retry_at_ms` <= ?1
             ORDER BY `retry_at_ms`, `peer_id`, `pack_id` LIMIT 32")?;
        let rows = query.query_map([now], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, Vec<u8>>(2)?,
                row.get::<_, Vec<u8>>(3)?,
            ))
        })?;
        rows.map(|row| {
            let (peer, pack, group, token) = row?;
            if peer.is_empty() || peer == self.owner_peer_id || group.is_empty() {
                return Err(Error::InboxConflict);
            }
            Ok(PackResponseJob {
                peer,
                group,
                pack_id: pack.try_into().map_err(|_| Error::InboxConflict)?,
                token: token.try_into().map_err(|_| Error::InboxConflict)?,
            })
        })
        .collect()
    }

    /// Reuse the outbox's 1 s exponential retry schedule, capped at 1 h, without sleeping.
    pub fn defer_sticker_pack_response(
        &self,
        job: &PackResponseJob,
        now_ms: u64,
    ) -> Result<bool, Error> {
        let now = time(now_ms)?;
        Ok(self.db.execute(
            "UPDATE `sticker_pack_responses` SET `attempts` = MIN(`attempts` + 1, 13),
             `retry_at_ms` = ?5 + MIN(1000 << MIN(`attempts`, 12), 3600000)
             WHERE `peer_id` = ?1 AND `pack_id` = ?2 AND `pending_group` = ?3 AND `job_token` = ?4",
            params![
                &job.peer,
                job.pack_id.as_slice(),
                &job.group,
                job.token.as_slice(),
                now
            ],
        )? == 1)
    }
}

mod complete;
#[cfg(test)]
mod tests;
