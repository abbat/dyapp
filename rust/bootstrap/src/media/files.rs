//! Blob-file reads, assembled-cache retention and disk usage.

use super::{hex, lock, remove_file, storage_error, write_aside, MediaStore};
use crate::storage::free;
use crate::Result;
use rusqlite::params;
use std::fs;

impl MediaStore {
    /// Keeps an assembled blob for the next pieces; `now` in Unix seconds.
    ///
    /// # Errors
    ///
    /// Returns an error if the database lock, cache-file write or cache-metadata update fails.
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
    ///
    /// # Errors
    ///
    /// Returns an error if the database lock, cache-metadata update or file read fails.
    /// A missing cache file returns `None` after its stale metadata is removed.
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
    ///
    /// # Errors
    ///
    /// Returns an error if the database lock, expired-cache deletion or cache-file removal fails.
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

    /// The whole blob file, if stored.
    ///
    /// # Errors
    ///
    /// Returns an error if the blob file cannot be read for a reason other than absence.
    pub fn get(&self, hash: &[u8]) -> Result<Option<Vec<u8>>> {
        match fs::read(self.path(hash)) {
            Ok(data) => Ok(Some(data)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(storage_error(e)),
        }
    }

    /// Bytes of distinct stored blobs, and bytes free on their file system.
    ///
    /// # Errors
    ///
    /// Returns an error if database locking, media-usage queries or the filesystem-space query fails.
    pub fn usage(&self) -> Result<(u64, u64)> {
        Ok((
            Self::used(&*lock(&self.db)?)?,
            free(&self.dir.to_string_lossy())?,
        ))
    }
}
