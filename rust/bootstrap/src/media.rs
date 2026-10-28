//! Media blobs of the media role: one file per blob at `<dir>/aa/bb/<hash>`, and in `media.db`
//! which owner lists which blob. A blob listed by several owners is stored once and counts
//! against each owner's quota; its file goes when the last owner drops it.
//!
//! A chat attachment is a slot `r:<release hash>:<owner>` in `attachments`, listing its blobs in
//! `blobs` like an owner; the owner's puts and quota cover its slots. A slot goes on release or
//! expiry.
//!
//! A sharded blob assembled for a ranged get stays whole at `<dir>/cache/<hash>` until idle for
//! `limits.media_cache_minutes`; under the disk guard it goes before any list.

use crate::replication::MediaManifestExt;
use crate::storage::{free, lock, open, storage_error};
use crate::Result;
use dyapp_identity::SignedRecord;
use dyapp_p2p_net::proto::{self, media_replica_put::Authorization};
use prost::Message;
use rusqlite::{params, Connection, OptionalExtension};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// What became of a put.
#[derive(Debug, PartialEq, Eq)]
pub enum Put {
    /// The blob, manifest or shard is stored.
    Stored,
    /// No keep of the owner lists the hash.
    NotListed,
    /// The owner's blobs would exceed the quota.
    OverQuota,
    /// The data does not match its hash, manifest or index.
    Invalid,
}

/// The outcome of an attach.
#[derive(Debug, PartialEq, Eq)]
pub enum Attached {
    /// Stored; the hashes not stored yet.
    Missing(Vec<Vec<u8>>),
    /// The owner already has the maximum of other attachments.
    Full,
    /// The release hash was released and the attachment has not expired yet.
    Released,
}

/// Media blob files and their ownership database.
pub struct MediaStore {
    dir: PathBuf,
    cache: PathBuf,
    // ponytail: one connection; puts hold it while the file is written (1 MiB at most).
    db: Mutex<Connection>,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Rows of the owner `?1`: its keep and its attachments.
const OWNED: &str =
    "(`owner` = ?1 OR `owner` IN (SELECT `slot` FROM `attachments` WHERE `owner` = ?1))";

/// Lists `hashes` for `owner`; returns those it has not stored yet.
fn list(tx: &Connection, owner: &str, hashes: &[Vec<u8>]) -> Result<Vec<Vec<u8>>> {
    let mut missing = Vec::new();
    for hash in hashes {
        tx.execute(
            "INSERT INTO `blobs` (`owner`, `hash`, `size`, `cost`) \
             VALUES (?1, ?2, (SELECT MAX(`size`) FROM `blobs` WHERE `hash` = ?2), (SELECT MAX(`cost`) FROM `blobs` WHERE `hash` = ?2)) \
             ON CONFLICT(`owner`, `hash`) DO UPDATE SET `size` = COALESCE(`blobs`.`size`, `excluded`.`size`), `cost` = COALESCE(`blobs`.`cost`, `excluded`.`cost`)",
            params![owner, hash],
        )
        .map_err(storage_error)?;
        let size: Option<i64> = tx
            .query_row(
                "SELECT `size` FROM `blobs` WHERE `owner` = ? AND `hash` = ?",
                params![owner, hash],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        if size.is_none() && !missing.contains(hash) {
            missing.push(hash.clone());
        }
    }
    Ok(missing)
}

/// The `dropped` hashes no row holds stored any more.
fn orphans(tx: &Connection, dropped: Vec<Vec<u8>>) -> Result<Vec<Vec<u8>>> {
    let mut orphans = Vec::new();
    for hash in dropped {
        let held: bool = tx
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM `blobs` WHERE `hash` = ?)",
                [&hash],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        if !held {
            tx.execute("DELETE FROM `shards` WHERE `hash` = ?", [&hash])
                .map_err(storage_error)?;
            tx.execute("DELETE FROM `manifests` WHERE `hash` = ?", [&hash])
                .map_err(storage_error)?;
            orphans.push(hash);
        }
    }
    Ok(orphans)
}

const KEEPS_AFTER: &str = "SELECT `owner`, `record` FROM `owners` \
     WHERE `owner` > ? AND `record` IS NOT NULL ORDER BY `owner` LIMIT ?";

/// Writes `data` aside and renames it to `path`, so a reader never sees a partial file.
fn write_aside(path: &Path, data: &[u8]) -> Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| storage_error("media path has no parent"))?;
    fs::create_dir_all(dir).map_err(storage_error)?;
    let temp = dir.join(format!(".{}", uuid::Uuid::new_v4()));
    let mut file = fs::File::create(&temp).map_err(storage_error)?;
    file.write_all(data)
        .and_then(|()| file.sync_all())
        .and_then(|()| fs::rename(&temp, path))
        .map_err(|e| {
            let _ = fs::remove_file(&temp);
            storage_error(e)
        })
}

