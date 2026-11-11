//! Durable signed ancestry is bounded by the same media budgets as the owner's blobs.
use super::{lock, owner_usage, storage_error, MediaStore};
use crate::{config::Limits, error::BootstrapError, Result};
use dyapp_identity::{sha256, Domain, SignedRecord};
use dyapp_p2p_net::proto;
use prost::Message;
use rusqlite::{params, Connection};

const MAX_PAGE_RECORDS: i64 = 256;
const MAX_RECORD_BYTES: usize = crate::replication::MEDIA_OBJECT_BYTES;

fn verified(owner: &str, version: u64, record: &SignedRecord) -> Result<proto::MediaKeepIntent> {
    if record.encoded_len() > MAX_RECORD_BYTES
        || record
            .verify(Domain::MediaConditionalKeep)
            .map_err(storage_error)?
            != owner
    {
        return Err(storage_error(
            "invalid conditional history owner or record size",
        ));
    }
    let intent =
        proto::MediaKeepIntent::decode(record.payload.as_slice()).map_err(storage_error)?;
    let expected = intent
        .expected
        .as_ref()
        .ok_or_else(|| storage_error("query is not ownership history"))?;
    if intent.version != version
        || version == 0
        || expected.version.checked_add(1) != Some(version)
        || !matches!(expected.record_hash.len(), 0 | 32)
        || (!expected.owner_present && (expected.version != 0 || !expected.record_hash.is_empty()))
        || (!expected.record_hash.is_empty() && expected.version == 0)
        || intent.intent_hash.len() != 32
        || intent.requester.parse::<libp2p::PeerId>().is_err()
        || intent.hashes.len() > 256
        || intent.hashes.iter().any(|hash| hash.len() != 32)
    {
        return Err(storage_error("invalid conditional ownership history"));
    }
    Ok(intent)
}

fn framed_bytes(bytes: usize) -> usize {
    // A repeated message field: tag 1, its actual length delimiter, then the record.
    1 + prost::encoding::encoded_len_varint(bytes as u64) + bytes
}

fn decode(owner: &str, version: i64, bytes: &[u8]) -> Result<SignedRecord> {
    let record = SignedRecord::decode(bytes).map_err(storage_error)?;
    verified(
        owner,
        u64::try_from(version).map_err(storage_error)?,
        &record,
    )?;
    Ok(record)
}

fn follows(previous: &SignedRecord, next: &SignedRecord) -> Result<()> {
    let old = proto::MediaKeepIntent::decode(previous.payload.as_slice()).map_err(storage_error)?;
    let intent = proto::MediaKeepIntent::decode(next.payload.as_slice()).map_err(storage_error)?;
    let expected = intent
        .expected
        .ok_or_else(|| storage_error("missing history predecessor"))?;
    if old.version.checked_add(1) != Some(intent.version)
        || !expected.owner_present
        || expected.version != old.version
        || expected.record_hash != sha256(&previous.encode_to_vec())
    {
        return Err(storage_error("conditional history is not contiguous"));
    }
    Ok(())
}

fn read_page(
    owner: &str,
    rows: &mut rusqlite::Rows<'_>,
    max_bytes: usize,
    link: fn(&SignedRecord, &SignedRecord) -> Result<()>,
) -> Result<Vec<SignedRecord>> {
    let mut records = Vec::new();
    let mut used = 0;
    while let Some(row) = rows.next().map_err(storage_error)? {
        let record = decode(
            owner,
            row.get(0).map_err(storage_error)?,
            row.get_ref(1)
                .map_err(storage_error)?
                .as_blob()
                .map_err(storage_error)?,
        )?;
        let size = framed_bytes(record.encoded_len());
        if used + size > max_bytes {
            if records.is_empty() {
                return Err(storage_error("history page cannot fit a record"));
            }
            break;
        }
        if let Some(previous) = records.last() {
            link(previous, &record)?;
        }
        used += size;
        records.push(record);
    }
    Ok(records)
}

impl MediaStore {
    /// Largest history page, including framing for a maximum-sized signed record.
    #[must_use]
    pub fn keep_history_page_bytes() -> usize {
        framed_bytes(MAX_RECORD_BYTES)
    }

    /// Persist a verified keep using the configured owner and node media budgets.
    ///
    /// # Errors
    /// Returns storage errors or `MediaHistoryFull`; neither references nor files change on failure.
    pub fn keep_signed_with_limits(
        &self,
        owner: &str,
        keep: &proto::MediaKeep,
        record: &SignedRecord,
        limits: &Limits,
    ) -> Result<Option<Vec<Vec<u8>>>> {
        self.keep_record(owner, keep, Some(record), limits)
    }

