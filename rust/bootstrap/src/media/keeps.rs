//! Atomic keep ownership and conditional snapshots; callers verify signatures and clock bounds.
//! Ownership writers hold the database mutex through commit and removal of orphan files.
use super::{list, lock, orphans, storage_error, MediaStore};
use crate::{config::Limits, Result};
use dyapp_identity::{sha256, SignedRecord};
use dyapp_p2p_net::proto;
use prost::Message;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

/// A keep revision and its exact signed record, read together.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KeepSnapshot {
    /// Whether an owner row exists, including an attachment-only or unsigned version-zero row.
    pub owner_present: bool,
    /// Persisted media revision; zero when no keep has been written.
    pub version: u64,
    /// Exact signed keep, absent for attachment-only or legacy unsigned ownership.
    pub record: Option<SignedRecord>,
}

impl KeepSnapshot {
    pub(crate) fn matches(&self, expected: &proto::MediaKeepCondition) -> bool {
        let digest = self
            .record
            .as_ref()
            .map_or_else(Vec::new, |record| sha256(&record.encode_to_vec()).to_vec());
        expected.owner_present == self.owner_present
            && expected.version == self.version
            && expected.record_hash == digest
    }
}

/// The result of a conditional ownership update.
#[derive(Debug, PartialEq)]
pub enum KeepUpdate {
    /// The next revision was committed; these hashes need upload.
    Applied(Vec<Vec<u8>>),
    /// Ownership changed or the expectation was stale; no references or files were removed.
    Conflict(KeepSnapshot),
}

struct Stored {
    snapshot: KeepSnapshot,
    seen: i64,
}

impl Stored {
    fn read(db: &Connection, owner: &str) -> Result<Option<Self>> {
        let row: Option<(i64, i64, Option<Vec<u8>>)> = db
            .query_row(
                "SELECT `version`, `last_seen`, `record` FROM `owners` WHERE `owner` = ?",
                [owner],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(storage_error)?;
        row.map(|(version, seen, record)| {
            Ok(Self {
                snapshot: KeepSnapshot {
                    owner_present: true,
                    version: u64::try_from(version).map_err(storage_error)?,
                    record: record
                        .map(|bytes| SignedRecord::decode(bytes.as_slice()).map_err(storage_error))
                        .transpose()?,
                },
                seen,
            })
        })
        .transpose()
    }

    fn refuses_record(&self, version: u64, record: Option<&SignedRecord>) -> Result<bool> {
        let conditional = conditional_record(record);
        if conditional_record(self.snapshot.record.as_ref())
            && (!conditional
                || (self.snapshot.version == version && self.snapshot.record.as_ref() != record))
        {
            return Ok(true);
        }
        if conditional && version > self.snapshot.version {
            let keep = proto::MediaKeepIntent::decode(
                record
                    .ok_or_else(|| storage_error("missing conditional proof"))?
                    .payload
                    .as_slice(),
            )
            .map_err(storage_error)?;
            // A forwarded request is not evidence that its conditional write committed.
            // Existing ownership may only change through its exact signed expectation.
            return Ok(keep
                .expected
                .as_ref()
                .is_none_or(|expected| !self.snapshot.matches(expected)));
        }
        Ok(false)
    }

    fn refuses(&self, version: u64, time: i64, old: &[Vec<u8>], hashes: &[Vec<u8>]) -> bool {
        self.snapshot.version > version
            || (self.snapshot.version == version
                && (time <= self.seen
                    || old.iter().any(|hash| !hashes.contains(hash))
                    || hashes.iter().any(|hash| !old.contains(hash))))
    }
}

pub(super) fn snapshot(db: &Connection, owner: &str) -> Result<KeepSnapshot> {
    Ok(Stored::read(db, owner)?.map_or_else(KeepSnapshot::default, |value| value.snapshot))
}

fn conditional_record(record: Option<&SignedRecord>) -> bool {
    record.is_some_and(|record| {
        record
            .verify(dyapp_identity::Domain::MediaConditionalKeep)
            .is_ok()
    })
}

fn check_condition(record: &SignedRecord, current: &KeepSnapshot) -> Result<()> {
    if conditional_record(Some(record)) {
        let intent =
            proto::MediaKeepIntent::decode(record.payload.as_slice()).map_err(storage_error)?;
        if intent
            .expected
            .as_ref()
            .is_none_or(|expected| !current.matches(expected))
        {
            return Err(storage_error(
                "signed media condition differs from the expected snapshot",
            ));
        }
    }
    Ok(())
}

pub(super) fn owned(db: &Connection, owner: &str) -> Result<Vec<Vec<u8>>> {
    db.prepare("SELECT `hash` FROM `blobs` WHERE `owner` = ?")
        .and_then(|mut query| query.query_map([owner], |row| row.get(0))?.collect())
        .map_err(storage_error)
}

pub(super) struct Projected {
    pub(super) missing: Vec<Vec<u8>>,
    pub(super) dropped: Vec<Vec<u8>>,
}

pub(super) fn project(
    db: &Connection,
    owner: &str,
    (version, time): (i64, i64),
    hashes: &[Vec<u8>],
    record: Option<&SignedRecord>,
    old: Vec<Vec<u8>>,
) -> Result<Projected> {
    db.execute(
        "INSERT INTO `owners` (`owner`, `version`, `last_seen`, `record`) VALUES (?, ?, ?, ?) \
         ON CONFLICT(`owner`) DO UPDATE SET `version` = `excluded`.`version`, \
         `last_seen` = MAX(`owners`.`last_seen`, `excluded`.`last_seen`), `record` = `excluded`.`record`",
        params![owner, version, time, record.map(Message::encode_to_vec)],
    ).map_err(storage_error)?;
    let mut dropped = Vec::new();
    for hash in old.into_iter().filter(|hash| !hashes.contains(hash)) {
        db.execute(
            "DELETE FROM `blobs` WHERE `owner` = ? AND `hash` = ?",
            params![owner, hash],
        )
        .map_err(storage_error)?;
        dropped.push(hash);
    }
    let missing = list(db, owner, hashes)?;
    Ok(Projected {
        missing,
        dropped: orphans(db, dropped)?,
    })
}

impl MediaStore {
    /// Read the media revision and exact signed keep from one database statement.
    ///
    /// # Errors
    /// Returns lock, database, negative-revision or signed-record decoding errors.
    pub fn keep_snapshot(&self, owner: &str) -> Result<KeepSnapshot> {
        let db = lock(&self.db)?;
        snapshot(&db, owner)
    }

