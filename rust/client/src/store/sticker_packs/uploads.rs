//! Reserve before network I/O; an uncertain upload resumes the same immutable ciphertext.
use crate::attachment::{self, Attachment};
use crate::chat::{AttachmentMessage, Body};
use crate::store::{Error, Store};
use rusqlite::{params, OptionalExtension, TransactionBehavior};

/// Independent application cap in node accounting units (encrypted bytes times five).
pub const CHARGED_BUDGET: i64 = 16 * 1024 * 1024;
const RETENTION: i64 = 7 * 24 * 3600;
const PENDING: i64 = 8 * 24 * 3600;
const REFRESH: i64 = 24 * 3600;
type StoredUpload = (Vec<u8>, i64, i64, bool, Vec<u8>);

/// Owner-only metadata; never return the release secret in a peer message or UI record.
pub enum UploadPlan {
    /// Reuse a confirmed blob with at least one day of retention left.
    Reuse(Attachment),
    /// Upload/refresh this descriptor; retries reconstruct its exact ciphertext.
    Upload(PendingUpload),
    /// The active charged reservations would exceed the application cap.
    Fallback,
}

/// One durable reservation generation, shared by retries of the same pending upload.
#[derive(Clone, PartialEq, Eq)]
pub struct PendingUpload {
    /// Owner-only descriptor, including its private release capability.
    pub attachment: Attachment,
    /// Prevents an old completion from confirming a later refresh of the same ciphertext.
    pub reservation: [u8; 16],
}

fn pending(attachment: Attachment) -> Result<PendingUpload, Error> {
    let mut reservation = [0; 16];
    getrandom::fill(&mut reservation).map_err(|_| Error::Random)?;
    Ok(PendingUpload {
        attachment,
        reservation,
    })
}

fn encode(attachment: &Attachment) -> Result<(Vec<u8>, i64), Error> {
    attachment.check().map_err(|_| Error::InboxConflict)?;
    if !(1..=262144).contains(&attachment.size)
        || attachment.mime != "image/webp"
        || !attachment.preview.is_empty()
        || attachment.release_secret == [0; 32]
        || !(1..=512).contains(&attachment.width)
        || !(1..=512).contains(&attachment.height)
    {
        return Err(Error::InboxConflict);
    }
    let size = usize::try_from(attachment.size).map_err(|_| Error::InboxConflict)?;
    let charge = attachment::sealed_size(size)
        .and_then(|size| size.checked_mul(5))
        .ok_or(Error::InboxConflict)?;
    let charge = i64::try_from(charge).map_err(|_| Error::InboxConflict)?;
    let bytes = Body::Attachment(AttachmentMessage {
        id: [0; 16],
        hlc: 0,
        attachment: attachment.clone(),
    })
    .encode();
    if bytes.len() > 2048 {
        return Err(Error::InboxConflict);
    }
    Ok((bytes, charge))
}

fn decode(bytes: &[u8], hash: &[u8; 32], charge: i64) -> Result<Attachment, Error> {
    if bytes.len() > 2048 {
        return Err(Error::InboxConflict);
    }
    let Body::Attachment(message) = Body::decode(bytes).map_err(|_| Error::InboxConflict)? else {
        return Err(Error::InboxConflict);
    };
    let (canonical, expected) = encode(&message.attachment)?;
    if message.id != [0; 16]
        || message.hlc != 0
        || canonical != bytes
        || expected != charge
        || message.attachment.plaintext_sha256 != *hash
    {
        return Err(Error::InboxConflict);
    }
    Ok(message.attachment)
}

fn time(now: u64) -> Result<i64, Error> {
    let now = i64::try_from(now).map_err(|_| Error::ClockRange)?;
    now.checked_add(PENDING).ok_or(Error::ClockRange)?;
    Ok(now)
}

