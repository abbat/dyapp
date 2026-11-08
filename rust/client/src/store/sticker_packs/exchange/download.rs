//! Download jobs expose request proof and local failure state, never peer descriptor secrets.
use super::response::time;
use crate::store::{Error, Store};
use rusqlite::{params, OptionalExtension};

/// Local retry reason, suitable for a translated UI status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackDownloadFailure {
    /// Node fetch or attachment verification failed.
    Fetch = 1,
    /// Pack validation or clicked-sticker binding failed.
    Invalid = 2,
    /// Saved collection quota has no room.
    Quota = 3,
    /// Local storage failed.
    Storage = 4,
}
fn failure(value: Option<i64>) -> Result<Option<PackDownloadFailure>, Error> {
    Ok(match value {
        None => None,
        Some(1) => Some(PackDownloadFailure::Fetch),
        Some(2) => Some(PackDownloadFailure::Invalid),
        Some(3) => Some(PackDownloadFailure::Quota),
        Some(4) => Some(PackDownloadFailure::Storage),
        _ => return Err(Error::InboxConflict),
    })
}

/// Metadata for one admitted current request; a separate guarded read opens its descriptor.
#[derive(Clone, PartialEq, Eq)]
pub struct PackDownloadJob {
    /// Authenticated peer the user requested the pack from.
    pub peer: String,
    /// Requested immutable manifest digest.
    pub pack_id: [u8; 32],
    /// The validated pack must contain this clicked sticker.
    pub sticker_hash: [u8; 32],
    /// Current local request generation.
    pub token: [u8; 16],
}
/// Identity-local state without media keys, capabilities or raw peer error text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackDownloadState {
    /// Whether one canonical answer is available.
    pub has_answer: bool,
    /// Last bounded local retry reason.
    pub failure: Option<PackDownloadFailure>,
    /// Saturating retry counter.
    pub attempts: u8,
    /// Unix-millisecond retry deadline.
    pub retry_at_ms: u64,
}

impl Store {
    /// Read safe local status; this alone never authorizes a network fetch.
    ///
    /// # Errors
    ///
    /// Returns an error if querying or decoding fails, retry state is inconsistent,
    /// a failure code or counter is invalid, or the retry timestamp is negative.
    pub fn sticker_pack_download_state(
        &self,
        peer: &str,
        pack: &[u8; 32],
    ) -> Result<Option<PackDownloadState>, Error> {
        let row: Option<(bool, Option<i64>, i64, i64)> = self.db.query_row(
            "SELECT `answer` IS NOT NULL, `failure`, `attempts`, `retry_at_ms` FROM `sticker_pack_requests`
             WHERE `peer_id` = ?1 AND `pack_id` = ?2",
            params![peer, pack.as_slice()], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))).optional()?;
        row.map(|(has_answer, code, attempts, retry)| {
            let failure = failure(code)?;
            if !has_answer && (failure.is_some() || attempts != 0 || retry != 0) {
                return Err(Error::InboxConflict);
            }
            Ok(PackDownloadState {
                has_answer,
                failure,
                attempts: u8::try_from(attempts).map_err(|_| Error::InboxConflict)?,
                retry_at_ms: u64::try_from(retry).map_err(|_| Error::ClockRange)?,
            })
        })
        .transpose()
    }

    /// Read at most 32 admitted due requests; no descriptor or transport work leaves the Store.
    ///
    /// # Errors
    ///
    /// Returns an error if `now_ms` exceeds the supported clock range, querying or
    /// decoding fails, or a stored peer, digest, or request token is invalid.
    pub fn due_sticker_pack_downloads(&self, now_ms: u64) -> Result<Vec<PackDownloadJob>, Error> {
        let now = time(now_ms)?;
        let mut query = self.db.prepare(
            "SELECT `peer_id`, `pack_id`, `sticker_hash`, `request_token` FROM `sticker_pack_requests`
             WHERE `answer` IS NOT NULL AND `retry_at_ms` <= ?1
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
            let (peer, pack, hash, token) = row?;
            if peer.is_empty() || peer == self.owner_peer_id {
                return Err(Error::InboxConflict);
            }
            Ok(PackDownloadJob {
                peer,
                pack_id: pack.try_into().map_err(|_| Error::InboxConflict)?,
                sticker_hash: hash.try_into().map_err(|_| Error::InboxConflict)?,
                token: token.try_into().map_err(|_| Error::InboxConflict)?,
            })
        })
        .collect()
    }

    /// Defer only the observed admitted generation, using the common 1 s..1 h retry schedule.
    ///
    /// # Errors
    ///
    /// Returns an error if `now_ms` exceeds the supported clock range or updating
    /// the matching download generation fails.
    pub fn defer_sticker_pack_download(
        &self,
        job: &PackDownloadJob,
        failure: PackDownloadFailure,
        now_ms: u64,
    ) -> Result<bool, Error> {
        let now = time(now_ms)?;
        Ok(self.db.execute(
            "UPDATE `sticker_pack_requests` SET `attempts` = MIN(`attempts` + 1, 13),
             `retry_at_ms` = ?5 + MIN(1000 << MIN(`attempts`, 12), 3600000), `failure` = ?6
             WHERE `peer_id` = ?1 AND `pack_id` = ?2 AND `sticker_hash` = ?3 AND `request_token` = ?4 AND `answer` IS NOT NULL",
            params![&job.peer, job.pack_id.as_slice(), job.sticker_hash.as_slice(), job.token.as_slice(), now, failure as i64])? == 1)
    }

    /// Explicit download retry keeps the useful answer, rotating generation against old callbacks.
    ///
    /// # Errors
    ///
    /// Returns an error if generating a new request token or updating the matching
    /// download generation fails.
    pub fn retry_sticker_pack_download(
        &self,
        job: &PackDownloadJob,
    ) -> Result<Option<PackDownloadJob>, Error> {
        let mut token = [0; 16];
        getrandom::fill(&mut token).map_err(|_| Error::Random)?;
        let changed = self.db.execute(
            "UPDATE `sticker_pack_requests` SET `attempts` = 0, `retry_at_ms` = 0,
             `failure` = NULL, `request_token` = ?5
             WHERE `peer_id` = ?1 AND `pack_id` = ?2 AND `sticker_hash` = ?3 AND `request_token` = ?4 AND `answer` IS NOT NULL",
            params![&job.peer, job.pack_id.as_slice(), job.sticker_hash.as_slice(), job.token.as_slice(), token.as_slice()])?;
        Ok((changed == 1).then(|| PackDownloadJob {
            token,
            ..job.clone()
        }))
    }
}

mod save;
#[cfg(test)]
mod tests;