    /// Replace ownership only when the complete expected keep still matches.
    ///
    /// `record` must encode `keep`; the caller verifies its owner signature and clock bounds.
    /// The requested revision must be exactly the next revision and fit signed SQLite storage.
    /// Writer admission precedes the read; a conflict returns metadata from that same transaction.
    /// This primitive alone neither publishes a profile nor renews an abandoned staged intent.
    ///
    /// # Errors
    /// Returns invalid payload/revision, lock, transaction, decoding or orphan-file removal errors.
    pub fn replace_keep(
        &self,
        owner: &str,
        expected: &KeepSnapshot,
        keep: &proto::MediaKeep,
        record: &SignedRecord,
    ) -> Result<KeepUpdate> {
        self.replace_keep_with_limits(owner, expected, keep, record, &Limits::default())
    }

    /// Conditional replacement with the configured owner and node media budgets.
    ///
    /// # Errors
    /// Returns the errors of `replace_keep`, including `MediaHistoryFull`; failure rolls back
    /// both ownership and signed history before any orphan file is removed.
    pub fn replace_keep_with_limits(
        &self,
        owner: &str,
        expected: &KeepSnapshot,
        keep: &proto::MediaKeep,
        record: &SignedRecord,
        limits: &Limits,
    ) -> Result<KeepUpdate> {
        if proto::MediaKeep::decode(record.payload.as_slice()).map_err(storage_error)? != *keep {
            return Err(storage_error(
                "signed media keep does not match its projection",
            ));
        }
        let (version, time) = (
            i64::try_from(keep.version).map_err(storage_error)?,
            i64::try_from(keep.time).map_err(storage_error)?,
        );
        let mut db = lock(&self.db)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let current = snapshot(&tx, owner)?;
        if current != *expected {
            return Ok(KeepUpdate::Conflict(current));
        }
        if conditional_record(current.record.as_ref()) && !conditional_record(Some(record)) {
            return Err(storage_error(
                "conditional media ownership cannot be downgraded",
            ));
        }
        if current.version.checked_add(1) != Some(keep.version) {
            return Err(storage_error(
                "media keep requires the next storage revision",
            ));
        }
        check_condition(record, &current)?;
        let old = owned(&tx, owner)?;
        let Projected { missing, dropped } =
            project(&tx, owner, (version, time), &keep.hashes, Some(record), old)?;
        Self::record_history(&tx, owner, keep, Some(record), limits)?;
        tx.commit().map_err(storage_error)?;
        self.remove(dropped)?;
        Ok(KeepUpdate::Applied(missing))
    }

    pub(super) fn keep_record(
        &self,
        owner: &str,
        keep: &proto::MediaKeep,
        record: Option<&SignedRecord>,
        limits: &Limits,
    ) -> Result<Option<Vec<Vec<u8>>>> {
        let version = keep.version;
        let hashes = &keep.hashes;
        let sql_version = i64::try_from(version).map_err(storage_error)?;
        let time = i64::try_from(keep.time).map_err(storage_error)?;
        let mut db = lock(&self.db)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let stored = Stored::read(&tx, owner)?;
        if let Some(value) = &stored {
            if value.refuses_record(version, record)? {
                return Ok(None);
            }
        }
        let old = owned(&tx, owner)?;
        if stored.is_some_and(|stored| stored.refuses(version, time, &old, hashes)) {
            return Ok(None);
        }
        let Projected { missing, dropped } =
            project(&tx, owner, (sql_version, time), hashes, record, old)?;
        Self::record_history(&tx, owner, keep, record, limits)?;
        tx.commit().map_err(storage_error)?;
        self.remove(dropped)?;
        Ok(Some(missing))
    }
}

#[cfg(test)]
mod tests;