impl Store {
    /// Atomically reuse/reserve one plaintext hash; no network work runs under the DB lock.
    /// Pending work retains charge for eight days and resumes its exact descriptor after a crash.
    pub fn plan_sticker_upload(
        &mut self,
        candidate: &Attachment,
        now: u64,
    ) -> Result<UploadPlan, Error> {
        let (bytes, charge) = encode(candidate)?;
        let now = time(now)?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let latest: i64 = tx.query_row(
            "SELECT COALESCE(MAX(`created`), 0) FROM `sticker_uploads`",
            [],
            |row| row.get(0),
        )?;
        if now < latest {
            return Err(Error::ClockRange);
        }
        let stored: Option<StoredUpload> = tx.query_row(
            "SELECT `descriptor`, `charge`, `expires`, `ready`, `reservation` FROM `sticker_uploads` WHERE `hash` = ?",
            [candidate.plaintext_sha256.as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        ).optional()?;
        if let Some((bytes, cost, expires, ready, reservation)) = stored {
            let attachment = decode(&bytes, &candidate.plaintext_sha256, cost)?;
            if expires > now {
                if ready && expires - now >= REFRESH {
                    tx.commit()?;
                    return Ok(UploadPlan::Reuse(attachment));
                }
                let pending = if ready {
                    let pending = pending(attachment)?;
                    tx.execute(
                        "UPDATE `sticker_uploads` SET `created` = ?2, `expires` = ?3, `ready` = 0,
                        `reservation` = ?4 WHERE `hash` = ?1",
                        params![
                            candidate.plaintext_sha256.as_slice(),
                            now,
                            now + PENDING,
                            pending.reservation.as_slice()
                        ],
                    )?;
                    pending
                } else {
                    // A retry may upload again: keep its possible node charge for eight more days.
                    tx.execute(
                        "UPDATE `sticker_uploads` SET `created` = ?2, `expires` = ?3 WHERE `hash` = ?1",
                        params![candidate.plaintext_sha256.as_slice(), now, now + PENDING],
                    )?;
                    PendingUpload {
                        attachment,
                        reservation: reservation.try_into().map_err(|_| Error::InboxConflict)?,
                    }
                };
                tx.commit()?;
                return Ok(UploadPlan::Upload(pending));
            }
        }
        tx.execute("DELETE FROM `sticker_uploads` WHERE `expires` <= ?", [now])?;
        let used: i64 = tx.query_row(
            "SELECT COALESCE(SUM(`charge`), 0) FROM `sticker_uploads`",
            [],
            |row| row.get(0),
        )?;
        if used
            .checked_add(charge)
            .is_none_or(|total| total > CHARGED_BUDGET)
        {
            tx.commit()?;
            return Ok(UploadPlan::Fallback);
        }
        let pending = pending(candidate.clone())?;
        tx.execute("INSERT INTO `sticker_uploads` (`hash`, `descriptor`, `charge`, `created`, `expires`, `ready`, `reservation`)
            VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6)",
            params![candidate.plaintext_sha256.as_slice(), bytes, charge, now, now + PENDING, pending.reservation.as_slice()])?;
        tx.commit()?;
        Ok(UploadPlan::Upload(pending))
    }

    /// Confirm only the same still-reserved descriptor; a stale completion cannot replace another.
    pub fn finish_sticker_upload(
        &mut self,
        pending: &PendingUpload,
        now: u64,
    ) -> Result<(), Error> {
        let attachment = &pending.attachment;
        let (bytes, _) = encode(attachment)?;
        let now = time(now)?;
        let changed = self.db.execute(
            "UPDATE `sticker_uploads` SET `created` = ?3, `expires` = ?4, `ready` = 1
            WHERE `hash` = ?1 AND `descriptor` = ?2 AND `created` <= ?3 AND `expires` > ?3 AND `reservation` = ?5 AND `ready` = 0",
            params![
                attachment.plaintext_sha256.as_slice(),
                bytes,
                now,
                now + RETENTION,
                pending.reservation.as_slice()
            ],
        )?;
        if changed != 1 {
            return Err(Error::InboxConflict);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