    pub(super) fn record_history(
        db: &Connection,
        owner: &str,
        keep: &proto::MediaKeep,
        record: Option<&SignedRecord>,
        limits: &Limits,
    ) -> Result<()> {
        let Some(record) = record else {
            return Ok(());
        };
        if record.verify(Domain::MediaConditionalKeep).is_err() {
            return Ok(());
        }
        verified(owner, keep.version, record)?;
        if proto::MediaKeep::decode(record.payload.as_slice()).map_err(storage_error)? != *keep {
            return Err(storage_error(
                "history record differs from ownership projection",
            ));
        }
        let bytes = record.encode_to_vec();
        db.execute(
            "INSERT INTO `keep_history` (`owner`, `version`, `record`) VALUES (?, ?, ?)",
            params![
                owner,
                i64::try_from(keep.version).map_err(storage_error)?,
                bytes
            ],
        )
        .map_err(storage_error)?;
        db.execute(
            "UPDATE `owners` SET `history_bytes` = `history_bytes` + ? WHERE `owner` = ?",
            params![i64::try_from(bytes.len()).map_err(storage_error)?, owner],
        )
        .map_err(storage_error)?;
        if owner_usage(db, owner)? > limits.media_per_owner_mb.saturating_mul(1 << 20)
            || Self::used(db)? > limits.media_max_mb.saturating_mul(1 << 20)
        {
            return Err(BootstrapError::MediaHistoryFull);
        }
        Ok(())
    }

    /// Read signed accepted ancestry after `after`, through a frozen target revision.
    ///
    /// At most 256 records and `keep_history_page_bytes()` repeated-message wire bytes
    /// are returned per page. The limit includes framing for a 1 MiB signed record.
    /// The first available record can be a seeded head with no older local ancestry.
    /// History is retained until the owner is actually expired or evicted, rather than pruning
    /// by record time: attachments may keep an older ownership head live.
    ///
    /// # Errors
    /// Returns invalid range/budget, lock/database or malformed, forged and non-contiguous history errors.
    /// A budget too small for the first available record fails explicitly.
    pub fn keep_history(
        &self,
        owner: &str,
        after: u64,
        through: u64,
        max_bytes: usize,
    ) -> Result<Vec<SignedRecord>> {
        if after > through || max_bytes == 0 || max_bytes > Self::keep_history_page_bytes() {
            return Err(storage_error("invalid ownership history range or budget"));
        }
        let db = lock(&self.db)?;
        let mut statement = db.prepare(
            "SELECT `version`, `record` FROM `keep_history` WHERE `owner` = ? AND `version` > ? AND `version` <= ? ORDER BY `version` LIMIT ?",
        ).map_err(storage_error)?;
        let mut rows = statement
            .query(params![
                owner,
                i64::try_from(after).map_err(storage_error)?,
                i64::try_from(through).map_err(storage_error)?,
                MAX_PAGE_RECORDS,
            ])
            .map_err(storage_error)?;
        read_page(owner, &mut rows, max_bytes, follows)
    }

    /// Read a bounded ancestry suffix in reverse order, starting at a frozen revision.
    ///
    /// Continue with the last returned record's expected predecessor version. Reading a page
    /// does not change ownership, history or retention.
    ///
    /// # Errors
    /// Returns the budget, database and proof-validation errors of `keep_history`.
    pub fn keep_history_reverse(
        &self,
        owner: &str,
        through: u64,
        max_bytes: usize,
    ) -> Result<Vec<SignedRecord>> {
        if max_bytes == 0 || max_bytes > Self::keep_history_page_bytes() {
            return Err(storage_error("invalid ownership history budget"));
        }
        let db = lock(&self.db)?;
        let mut statement = db.prepare(
            "SELECT `version`, `record` FROM `keep_history` WHERE `owner` = ? AND `version` <= ? ORDER BY `version` DESC LIMIT ?",
        ).map_err(storage_error)?;
        let mut rows = statement
            .query(params![
                owner,
                i64::try_from(through).map_err(storage_error)?,
                MAX_PAGE_RECORDS,
            ])
            .map_err(storage_error)?;
        read_page(owner, &mut rows, max_bytes, |newer, older| {
            follows(older, newer)
        })
    }
}
