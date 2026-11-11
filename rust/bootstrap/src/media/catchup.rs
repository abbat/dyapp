//! Reverse proofs are staged without changing live references or accepted history.
#[path = "catchup/apply.rs"]
mod apply;
#[path = "catchup/storage.rs"]
mod storage;
pub use apply::KeepHistoryProgress;
pub(super) use storage::{discard, expire};

use super::{history, lock, owner_usage, storage_error, MediaStore};
use crate::{config::Limits, error::BootstrapError, Result};
use dyapp_identity::{key_hash, sha256, SignedRecord};
use dyapp_p2p_net::proto;
use prost::Message;
use rusqlite::{Connection, TransactionBehavior};
use std::ops::RangeInclusive;

pub(super) const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS `keep_pending` (
    `owner` TEXT PRIMARY KEY, `target` BLOB NOT NULL, `version` INTEGER NOT NULL,
    `time` INTEGER NOT NULL, `frontier` BLOB NOT NULL,
    `bytes` INTEGER NOT NULL CHECK (`bytes` >= 0)
);
CREATE INDEX IF NOT EXISTS `keep_pending_time` ON `keep_pending` (`time`);
CREATE TABLE IF NOT EXISTS `keep_pending_records` (
    `owner` TEXT NOT NULL, `version` INTEGER NOT NULL, `record` BLOB NOT NULL,
    PRIMARY KEY (`owner`, `version`)
) WITHOUT ROWID;";

pub(super) fn evict(db: &mut Connection, target: u64) -> Result<usize> {
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage_error)?;
    let removed = storage::trim(&tx, target)?;
    tx.commit().map_err(storage_error)?;
    Ok(removed)
}

fn invalid(message: &str) -> BootstrapError {
    BootstrapError::InvalidRequest(message.into())
}

fn owner_of(target: &SignedRecord) -> Result<String> {
    let key: [u8; 32] = target
        .public_key
        .as_slice()
        .try_into()
        .map_err(|_| invalid("invalid history owner key"))?;
    Ok(super::hex(&key_hash(&key)))
}

fn inspect(owner: &str, record: &SignedRecord) -> Result<proto::MediaKeepIntent> {
    if record.encoded_len() > crate::replication::MEDIA_OBJECT_BYTES {
        return Err(invalid("history record too large"));
    }
    let intent = proto::MediaKeepIntent::decode(record.payload.as_slice())
        .map_err(|_| invalid("malformed history proof"))?;
    history::verified(owner, intent.version, record)
        .map_err(|_| invalid("invalid signed history proof"))
}

fn validate(
    owner: &str,
    target: &SignedRecord,
    ancestors: &[SignedRecord],
    fresh: &RangeInclusive<u64>,
) -> Result<(proto::MediaKeepIntent, Vec<proto::MediaKeepIntent>)> {
    if i64::try_from(ancestors.len()).map_err(storage_error)? > history::MAX_PAGE_RECORDS {
        return Err(invalid("too many history ancestors"));
    }
    let bytes = ancestors
        .iter()
        .try_fold(0usize, |used, record| {
            used.checked_add(history::framed_bytes(record.encoded_len()))
        })
        .ok_or_else(|| invalid("history size overflow"))?;
    if bytes > MediaStore::keep_history_page_bytes() {
        return Err(invalid("history segment too large"));
    }
    let head = inspect(owner, target)?;
    if !fresh.contains(&head.time) {
        return Err(invalid("history target clock invalid"));
    }
    let mut parsed = Vec::new();
    for (index, record) in ancestors.iter().enumerate() {
        let intent = inspect(owner, record)?;
        if intent.version >= head.version || intent.time > *fresh.end() {
            return Err(invalid("invalid history ancestor version or clock"));
        }
        if index > 0 {
            history::follows(record, &ancestors[index - 1])
                .map_err(|_| invalid("unlinked history page"))?;
        }
        parsed.push(intent);
    }
    Ok((head, parsed))
}

fn ingest(
    db: &Connection,
    owner: &str,
    session: &mut storage::Session,
    record: &SignedRecord,
    intent: &proto::MediaKeepIntent,
) -> Result<bool> {
    if intent.version > session.frontier.version {
        return Ok(storage::known(db, owner, intent.version, record)?.unwrap_or(false));
    }
    if intent.version != session.frontier.version
        || !session.frontier.owner_present
        || session.frontier.record_hash != sha256(&record.encode_to_vec())
    {
        return Err(invalid("history proof does not match the frontier"));
    }
    if !storage::store(db, owner, record, intent)? {
        return Ok(false);
    }
    session.frontier = intent
        .expected
        .clone()
        .ok_or_else(|| invalid("missing history predecessor"))?;
    Ok(true)
}

impl MediaStore {
    /// Stage reverse-linked proofs without changing live ownership or retention.
    ///
    /// `fresh` bounds the target signed time; ancestors may be older, but cannot exceed its upper
    /// bound. Exact replays consume no additional quota. Returns the next ancestor revision,
    /// or `None` for a conflicting target/proof. Provisional bytes count toward media budgets.
    ///
    /// # Errors
    /// Returns invalid proof/clock, storage or `MediaHistoryFull` errors.
    /// Failure preserves the previous session and live ownership.
    pub fn stage_keep_history(
        &self,
        target: &SignedRecord,
        ancestors: &[SignedRecord],
        fresh: RangeInclusive<u64>,
        limits: &Limits,
    ) -> Result<Option<u64>> {
        let owner = owner_of(target)?;
        let (head, parsed) = validate(&owner, target, ancestors, &fresh)?;
        let mut db = lock(&self.db)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let Some(mut session) = storage::prepare(&tx, &owner, target, &head)? else {
            return Ok(None);
        };
        if !storage::store(&tx, &owner, target, &head)? {
            return Ok(None);
        }
        for (record, intent) in ancestors.iter().zip(&parsed) {
            if !ingest(&tx, &owner, &mut session, record, intent)? {
                return Ok(None);
            }
        }
        storage::save(&tx, &owner, &session)?;
        if owner_usage(&tx, &owner)? > limits.media_per_owner_mb.saturating_mul(1 << 20)
            || Self::used(&tx)? > limits.media_max_mb.saturating_mul(1 << 20)
        {
            return Err(BootstrapError::MediaHistoryFull);
        }
        tx.commit().map_err(storage_error)?;
        Ok(Some(session.frontier.version))
    }
}