/// Removes `path`; true if it was there.
fn remove_file(path: &Path) -> Result<bool> {
    match fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(storage_error(e)),
    }
}

impl MediaStore {
    /// Opens `dir/media.db` with the blob files under `dir/data` and assembled ones under
    /// `dir/cache`.
    pub fn open(dir: &Path) -> Result<Self> {
        let (data, db) = (dir.join("data"), dir.join("media.db"));
        fs::create_dir_all(&data).map_err(storage_error)?;
        let db = open(
            &db,
            // `size` is NULL while a listed blob has not been put. `last_seen`, `created` and
            // `record`: when the owner was last seen, when an attachment was stored, and the
            // signed records. `manifests` and `shards` hold the erasure-coded replicas,
            // `released` the release hashes of released attachments until the attachments'
            // expiry, `cache` the assembled blobs and their last read, in Unix seconds.
            &["CREATE TABLE IF NOT EXISTS `owners` (
                 `owner` TEXT PRIMARY KEY,
                 `version` INTEGER NOT NULL,
                 `last_seen` INTEGER NOT NULL,
                 `record` BLOB
             );
             CREATE INDEX IF NOT EXISTS `owners_seen` ON `owners` (`last_seen`);
             CREATE TABLE IF NOT EXISTS `blobs` (
                 `owner` TEXT NOT NULL,
                 `hash` BLOB NOT NULL,
                 `size` INTEGER,
                 `cost` INTEGER,
                 PRIMARY KEY (`owner`, `hash`)
             );
             CREATE INDEX IF NOT EXISTS `blobs_hash` ON `blobs` (`hash`);
             CREATE TABLE IF NOT EXISTS `attachments` (
                 `slot` TEXT PRIMARY KEY,
                 `release` BLOB NOT NULL,
                 `owner` TEXT NOT NULL,
                 `expires` INTEGER NOT NULL,
                 `created` INTEGER NOT NULL,
                 `record` BLOB
             );
             CREATE INDEX IF NOT EXISTS `attachments_owner` ON `attachments` (`owner`);
             CREATE INDEX IF NOT EXISTS `attachments_release` ON `attachments` (`release`);
             CREATE INDEX IF NOT EXISTS `attachments_created` ON `attachments` (`created`);
             CREATE TABLE IF NOT EXISTS `manifests` (`hash` BLOB PRIMARY KEY, `record` BLOB NOT NULL);
             CREATE TABLE IF NOT EXISTS `shards` (`hash` BLOB NOT NULL, `idx` INTEGER NOT NULL,
                 `data` BLOB NOT NULL, PRIMARY KEY (`hash`, `idx`));
             CREATE TABLE IF NOT EXISTS `purges` (`id` INTEGER PRIMARY KEY CHECK (`id` = 1),
                 `count` INTEGER NOT NULL);
             INSERT OR IGNORE INTO `purges` (`id`, `count`) VALUES (1, 0);
             CREATE TABLE IF NOT EXISTS `released` (`release` BLOB PRIMARY KEY,
                 `expires` INTEGER NOT NULL) WITHOUT ROWID;
             CREATE INDEX IF NOT EXISTS `released_expires` ON `released` (`expires`);
             CREATE TABLE IF NOT EXISTS `cache` (`hash` BLOB PRIMARY KEY, `size` INTEGER NOT NULL,
                 `read` INTEGER NOT NULL) WITHOUT ROWID;
             CREATE INDEX IF NOT EXISTS `cache_read` ON `cache` (`read`);"],
        )?;
        // A blob listed again while already held takes its size.
        lock(&db)?
            .execute_batch(
                "UPDATE `blobs` SET `size` = (SELECT MAX(`held`.`size`) FROM `blobs` AS `held`
                                          WHERE `held`.`hash` = `blobs`.`hash`)
                 WHERE `size` IS NULL;",
            )
            .map_err(storage_error)?;
        Ok(Self {
            dir: data,
            cache: dir.join("cache"),
            db,
        })
    }

    pub(crate) fn path(&self, hash: &[u8]) -> PathBuf {
        let name = hex(hash);
        self.dir.join(&name[..2]).join(&name[2..4]).join(name)
    }

    /// Replaces the list for a newer version, or refreshes an identical list for a newer signed
    /// time. The caller validates time against the clock. Deletes orphan files and returns missing
    /// hashes; `None` means an older version, repeated time, or a conflicting same-version list.
    pub fn keep(
        &self,
        owner: &str,
        version: u64,
        hashes: &[Vec<u8>],
        time: u64,
    ) -> Result<Option<Vec<Vec<u8>>>> {
        self.keep_record(owner, version, hashes, time, None)
    }

    /// Persists the already-verified signature atomically with the latest keep.
    pub fn keep_signed(
        &self,
        owner: &str,
        keep: &proto::MediaKeep,
        record: &SignedRecord,
    ) -> Result<Option<Vec<Vec<u8>>>> {
        self.keep_record(owner, keep.version, &keep.hashes, keep.time, Some(record))
    }

    fn keep_record(
        &self,
        owner: &str,
        version: u64,
        hashes: &[Vec<u8>],
        time: u64,
        record: Option<&SignedRecord>,
    ) -> Result<Option<Vec<Vec<u8>>>> {
        let version = i64::try_from(version).map_err(storage_error)?;
        let time = i64::try_from(time).map_err(storage_error)?;
        let mut db = lock(&self.db)?;
        let tx = db.transaction().map_err(storage_error)?;
        let stored: Option<(i64, i64)> = tx
            .query_row(
                "SELECT `version`, `last_seen` FROM `owners` WHERE `owner` = ?",
                [owner],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(storage_error)?;
        let old: Vec<Vec<u8>> = tx
            .prepare("SELECT `hash` FROM `blobs` WHERE `owner` = ?")
            .and_then(|mut q| q.query_map([owner], |row| row.get(0))?.collect())
            .map_err(storage_error)?;
        if let Some((stored_version, seen)) = stored {
            if stored_version > version
                || (stored_version == version
                    && (time <= seen
                        || old.iter().any(|hash| !hashes.contains(hash))
                        || hashes.iter().any(|hash| !old.contains(hash))))
            {
                return Ok(None);
            }
        }
        tx.execute(
            "INSERT INTO `owners` (`owner`, `version`, `last_seen`, `record`) VALUES (?, ?, ?, ?) \
             ON CONFLICT(`owner`) DO UPDATE SET `version` = `excluded`.`version`, \
             `last_seen` = MAX(`owners`.`last_seen`, `excluded`.`last_seen`), `record` = `excluded`.`record`",
            params![owner, version, time, record.map(Message::encode_to_vec)],
        )
        .map_err(storage_error)?;
        let mut dropped = Vec::new();
        for hash in old.into_iter().filter(|hash| !hashes.contains(hash)) {
            tx.execute(
                "DELETE FROM `blobs` WHERE `owner` = ? AND `hash` = ?",
                params![owner, hash],
            )
            .map_err(storage_error)?;
            dropped.push(hash);
        }
        let missing = list(&tx, owner, hashes)?;
        let orphans = orphans(&tx, dropped)?;
        tx.commit().map_err(storage_error)?;
        // Still under the lock, so no put of the same blob runs in between.
        self.remove(orphans)?;
        Ok(Some(missing))
    }

    /// Lists `hashes` for `owner` in the attachment released by the secret hashing to `release`,
    /// until `expires` (Unix seconds).
    pub fn attach(
        &self,
        owner: &str,
        release: &[u8],
        hashes: &[Vec<u8>],
        expires: u64,
        created: u64,
        max: usize,
    ) -> Result<Attached> {
        self.attach_record(owner, release, hashes, (expires, created), max, None)
    }

    /// Like [`Self::attach`], and persists the already-verified signed `record` with the slot.
    pub fn attach_signed(
        &self,
        owner: &str,
        attach: &proto::MediaAttach,
        expires: u64,
        max: usize,
        record: &SignedRecord,
    ) -> Result<Attached> {
        self.attach_record(
            owner,
            &attach.release,
            &attach.hashes,
            (expires, attach.created),
            max,
            Some(record),
        )
    }

    fn attach_record(
        &self,
        owner: &str,
        release: &[u8],
        hashes: &[Vec<u8>],
        (expires, created): (u64, u64),
        max: usize,
        record: Option<&SignedRecord>,
    ) -> Result<Attached> {
        let slot = format!("r:{}:{owner}", hex(release));
        let expires = i64::try_from(expires).map_err(storage_error)?;
        let created = i64::try_from(created).map_err(storage_error)?;
        let mut db = lock(&self.db)?;
        let tx = db.transaction().map_err(storage_error)?;
        let released: bool = tx
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM `released` WHERE `release` = ?)",
                [release],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        if released {
            return Ok(Attached::Released);
        }
        let others: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM `attachments` WHERE `owner` = ? AND `slot` != ?",
                params![owner, slot],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        if others.unsigned_abs() >= max as u64 {
            return Ok(Attached::Full);
        }
        // ponytail: a replayed attach renews the expiry, at most to its `created` + retention.
        tx.execute(
            "INSERT OR REPLACE INTO `attachments` (`slot`, `release`, `owner`, `expires`, `created`, `record`) \
             VALUES (?, ?, ?, ?, ?, ?)",
            params![
                slot,
                release,
                owner,
                expires,
                created,
                record.map(Message::encode_to_vec)
            ],
        )
        .map_err(storage_error)?;
        tx.execute(
            "INSERT INTO `owners` (`owner`, `version`, `last_seen`) VALUES (?, 0, ?) \
             ON CONFLICT(`owner`) DO UPDATE SET `last_seen` = MAX(`owners`.`last_seen`, `excluded`.`last_seen`)",
            params![owner, created],
        )
        .map_err(storage_error)?;
        let missing = list(&tx, &slot, hashes)?;
        tx.commit().map_err(storage_error)?;
        Ok(Attached::Missing(missing))
    }

    /// A stored keep is authoritative even when a replica carries an older signed list.
    pub fn kept(&self, owner: &str, hash: &[u8]) -> Result<bool> {
        lock(&self.db)?
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM `blobs` WHERE `owner` = ? AND `hash` = ?)",
                params![owner, hash],
                |row| row.get(0),
            )
            .map_err(storage_error)
    }

    /// Owners after `after` with a signed keep, in owner order, and that keep.
    pub fn keeps_after(&self, after: &str, limit: u32) -> Result<Vec<(String, SignedRecord)>> {
        let db = lock(&self.db)?;
        let mut query = db.prepare(KEEPS_AFTER).map_err(storage_error)?;
        let rows: Vec<(String, Vec<u8>)> = query
            .query_map(params![after, limit], |row| Ok((row.get(0)?, row.get(1)?)))
            .and_then(Iterator::collect)
            .map_err(storage_error)?;
        rows.into_iter()
            .map(|(owner, bytes)| {
                let record = SignedRecord::decode(bytes.as_slice()).map_err(storage_error)?;
                Ok((owner, record))
            })
            .collect()
    }

    /// The signed keep of `owner`, if any.
    pub fn keep_of(&self, owner: &str) -> Result<Option<SignedRecord>> {
        let bytes: Option<Option<Vec<u8>>> = lock(&self.db)?
            .query_row(
                "SELECT `record` FROM `owners` WHERE `owner` = ?",
                params![owner],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage_error)?;
        bytes
            .flatten()
            .map(|bytes| SignedRecord::decode(bytes.as_slice()).map_err(storage_error))
            .transpose()
    }

    /// Returns the latest signed list naming this hash; legacy unsigned rows need a fresh keep.
    pub fn authorization(&self, owner: &str, hash: &[u8]) -> Result<Option<Authorization>> {
        let now = chrono::Utc::now().timestamp();
        let held: Option<(bool, Vec<u8>)> = lock(&self.db)?.query_row(
            "SELECT 1, `attachments`.`record` FROM `attachments` JOIN `blobs` ON `blobs`.`owner` = `attachments`.`slot`
             WHERE `attachments`.`owner` = ?1 AND `blobs`.`hash` = ?2 AND `attachments`.`expires` > ?3
             AND `attachments`.`record` IS NOT NULL
             UNION ALL
             SELECT 0, `owners`.`record` FROM `owners` JOIN `blobs` USING(`owner`)
             WHERE `owners`.`owner` = ?1 AND `blobs`.`hash` = ?2 AND `owners`.`record` IS NOT NULL LIMIT 1",
            params![owner, hash, now], |row| Ok((row.get(0)?, row.get(1)?)),
        ).optional().map_err(storage_error)?;
        held.map(|(attach, bytes)| {
            let record = SignedRecord::decode(bytes.as_slice()).map_err(storage_error)?;
            Ok(if attach {
                Authorization::Attach(record)
            } else {
                Authorization::Keep(record)
            })
        })
        .transpose()
    }

    /// Drops the attachments released by `secret` and refuses their release hash until they
    /// would have expired; false if there were none.
    pub fn release(&self, secret: &[u8]) -> Result<bool> {
        let release = dyapp_identity::sha256(secret);
        Ok(self.drop_attachments(
            "`release` = ?",
            "INSERT OR REPLACE INTO `released` SELECT `release`, MAX(`expires`) FROM `attachments`
             WHERE `release` = ?1 GROUP BY `release`",
            release.as_slice(),
        )? > 0)
    }

    /// Drops the attachments and release records expired at `now` (Unix seconds); returns how
    /// many attachments.
    pub fn expire(&self, now: u64) -> Result<usize> {
        self.drop_attachments(
            "`expires` <= ?",
            "DELETE FROM `released` WHERE `expires` <= ?1",
            i64::try_from(now).map_err(storage_error)?,
        )
    }

    /// Runs `first` with `param`, then drops the attachments matching `which`.
    fn drop_attachments(
        &self,
        which: &'static str,
        first: &'static str,
        param: impl rusqlite::ToSql,
    ) -> Result<usize> {
        let mut db = lock(&self.db)?;
        let tx = db.transaction().map_err(storage_error)?;
        tx.execute(first, [&param]).map_err(storage_error)?;
        let slots: Vec<String> = tx
            .prepare(&format!("SELECT `slot` FROM `attachments` WHERE {which}"))
            .and_then(|mut q| q.query_map([&param], |row| row.get(0))?.collect())
            .map_err(storage_error)?;
        let orphans = Self::drop_lists(&tx, &slots, true)?;
        tx.commit().map_err(storage_error)?;
        self.remove(orphans)?;
        Ok(slots.len())
    }

    /// Drops inactive owners' keeps. Attachments retain their independent lifetime.
    pub fn expire_owners(&self, cutoff: i64) -> Result<usize> {
        let mut db = lock(&self.db)?;
        let tx = db.transaction().map_err(storage_error)?;
        let owners: Vec<String> = tx
            .prepare("SELECT `owner` FROM `owners` WHERE `last_seen` <= ?")
            .and_then(|mut q| q.query_map([cutoff], |row| row.get(0))?.collect())
            .map_err(storage_error)?;
        let orphans = Self::drop_lists(&tx, &owners, false)?;
        tx.commit().map_err(storage_error)?;
        self.remove(orphans)?;
        Ok(owners.len())
    }

    fn drop_lists(tx: &Connection, owners: &[String], attachments: bool) -> Result<Vec<Vec<u8>>> {
        let mut dropped = Vec::new();
        for owner in owners {
            let hashes: Vec<Vec<u8>> = tx
                .prepare("SELECT `hash` FROM `blobs` WHERE `owner` = ?")
                .and_then(|mut q| q.query_map([owner], |row| row.get(0))?.collect())
                .map_err(storage_error)?;
            dropped.extend(hashes);
            tx.execute("DELETE FROM `blobs` WHERE `owner` = ?", [owner])
                .and_then(|_| {
                    if attachments {
                        tx.execute("DELETE FROM `attachments` WHERE `slot` = ?", [owner])
                    } else {
                        tx.execute("DELETE FROM `owners` WHERE `owner` = ?", [owner])
                    }
                })
                .map_err(storage_error)?;
        }
        orphans(tx, dropped)
    }

    /// Evicts assembled blobs by last read, then attachments by creation time, then keeps by
    /// liveness, down to `target` bytes.
    pub fn evict(&self, target: u64) -> Result<usize> {
        let mut db = lock(&self.db)?;
        let mut removed = 0;
        while Self::used(&db)? > target {
            let hash: Option<Vec<u8>> = db
                .query_row(
                    "DELETE FROM `cache` WHERE `hash` = \
                     (SELECT `hash` FROM `cache` ORDER BY `read` LIMIT 1) RETURNING `hash`",
                    [],
                    |row| row.get(0),
                )
                .optional()
                .map_err(storage_error)?;
            let Some(hash) = hash else { break };
            remove_file(&self.cache.join(hex(&hash)))?;
            removed += 1;
        }
        for (attachments, query) in [
            (
                true,
                "SELECT `slot` FROM `attachments` ORDER BY `created`, `slot` LIMIT 1",
            ),
            (
                false,
                "SELECT `owner` FROM `owners` ORDER BY `last_seen`, `owner` LIMIT 1",
            ),
        ] {
            // Bound work per request even when many lists hold the same blob.
            for _ in 0..4096 {
                if Self::used(&db)? <= target {
                    return Ok(removed);
                }
                let owner: Option<String> = db
                    .query_row(query, [], |row| row.get(0))
                    .optional()
                    .map_err(storage_error)?;
                let Some(owner) = owner else { break };
                let tx = db.transaction().map_err(storage_error)?;
                let orphans = Self::drop_lists(&tx, &[owner], attachments)?;
                tx.commit().map_err(storage_error)?;
                self.remove(orphans)?;
                removed += 1;
            }
            if attachments {
                let remain: bool = db
                    .query_row("SELECT EXISTS (SELECT 1 FROM `attachments`)", [], |row| {
                        row.get(0)
                    })
                    .map_err(storage_error)?;
                if remain {
                    return Ok(removed);
                }
            }
        }
        Ok(removed)
    }

    /// Removes a deny-listed blob's local payload while preserving keep and attachment rows.
    /// Returns whether stored content or storage metadata was present.
    pub fn purge(&self, hash: &[u8]) -> Result<bool> {
        let mut db = lock(&self.db)?;
        let stored: bool = db
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM `blobs` WHERE `hash` = ?1 AND `size` IS NOT NULL) \
                 OR EXISTS (SELECT 1 FROM `manifests` WHERE `hash` = ?1) \
                 OR EXISTS (SELECT 1 FROM `shards` WHERE `hash` = ?1)",
                [hash],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        let file = remove_file(&self.path(hash))? | remove_file(&self.cache.join(hex(hash)))?;
        let tx = db.transaction().map_err(storage_error)?;
        tx.execute("DELETE FROM `cache` WHERE `hash` = ?", [hash])
            .map_err(storage_error)?;
        tx.execute("DELETE FROM `shards` WHERE `hash` = ?", [hash])
            .map_err(storage_error)?;
        tx.execute("DELETE FROM `manifests` WHERE `hash` = ?", [hash])
            .map_err(storage_error)?;
        tx.execute(
            "UPDATE `blobs` SET `size` = NULL, `cost` = NULL WHERE `hash` = ?",
            [hash],
        )
        .map_err(storage_error)?;
        if stored || file {
            tx.execute(
                "UPDATE `purges` SET `count` = `count` + 1 WHERE `id` = 1",
                [],
            )
            .map_err(storage_error)?;
        }
        tx.commit().map_err(storage_error)?;
        if stored || file {
            tracing::info!(hash = %hex(hash), "deny-listed media purged");
        }
        Ok(stored || file)
    }

    fn remove(&self, hashes: Vec<Vec<u8>>) -> Result<()> {
        for hash in hashes {
            remove_file(&self.path(&hash))?;
        }
        Ok(())
    }

    /// Keeps an assembled blob for the next pieces; `now` in Unix seconds.
    pub fn cache(&self, hash: &[u8], data: &[u8], now: i64) -> Result<()> {
        let db = lock(&self.db)?;
        write_aside(&self.cache.join(hex(hash)), data)?;
        db.execute(
            "INSERT OR REPLACE INTO `cache` (`hash`, `size`, `read`) VALUES (?, ?, ?)",
            params![hash, data.len() as i64, now],
        )
        .map_err(storage_error)?;
        Ok(())
    }

    /// An assembled blob, refreshing its last read.
    pub fn cached(&self, hash: &[u8], now: i64) -> Result<Option<Vec<u8>>> {
        let db = lock(&self.db)?;
        let held = db
            .execute(
                "UPDATE `cache` SET `read` = ? WHERE `hash` = ?",
                params![now, hash],
            )
            .map_err(storage_error)?;
        if held == 0 {
            return Ok(None);
        }
        match fs::read(self.cache.join(hex(hash))) {
            Ok(data) => Ok(Some(data)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                db.execute("DELETE FROM `cache` WHERE `hash` = ?", [hash])
                    .map_err(storage_error)?;
                Ok(None)
            }
            Err(e) => Err(storage_error(e)),
        }
    }

    /// Drops assembled blobs last read before `before` (Unix seconds); returns how many.
    pub fn expire_cache(&self, before: i64) -> Result<usize> {
        let db = lock(&self.db)?;
        let hashes: Vec<Vec<u8>> = db
            .prepare("DELETE FROM `cache` WHERE `read` < ? RETURNING `hash`")
            .and_then(|mut q| q.query_map([before], |row| row.get(0))?.collect())
            .map_err(storage_error)?;
        for hash in &hashes {
            remove_file(&self.cache.join(hex(hash)))?;
        }
        Ok(hashes.len())
    }

    /// Stores `data` for `owner` if a keep or an attachment of the owner lists its hash and the
    /// owner stays within `quota` bytes. A repeated put is `Stored`.
    pub fn put(&self, owner: &str, data: &[u8], quota: u64) -> Result<Put> {
        let hash = dyapp_identity::sha256(data);
        let mut db = lock(&self.db)?;
        let (rows, stored): (i64, i64) = db
            .query_row(
                &format!(
                    "SELECT COUNT(*), COALESCE(MIN(`size` IS NOT NULL AND `hash` NOT IN (SELECT `hash` FROM `manifests`)), 0) FROM `blobs` \
                     WHERE `hash` = ?2 AND {OWNED}"
                ),
                params![owner, hash.as_slice()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(storage_error)?;
        if rows == 0 {
            return Ok(Put::NotListed);
        }
        let used: i64 = db
            .query_row(
                &format!(
                    "SELECT COALESCE(SUM(COALESCE(`cost`, `size` * 5)), 0) FROM `blobs` WHERE {OWNED}"
                ),
                [owner],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        let previous: i64 = db.query_row(
            &format!("SELECT COALESCE(SUM(COALESCE(`cost`, `size` * 5)), 0) FROM `blobs` WHERE `hash` = ?2 AND {OWNED}"),
            params![owner, hash.as_slice()], |r| r.get(0)
        ).map_err(storage_error)?;
        let charge = if stored == 1 {
            used.unsigned_abs()
        } else {
            used.unsigned_abs()
                .saturating_sub(previous.unsigned_abs())
                .saturating_add(
                    (data.len() as u64)
                        .saturating_mul(5)
                        .saturating_mul(rows.unsigned_abs()),
                )
        };
        if charge > quota.saturating_mul(5) {
            return Ok(Put::OverQuota);
        }
        let held = self.get(&hash)?;
        if stored == 1 && held.as_deref() == Some(data) {
            return Ok(Put::Stored);
        }
        if held.as_deref() != Some(data) {
            write_aside(&self.path(&hash), data)?;
        }
        let tx = db.transaction().map_err(storage_error)?;
        tx.execute(
            "UPDATE `blobs` SET `size` = ?1, `cost` = ?1 * 5 WHERE `hash` = ?2",
            params![data.len() as i64, hash.as_slice()],
        )
        .map_err(storage_error)?;
        tx.execute("DELETE FROM `shards` WHERE `hash` = ?", [hash.as_slice()])
            .map_err(storage_error)?;
        tx.execute(
            "DELETE FROM `manifests` WHERE `hash` = ?",
            [hash.as_slice()],
        )
        .map_err(storage_error)?;
        tx.commit().map_err(storage_error)?;
        Ok(Put::Stored)
    }

    /// The stored manifest of a sharded blob.
    pub fn manifest(&self, hash: &[u8]) -> Result<Option<proto::MediaManifest>> {
        let bytes: Option<Vec<u8>> = lock(&self.db)?
            .query_row(
                "SELECT `record` FROM `manifests` WHERE `hash` = ?",
                [hash],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage_error)?;
        bytes
            .map(|b| proto::MediaManifest::decode(b.as_slice()).map_err(storage_error))
            .transpose()
    }

    /// Stores the manifest of a blob the owner lists, charging its billed bytes to `quota`.
    pub fn put_manifest(
        &self,
        owner: &str,
        manifest: &proto::MediaManifest,
        quota: u64,
    ) -> Result<Put> {
        if let Some(data) = self.get(&manifest.hash)? {
            if dyapp_identity::sha256(&data).as_slice() == manifest.hash {
                return self.put(owner, &data, quota / 5);
            }
        }
        let mut db = lock(&self.db)?;
        let tx = db.transaction().map_err(storage_error)?;
        let previous: Option<Vec<u8>> = tx
            .query_row(
                "SELECT `record` FROM `manifests` WHERE `hash` = ?",
                [&manifest.hash],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage_error)?;
        let bytes = manifest.encode_to_vec();
        if previous.as_ref().is_some_and(|old| *old != bytes) {
            return Ok(Put::Invalid);
        }
        let (rows, held): (i64, Option<i64>) = tx.query_row(
            &format!("SELECT COUNT(*), MIN(COALESCE(`cost`, `size` * 5)) FROM `blobs` WHERE `hash` = ?2 AND {OWNED}"),
            params![owner, manifest.hash], |r| Ok((r.get(0)?, r.get(1)?))
        ).map_err(storage_error)?;
        if rows == 0 {
            return Ok(Put::NotListed);
        }
        let cost = manifest
            .billed_bytes()
            .ok_or_else(|| storage_error("invalid manifest"))?;
        let used: i64 = tx
            .query_row(
                &format!(
                    "SELECT COALESCE(SUM(COALESCE(`cost`, `size` * 5)), 0) FROM `blobs` WHERE {OWNED}"
                ),
                [owner],
                |r| r.get(0),
            )
            .map_err(storage_error)?;
        let extra = cost
            .saturating_sub(held.map_or(0, i64::unsigned_abs))
            .saturating_mul(rows.unsigned_abs());
        if used.unsigned_abs().saturating_add(extra) > quota {
            return Ok(Put::OverQuota);
        }
        tx.execute(
            "INSERT OR IGNORE INTO `manifests` (`hash`, `record`) VALUES (?, ?)",
            params![manifest.hash, bytes],
        )
        .map_err(storage_error)?;
        tx.execute(
            "UPDATE `blobs` SET `size` = ?1, `cost` = ?2 WHERE `hash` = ?3",
            params![manifest.length as i64, cost as i64, manifest.hash],
        )
        .map_err(storage_error)?;
        tx.commit().map_err(storage_error)?;
        Ok(Put::Stored)
    }

    /// Stores shard `index` of a blob whose manifest is stored.
    pub fn put_shard(
        &self,
        manifest: &proto::MediaManifest,
        index: usize,
        data: &[u8],
    ) -> Result<Put> {
        if !manifest.accepts(index, data) {
            return Ok(Put::Invalid);
        }
        // A verified whole copy can generate the canonical shard without another stored object.
        if self
            .get(&manifest.hash)?
            .is_some_and(|d| dyapp_identity::sha256(&d).as_slice() == manifest.hash)
        {
            return Ok(Put::Stored);
        }
        let db = lock(&self.db)?;
        let held: Option<Vec<u8>> = db
            .query_row(
                "SELECT `record` FROM `manifests` WHERE `hash` = ?",
                [&manifest.hash],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage_error)?;
        if held.as_deref() != Some(manifest.encode_to_vec().as_slice()) {
            return Ok(Put::NotListed);
        }
        db.execute(
            "INSERT INTO `shards` (`hash`, `idx`, `data`) VALUES (?, ?, ?)
             ON CONFLICT(`hash`, `idx`) DO UPDATE SET `data` = `excluded`.`data`",
            params![manifest.hash, index as i64, data],
        )
        .map_err(storage_error)?;
        Ok(Put::Stored)
    }

    /// Shard `index` of a blob: stored, or encoded from a verified whole copy.
    pub fn shard(&self, hash: &[u8], index: usize) -> Result<Option<Vec<u8>>> {
        let data: Option<Vec<u8>> = lock(&self.db)?
            .query_row(
                "SELECT `data` FROM `shards` WHERE `hash` = ? AND `idx` = ?",
                params![hash, index as i64],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage_error)?;
        if data.is_some() {
            return Ok(data);
        }
        if let Some(whole) = self.get(hash)? {
            if dyapp_identity::sha256(&whole).as_slice() == hash && !whole.is_empty() {
                let (_, shards) = crate::replication::media_encode(&whole)?;
                return Ok(shards.get(index).cloned());
            }
        }
        Ok(None)
    }

    /// Invalid decoded content is discarded locally; ownership lists remain for reupload.
    pub fn discard_sharded(&self, hash: &[u8]) -> Result<()> {
        let mut db = lock(&self.db)?;
        let tx = db.transaction().map_err(storage_error)?;
        tx.execute("DELETE FROM `shards` WHERE `hash` = ?", [hash])
            .map_err(storage_error)?;
        tx.execute("DELETE FROM `manifests` WHERE `hash` = ?", [hash])
            .map_err(storage_error)?;
        tx.execute(
            "UPDATE `blobs` SET `size` = NULL, `cost` = NULL WHERE `hash` = ?",
            [hash],
        )
        .map_err(storage_error)?;
        tx.commit().map_err(storage_error)?;
        Ok(())
    }

    /// The whole blob file, if stored.
    pub fn get(&self, hash: &[u8]) -> Result<Option<Vec<u8>>> {
        match fs::read(self.path(hash)) {
            Ok(data) => Ok(Some(data)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(storage_error(e)),
        }
    }

    /// Bytes of distinct stored blobs, and bytes free on their file system.
    pub fn usage(&self) -> Result<(u64, u64)> {
        Ok((
            Self::used(&*lock(&self.db)?)?,
            free(&self.dir.to_string_lossy())?,
        ))
    }

    fn used(db: &Connection) -> Result<u64> {
        db.query_row(
            "SELECT COALESCE((SELECT SUM(length(`record`)) FROM `manifests`), 0) + COALESCE((SELECT SUM(length(`data`)) FROM `shards`), 0) + COALESCE((SELECT SUM(`size`) FROM `cache`), 0) + COALESCE(SUM(`size`), 0) FROM \
             (SELECT MAX(`size`) AS `size` FROM `blobs` WHERE `size` IS NOT NULL AND `hash` NOT IN (SELECT `hash` FROM `manifests`) GROUP BY `hash`)",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(i64::unsigned_abs)
        .map_err(storage_error)
    }
}

#[cfg(test)]
mod tests;
