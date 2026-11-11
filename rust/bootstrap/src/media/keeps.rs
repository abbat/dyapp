//! Atomic keep ownership and conditional snapshots; callers verify signatures and clock bounds.
//! Ownership writers hold the database mutex through commit and removal of orphan files.
use super::{list, lock, orphans, storage_error, MediaStore};
use crate::Result;
use dyapp_identity::SignedRecord;
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

    fn refuses(&self, version: u64, time: i64, old: &[Vec<u8>], hashes: &[Vec<u8>]) -> bool {
        self.snapshot.version > version
            || (self.snapshot.version == version
                && (time <= self.seen
                    || old.iter().any(|hash| !hashes.contains(hash))
                    || hashes.iter().any(|hash| !old.contains(hash))))
    }
}

fn owned(db: &Connection, owner: &str) -> Result<Vec<Vec<u8>>> {
    db.prepare("SELECT `hash` FROM `blobs` WHERE `owner` = ?")
        .and_then(|mut query| query.query_map([owner], |row| row.get(0))?.collect())
        .map_err(storage_error)
}

struct Projected {
    missing: Vec<Vec<u8>>,
    dropped: Vec<Vec<u8>>,
}

fn project(
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
        Ok(Stored::read(&db, owner)?.map_or_else(KeepSnapshot::default, |value| value.snapshot))
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
        let current =
            Stored::read(&tx, owner)?.map_or_else(KeepSnapshot::default, |value| value.snapshot);
        if current != *expected {
            return Ok(KeepUpdate::Conflict(current));
        }
        if current.version.checked_add(1) != Some(keep.version) {
            return Err(storage_error(
                "media keep requires the next storage revision",
            ));
        }
        let old = owned(&tx, owner)?;
        let Projected { missing, dropped } =
            project(&tx, owner, (version, time), &keep.hashes, Some(record), old)?;
        tx.commit().map_err(storage_error)?;
        self.remove(dropped)?;
        Ok(KeepUpdate::Applied(missing))
    }

    pub(super) fn keep_record(
        &self,
        owner: &str,
        version: u64,
        hashes: &[Vec<u8>],
        time: u64,
        record: Option<&SignedRecord>,
    ) -> Result<Option<Vec<Vec<u8>>>> {
        let sql_version = i64::try_from(version).map_err(storage_error)?;
        let time = i64::try_from(time).map_err(storage_error)?;
        let mut db = lock(&self.db)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let stored = Stored::read(&tx, owner)?;
        let old = owned(&tx, owner)?;
        if stored.is_some_and(|stored| stored.refuses(version, time, &old, hashes)) {
            return Ok(None);
        }
        let Projected { missing, dropped } =
            project(&tx, owner, (sql_version, time), hashes, record, old)?;
        tx.commit().map_err(storage_error)?;
        self.remove(dropped)?;
        Ok(Some(missing))
    }
}

#[cfg(test)]
mod tests;
