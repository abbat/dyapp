//! Media blobs of the media role: one file per blob at `<dir>/aa/bb/<hash>`, and in `media.db`
//! which owner lists which blob. A blob listed by several owners is stored once and counts
//! against each owner's quota; its file goes when the last owner drops it.
//!
//! A chat attachment is a slot `r:<release hash>:<owner>` in `attachments`, listing its blobs in
//! `blobs` like an owner; the owner's puts and quota cover its slots. A slot goes on release or
//! expiry.

use crate::storage::{free, lock, open, storage_error};
use crate::Result;
use rusqlite::{params, Connection, OptionalExtension};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// What became of a put.
#[derive(Debug, PartialEq, Eq)]
pub enum Put {
    Stored,
    /// No keep of the owner lists the hash.
    NotListed,
    /// The owner's blobs would exceed the quota.
    OverQuota,
}

pub struct MediaStore {
    dir: PathBuf,
    // ponytail: one connection; puts hold it while the file is written (1 MiB at most).
    db: Mutex<Connection>,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Rows of the owner `?1`: its keep and its attachments.
const OWNED: &str = "(owner = ?1 OR owner IN (SELECT slot FROM attachments WHERE owner = ?1))";

/// Lists `hashes` for `owner`; returns those it has not stored yet.
fn list(tx: &Connection, owner: &str, hashes: &[Vec<u8>]) -> Result<Vec<Vec<u8>>> {
    let mut missing = Vec::new();
    for hash in hashes {
        tx.execute(
            "INSERT INTO blobs (owner, hash, size) \
             VALUES (?1, ?2, (SELECT MAX(size) FROM blobs WHERE hash = ?2)) \
             ON CONFLICT(owner, hash) DO UPDATE SET size = COALESCE(blobs.size, excluded.size)",
            params![owner, hash],
        )
        .map_err(storage_error)?;
        let size: Option<i64> = tx
            .query_row(
                "SELECT size FROM blobs WHERE owner = ? AND hash = ?",
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
                "SELECT EXISTS (SELECT 1 FROM blobs WHERE hash = ?)",
                [&hash],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        if !held {
            orphans.push(hash);
        }
    }
    Ok(orphans)
}

impl MediaStore {
    /// Opens `dir/media.db` with the blob files under `dir/data`.
    pub fn open(dir: &Path) -> Result<Self> {
        let (data, db) = (dir.join("data"), dir.join("media.db"));
        // Older nodes kept both in `dir/media`. Each step is a rename, so a crash between them
        // is finished by the next start.
        let old = dir.join("media");
        if old.join("media.db").exists() && !data.exists() {
            fs::rename(&old, &data).map_err(storage_error)?;
            tracing::info!(from = %old.display(), to = %data.display(), "media store moved");
        }
        if data.join("media.db").exists() && !db.exists() {
            fs::rename(data.join("media.db"), &db).map_err(storage_error)?;
        }
        fs::create_dir_all(&data).map_err(storage_error)?;
        let db = open(
            &db,
            // `size` is NULL while a listed blob has not been put.
            "CREATE TABLE IF NOT EXISTS owners (
                 owner TEXT PRIMARY KEY,
                 version INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS blobs (
                 owner TEXT NOT NULL,
                 hash BLOB NOT NULL,
                 size INTEGER,
                 PRIMARY KEY (owner, hash)
             );
             CREATE INDEX IF NOT EXISTS blobs_hash ON blobs (hash);
             CREATE TABLE IF NOT EXISTS attachments (
                 slot TEXT PRIMARY KEY,
                 release BLOB NOT NULL,
                 owner TEXT NOT NULL,
                 expires INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS attachments_owner ON attachments (owner);
             CREATE INDEX IF NOT EXISTS attachments_release ON attachments (release);",
        )?;
        {
            let connection = lock(&db)?;
            for (table, column, default) in [
                ("owners", "last_seen", chrono::Utc::now().timestamp()),
                ("attachments", "created", 0),
            ] {
                let exists: bool = connection
                    .query_row(
                        &format!("SELECT EXISTS (SELECT 1 FROM pragma_table_info('{table}') WHERE name = ?)"),
                        [column],
                        |row| row.get(0),
                    )
                    .map_err(storage_error)?;
                if !exists {
                    connection.execute_batch(&format!(
                        "ALTER TABLE {table} ADD COLUMN {column} INTEGER NOT NULL DEFAULT {default};"
                    )).map_err(storage_error)?;
                }
            }
            connection
                .execute_batch(
                    "CREATE INDEX IF NOT EXISTS owners_seen ON owners (last_seen);
                 CREATE INDEX IF NOT EXISTS attachments_created ON attachments (created);
                 UPDATE blobs SET size = (SELECT MAX(held.size) FROM blobs AS held
                                          WHERE held.hash = blobs.hash)
                 WHERE size IS NULL;",
                )
                .map_err(storage_error)?;
        }
        Ok(Self { dir: data, db })
    }

    fn path(&self, hash: &[u8]) -> PathBuf {
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
        let version = i64::try_from(version).map_err(storage_error)?;
        let time = i64::try_from(time).map_err(storage_error)?;
        let mut db = lock(&self.db)?;
        let tx = db.transaction().map_err(storage_error)?;
        let stored: Option<(i64, i64)> = tx
            .query_row(
                "SELECT version, last_seen FROM owners WHERE owner = ?",
                [owner],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(storage_error)?;
        let old: Vec<Vec<u8>> = tx
            .prepare("SELECT hash FROM blobs WHERE owner = ?")
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
            "INSERT INTO owners (owner, version, last_seen) VALUES (?, ?, ?) \
             ON CONFLICT(owner) DO UPDATE SET version = excluded.version, \
             last_seen = MAX(owners.last_seen, excluded.last_seen)",
            params![owner, version, time],
        )
        .map_err(storage_error)?;
        let mut dropped = Vec::new();
        for hash in old.into_iter().filter(|hash| !hashes.contains(hash)) {
            tx.execute(
                "DELETE FROM blobs WHERE owner = ? AND hash = ?",
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
    /// until `expires` (Unix seconds). Returns the hashes not stored yet, or `None` if the owner
    /// already has `max` other attachments.
    pub fn attach(
        &self,
        owner: &str,
        release: &[u8],
        hashes: &[Vec<u8>],
        expires: u64,
        created: u64,
        max: usize,
    ) -> Result<Option<Vec<Vec<u8>>>> {
        let slot = format!("r:{}:{owner}", hex(release));
        let expires = i64::try_from(expires).map_err(storage_error)?;
        let created = i64::try_from(created).map_err(storage_error)?;
        let mut db = lock(&self.db)?;
        let tx = db.transaction().map_err(storage_error)?;
        let others: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM attachments WHERE owner = ? AND slot != ?",
                params![owner, slot],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        if others.unsigned_abs() >= max as u64 {
            return Ok(None);
        }
        // ponytail: a replayed attach renews the expiry, at most to its `created` + retention.
        tx.execute(
            "INSERT OR REPLACE INTO attachments (slot, release, owner, expires, created) \
             VALUES (?, ?, ?, ?, ?)",
            params![slot, release, owner, expires, created],
        )
        .map_err(storage_error)?;
        tx.execute(
            "INSERT INTO owners (owner, version, last_seen) VALUES (?, 0, ?) \
             ON CONFLICT(owner) DO UPDATE SET last_seen = MAX(owners.last_seen, excluded.last_seen)",
            params![owner, created],
        )
        .map_err(storage_error)?;
        let missing = list(&tx, &slot, hashes)?;
        tx.commit().map_err(storage_error)?;
        Ok(Some(missing))
    }

    /// Drops the attachments released by `secret`; false if there were none.
    pub fn release(&self, secret: &[u8]) -> Result<bool> {
        let release = dyapp_identity::sha256(secret);
        Ok(self.drop_attachments("release = ?", release.as_slice())? > 0)
    }

    /// Drops the attachments expired at `now` (Unix seconds); returns how many.
    pub fn expire(&self, now: u64) -> Result<usize> {
        self.drop_attachments("expires <= ?", i64::try_from(now).map_err(storage_error)?)
    }

    fn drop_attachments(&self, which: &str, param: impl rusqlite::ToSql) -> Result<usize> {
        let mut db = lock(&self.db)?;
        let tx = db.transaction().map_err(storage_error)?;
        let slots: Vec<String> = tx
            .prepare(&format!("SELECT slot FROM attachments WHERE {which}"))
            .and_then(|mut q| q.query_map([param], |row| row.get(0))?.collect())
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
            .prepare("SELECT owner FROM owners WHERE last_seen <= ?")
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
                .prepare("SELECT hash FROM blobs WHERE owner = ?")
                .and_then(|mut q| q.query_map([owner], |row| row.get(0))?.collect())
                .map_err(storage_error)?;
            dropped.extend(hashes);
            tx.execute("DELETE FROM blobs WHERE owner = ?", [owner])
                .and_then(|_| {
                    if attachments {
                        tx.execute("DELETE FROM attachments WHERE slot = ?", [owner])
                    } else {
                        tx.execute("DELETE FROM owners WHERE owner = ?", [owner])
                    }
                })
                .map_err(storage_error)?;
        }
        orphans(tx, dropped)
    }

    /// Evicts attachments by creation time, then keeps by liveness, down to `target` bytes.
    pub fn evict(&self, target: u64) -> Result<usize> {
        let mut db = lock(&self.db)?;
        let mut removed = 0;
        for (attachments, query) in [
            (
                true,
                "SELECT slot FROM attachments ORDER BY created, slot LIMIT 1",
            ),
            (
                false,
                "SELECT owner FROM owners ORDER BY last_seen, owner LIMIT 1",
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
                    .query_row("SELECT EXISTS (SELECT 1 FROM attachments)", [], |row| {
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

    fn remove(&self, hashes: Vec<Vec<u8>>) -> Result<()> {
        for hash in hashes {
            match fs::remove_file(self.path(&hash)) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(storage_error(e)),
                _ => {}
            }
        }
        Ok(())
    }

    /// Stores `data` for `owner` if a keep or an attachment of the owner lists its hash and the
    /// owner stays within `quota` bytes. A repeated put is `Stored`.
    pub fn put(&self, owner: &str, data: &[u8], quota: u64) -> Result<Put> {
        let hash = dyapp_identity::sha256(data);
        let db = lock(&self.db)?;
        let (rows, stored): (i64, i64) = db
            .query_row(
                &format!(
                    "SELECT COUNT(*), COALESCE(MIN(size IS NOT NULL), 0) FROM blobs \
                     WHERE hash = ?2 AND {OWNED}"
                ),
                params![owner, hash.as_slice()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(storage_error)?;
        if rows == 0 {
            return Ok(Put::NotListed);
        }
        if stored == 1 {
            return Ok(Put::Stored);
        }
        let used: i64 = db
            .query_row(
                &format!("SELECT COALESCE(SUM(size), 0) FROM blobs WHERE {OWNED}"),
                [owner],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        if used.unsigned_abs() + data.len() as u64 > quota {
            return Ok(Put::OverQuota);
        }
        let path = self.path(&hash);
        if !path.exists() {
            // Written aside and renamed, so a reader never sees a partial blob.
            let dir = path.parent().unwrap_or(&self.dir);
            fs::create_dir_all(dir).map_err(storage_error)?;
            let temp = dir.join(format!(".{}", uuid::Uuid::new_v4()));
            let mut file = fs::File::create(&temp).map_err(storage_error)?;
            file.write_all(data)
                .and_then(|()| file.sync_all())
                .and_then(|()| fs::rename(&temp, &path))
                .map_err(|e| {
                    let _ = fs::remove_file(&temp);
                    storage_error(e)
                })?;
        }
        db.execute(
            "UPDATE blobs SET size = ? WHERE hash = ?",
            params![data.len() as i64, hash.as_slice()],
        )
        .map_err(storage_error)?;
        Ok(Put::Stored)
    }

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
            "SELECT COALESCE(SUM(size), 0) FROM \
             (SELECT MAX(size) AS size FROM blobs WHERE size IS NOT NULL GROUP BY hash)",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(i64::unsigned_abs)
        .map_err(storage_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keep_put_get_and_drop() {
        let dir = PathBuf::from(format!("/tmp/ai/test-media-{}", uuid::Uuid::new_v4()));
        let media = MediaStore::open(&dir).unwrap();
        let (a, b) = (vec![1u8; 100], vec![2u8; 300]);
        let (ha, hb) = (
            dyapp_identity::sha256(&a).to_vec(),
            dyapp_identity::sha256(&b).to_vec(),
        );

        assert_eq!(media.put("alice", &a, 1000).unwrap(), Put::NotListed);
        let both = [ha.clone(), hb.clone()];
        assert_eq!(
            media.keep("alice", 1, &both, 10).unwrap(),
            Some(both.to_vec())
        );
        assert_eq!(media.keep("alice", 1, &[], 10).unwrap(), None, "stale");
        assert_eq!(media.put("alice", &a, 1000).unwrap(), Put::Stored);
        assert_eq!(media.put("alice", &a, 0).unwrap(), Put::Stored, "repeat");
        assert_eq!(media.put("alice", &b, 350).unwrap(), Put::OverQuota);
        assert_eq!(media.get(&ha).unwrap(), Some(a.clone()));
        assert_eq!(media.get(&hb).unwrap(), None);

        // Bob lists the same blob: stored once, kept while either lists it.
        assert_eq!(
            media.keep("bob", 5, std::slice::from_ref(&ha), 10).unwrap(),
            Some(vec![])
        );
        assert_eq!(media.put("bob", &a, 1000).unwrap(), Put::Stored);
        assert_eq!(media.usage().unwrap().0, 100);
        assert_eq!(
            media
                .keep("alice", 2, std::slice::from_ref(&hb), 20)
                .unwrap(),
            Some(vec![hb])
        );
        assert_eq!(media.get(&ha).unwrap(), Some(a));
        assert_eq!(media.keep("bob", 6, &[], 20).unwrap(), Some(vec![]));
        assert_eq!(media.get(&ha).unwrap(), None);
        assert_eq!(media.usage().unwrap().0, 0);
    }

    #[test]
    fn keep_refresh_uses_signed_time_and_preserves_the_list() {
        let dir = PathBuf::from(format!("/tmp/ai/test-media-{}", uuid::Uuid::new_v4()));
        let media = MediaStore::open(&dir).unwrap();
        let hashes = vec![vec![1; 32]];
        assert_eq!(
            media.keep("alice", 1, &hashes, 100).unwrap(),
            Some(hashes.clone())
        );
        assert_eq!(media.keep("alice", 1, &hashes, 100).unwrap(), None);
        assert_eq!(media.keep("alice", 1, &hashes, 90).unwrap(), None);
        assert_eq!(
            media.keep("alice", 1, &hashes, 110).unwrap(),
            Some(hashes.clone())
        );
        assert_eq!(media.keep("alice", 1, &[], 120).unwrap(), None);
        let db = lock(&media.db).unwrap();
        let seen: i64 = db
            .query_row(
                "SELECT last_seen FROM owners WHERE owner = 'alice'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(seen, 110);
    }

    #[test]
    fn expiry_keeps_shared_and_attached_blobs_and_honours_refresh() {
        let dir = PathBuf::from(format!("/tmp/ai/test-media-{}", uuid::Uuid::new_v4()));
        let media = MediaStore::open(&dir).unwrap();
        let data = vec![1; 100];
        let hash = dyapp_identity::sha256(&data).to_vec();
        let hashes = std::slice::from_ref(&hash);
        media.keep("alice", 1, hashes, 100).unwrap();
        media.keep("bob", 1, hashes, 110).unwrap();
        media.put("alice", &data, 1000).unwrap();
        // The single upload fulfils both lists and is counted while either owner holds it.
        assert_eq!(media.keep("bob", 1, hashes, 111).unwrap(), Some(vec![]));
        assert_eq!(media.expire_owners(100).unwrap(), 1);
        assert_eq!(media.get(&hash).unwrap(), Some(data.clone()));
        media.keep("bob", 1, hashes, 150).unwrap();
        assert_eq!(media.expire_owners(120).unwrap(), 0);
        let release = dyapp_identity::sha256(b"secret");
        assert_eq!(
            media
                .attach("sender", &release, hashes, 1000, 1, 10)
                .unwrap(),
            Some(vec![])
        );
        assert_eq!(media.expire_owners(150).unwrap(), 2);
        assert_eq!(media.get(&hash).unwrap(), Some(data));
        assert_eq!(media.expire(1000).unwrap(), 1);
        assert_eq!(media.get(&hash).unwrap(), None);
    }

    #[test]
    fn eviction_drops_attachments_then_oldest_keeps_and_stops_at_target() {
        let dir = PathBuf::from(format!("/tmp/ai/test-media-{}", uuid::Uuid::new_v4()));
        let media = MediaStore::open(&dir).unwrap();
        let blobs: Vec<Vec<u8>> = (1..=4).map(|byte| vec![byte; 100]).collect();
        let hashes: Vec<Vec<u8>> = blobs
            .iter()
            .map(|data| dyapp_identity::sha256(data).to_vec())
            .collect();
        for (owner, index, time) in [("old", 0, 100), ("young", 1, 200)] {
            media
                .keep(owner, 1, std::slice::from_ref(&hashes[index]), time)
                .unwrap();
            media.put(owner, &blobs[index], 1000).unwrap();
        }
        for (index, created) in [(2, 30), (3, 10), (0, 5)] {
            let release = vec![index as u8; 32];
            media
                .attach(
                    "sender",
                    &release,
                    std::slice::from_ref(&hashes[index]),
                    1000,
                    created,
                    10,
                )
                .unwrap();
            media.put("sender", &blobs[index], 1000).unwrap();
        }
        assert_eq!(media.usage().unwrap().0, 400);
        media.evict(380).unwrap(); // 95% of 400; shared attachment frees no bytes.
        assert_eq!(media.usage().unwrap().0, 300);
        assert_eq!(media.get(&hashes[3]).unwrap(), None);
        assert_eq!(media.get(&hashes[2]).unwrap(), Some(blobs[2].clone()));
        assert_eq!(media.get(&hashes[0]).unwrap(), Some(blobs[0].clone()));
        media.evict(100).unwrap();
        assert_eq!(media.usage().unwrap().0, 100);
        assert_eq!(media.get(&hashes[0]).unwrap(), None);
        assert_eq!(media.get(&hashes[2]).unwrap(), None);
        assert_eq!(media.get(&hashes[1]).unwrap(), Some(blobs[1].clone()));
    }

    #[test]
    fn old_layout_is_moved() {
        let dir = PathBuf::from(format!("/tmp/ai/test-media-{}", uuid::Uuid::new_v4()));
        let data = vec![7u8; 10];
        let hash = dyapp_identity::sha256(&data).to_vec();
        {
            let media = MediaStore::open(&dir).unwrap();
            media
                .keep("bob", 1, std::slice::from_ref(&hash), 10)
                .unwrap();
            media.put("bob", &data, 1000).unwrap();
        }
        fs::rename(dir.join("data"), dir.join("media")).unwrap();
        fs::rename(dir.join("media.db"), dir.join("media/media.db")).unwrap();
        let media = MediaStore::open(&dir).unwrap();
        assert_eq!(media.get(&hash).unwrap(), Some(data));
        assert_eq!(media.usage().unwrap().0, 10);
        assert!(!dir.join("media").exists());
    }

    #[test]
    fn attach_put_release_and_expire() {
        let dir = PathBuf::from(format!("/tmp/ai/test-media-{}", uuid::Uuid::new_v4()));
        let media = MediaStore::open(&dir).unwrap();
        let (a, b) = (vec![1u8; 100], vec![2u8; 300]);
        let (ha, hb) = (
            dyapp_identity::sha256(&a).to_vec(),
            dyapp_identity::sha256(&b).to_vec(),
        );
        let (s1, s2) = (b"one".as_slice(), b"two".as_slice());
        let (r1, r2) = (dyapp_identity::sha256(s1), dyapp_identity::sha256(s2));

        let ha1 = std::slice::from_ref(&ha);
        assert_eq!(
            media.attach("alice", &r1, ha1, 100, 1, 1).unwrap(),
            Some(vec![ha.clone()])
        );
        assert_eq!(
            media.attach("alice", &r2, ha1, 100, 1, 1).unwrap(),
            None,
            "max"
        );
        assert_eq!(
            media.attach("alice", &r1, ha1, 100, 1, 1).unwrap(),
            Some(vec![ha.clone()])
        );
        assert_eq!(media.put("bob", &a, 1000).unwrap(), Put::NotListed);
        assert_eq!(media.put("alice", &a, 1000).unwrap(), Put::Stored);
        // Kept and attached: counted for both.
        media
            .keep("alice", 1, std::slice::from_ref(&hb), 10)
            .unwrap();
        assert_eq!(media.put("alice", &b, 350).unwrap(), Put::OverQuota);
        assert_eq!(media.put("alice", &b, 400).unwrap(), Put::Stored);
        assert_eq!(media.usage().unwrap().0, 400);

        assert!(!media.release(s2).unwrap());
        assert!(media.release(s1).unwrap());
        assert_eq!(media.get(&ha).unwrap(), None);
        assert_eq!(media.get(&hb).unwrap(), Some(b.clone()));

        // An attached blob that is also kept survives its expiry.
        let hb1 = std::slice::from_ref(&hb);
        assert_eq!(
            media.attach("alice", &r2, hb1, 50, 2, 1).unwrap(),
            Some(vec![])
        );
        assert_eq!(media.put("alice", &b, 1000).unwrap(), Put::Stored);
        assert_eq!(media.expire(49).unwrap(), 0);
        assert_eq!(media.expire(50).unwrap(), 1);
        assert_eq!(media.get(&hb).unwrap(), Some(b));
        assert!(!media.release(s2).unwrap());
    }
}
