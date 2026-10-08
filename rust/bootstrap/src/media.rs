//! Media blobs of the media role: one file per blob at `<dir>/aa/bb/<hash>`, and in `media.db`
//! which owner lists which blob. A blob listed by several owners is stored once and counts
//! against each owner's quota; its file goes when the last owner drops it.

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

impl MediaStore {
    pub fn open(dir: &Path) -> Result<Self> {
        fs::create_dir_all(dir).map_err(storage_error)?;
        let db = open(
            &dir.join("media.db"),
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
             CREATE INDEX IF NOT EXISTS blobs_hash ON blobs (hash);",
        )?;
        Ok(Self {
            dir: dir.into(),
            db,
        })
    }

    fn path(&self, hash: &[u8]) -> PathBuf {
        let name = hex(hash);
        self.dir.join(&name[..2]).join(&name[2..4]).join(name)
    }

    /// Replaces the owner's list with `hashes` if `version` is newer than the stored one, and
    /// deletes the files no owner lists any more. Returns the listed hashes not stored yet, or
    /// `None` for a stale version.
    pub fn keep(
        &self,
        owner: &str,
        version: u64,
        hashes: &[Vec<u8>],
    ) -> Result<Option<Vec<Vec<u8>>>> {
        let version = i64::try_from(version).map_err(storage_error)?;
        let mut db = lock(&self.db)?;
        let tx = db.transaction().map_err(storage_error)?;
        let stored: Option<i64> = tx
            .query_row(
                "SELECT version FROM owners WHERE owner = ?",
                [owner],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage_error)?;
        if stored.is_some_and(|stored| stored >= version) {
            return Ok(None);
        }
        tx.execute(
            "INSERT OR REPLACE INTO owners (owner, version) VALUES (?, ?)",
            params![owner, version],
        )
        .map_err(storage_error)?;
        let old: Vec<Vec<u8>> = tx
            .prepare("SELECT hash FROM blobs WHERE owner = ?")
            .and_then(|mut q| q.query_map([owner], |row| row.get(0))?.collect())
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
        let mut missing = Vec::new();
        for hash in hashes {
            tx.execute(
                "INSERT OR IGNORE INTO blobs (owner, hash) VALUES (?, ?)",
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
        let mut orphans = Vec::new();
        for hash in dropped {
            let held: bool = tx
                .query_row(
                    "SELECT EXISTS (SELECT 1 FROM blobs WHERE hash = ? AND size IS NOT NULL)",
                    [&hash],
                    |row| row.get(0),
                )
                .map_err(storage_error)?;
            if !held {
                orphans.push(hash);
            }
        }
        tx.commit().map_err(storage_error)?;
        // Still under the lock, so no put of the same blob runs in between.
        for hash in orphans {
            match fs::remove_file(self.path(&hash)) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(storage_error(e)),
                _ => {}
            }
        }
        Ok(Some(missing))
    }

    /// Stores `data` for `owner` if a keep lists its hash and the owner stays within
    /// `quota` bytes. A repeated put is `Stored`.
    pub fn put(&self, owner: &str, data: &[u8], quota: u64) -> Result<Put> {
        let hash = dyapp_identity::sha256(data);
        let db = lock(&self.db)?;
        let size: Option<Option<i64>> = db
            .query_row(
                "SELECT size FROM blobs WHERE owner = ? AND hash = ?",
                params![owner, hash.as_slice()],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage_error)?;
        match size {
            None => return Ok(Put::NotListed),
            Some(Some(_)) => return Ok(Put::Stored),
            Some(None) => {}
        }
        let used: i64 = db
            .query_row(
                "SELECT COALESCE(SUM(size), 0) FROM blobs WHERE owner = ?",
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
            "UPDATE blobs SET size = ? WHERE owner = ? AND hash = ?",
            params![data.len() as i64, owner, hash.as_slice()],
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
        let used: i64 = lock(&self.db)?
            .query_row(
                "SELECT COALESCE(SUM(size), 0) FROM \
                 (SELECT MAX(size) AS size FROM blobs WHERE size IS NOT NULL GROUP BY hash)",
                [],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        Ok((used.unsigned_abs(), free(&self.dir.to_string_lossy())?))
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
        assert_eq!(media.keep("alice", 1, &both).unwrap(), Some(both.to_vec()));
        assert_eq!(media.keep("alice", 1, &[]).unwrap(), None, "stale");
        assert_eq!(media.put("alice", &a, 1000).unwrap(), Put::Stored);
        assert_eq!(media.put("alice", &a, 0).unwrap(), Put::Stored, "repeat");
        assert_eq!(media.put("alice", &b, 350).unwrap(), Put::OverQuota);
        assert_eq!(media.get(&ha).unwrap(), Some(a.clone()));
        assert_eq!(media.get(&hb).unwrap(), None);

        // Bob lists the same blob: stored once, kept while either lists it.
        assert_eq!(
            media.keep("bob", 5, std::slice::from_ref(&ha)).unwrap(),
            Some(vec![ha.clone()])
        );
        assert_eq!(media.put("bob", &a, 1000).unwrap(), Put::Stored);
        assert_eq!(media.usage().unwrap().0, 100);
        assert_eq!(
            media.keep("alice", 2, std::slice::from_ref(&hb)).unwrap(),
            Some(vec![hb])
        );
        assert_eq!(media.get(&ha).unwrap(), Some(a));
        assert_eq!(media.keep("bob", 6, &[]).unwrap(), Some(vec![]));
        assert_eq!(media.get(&ha).unwrap(), None);
        assert_eq!(media.usage().unwrap().0, 0);
    }
}
