//! Blob-file reads, assembled-cache retention and disk usage.

use super::{hex, lock, remove_file, storage_error, MediaStore};
use crate::replication::MEDIA_MAX_BYTES;
use crate::storage::free;
use crate::Result;
use rusqlite::params;
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;

fn existing_ancestor(mut path: &Path) -> Result<&Path> {
    while !path.try_exists().map_err(storage_error)? {
        path = path
            .parent()
            .ok_or_else(|| storage_error("media path has no existing ancestor"))?;
    }
    Ok(path)
}

pub(super) fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        fs::File::open(path)
            .and_then(|file| file.sync_all())
            .map_err(storage_error)?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Err(storage_error("media directory sync is unsupported"))
    }
}

fn sync_created_directories(
    dir: &Path,
    existing: &Path,
    mut sync: impl FnMut(&Path) -> Result<()>,
) -> Result<()> {
    let mut current = dir;
    loop {
        sync(current)?;
        if current == existing {
            return Ok(());
        }
        current = current
            .parent()
            .ok_or_else(|| storage_error("media directory chain escaped its ancestor"))?;
    }
}

pub(super) fn write_aside(path: &Path, data: &[u8]) -> Result<()> {
    write_aside_with(path, data, sync_directory)
}

pub(super) fn write_aside_with(
    path: &Path,
    data: &[u8],
    sync: impl FnMut(&Path) -> Result<()>,
) -> Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| storage_error("media path has no parent"))?;
    let existing = existing_ancestor(dir)?;
    fs::create_dir_all(dir).map_err(storage_error)?;
    let temp = dir.join(format!(".{}", uuid::Uuid::new_v4()));
    let mut file = fs::File::create(&temp).map_err(storage_error)?;
    file.write_all(data)
        .and_then(|()| file.sync_all())
        .and_then(|()| fs::rename(&temp, path))
        .map_err(|error| {
            let _ = fs::remove_file(&temp);
            storage_error(error)
        })?;
    sync_created_directories(dir, existing, sync)
}

pub(super) fn sql_size(value: impl TryInto<i64>) -> Result<i64> {
    value
        .try_into()
        .map_err(|_| storage_error("size exceeds signed storage range"))
}

fn read_bounded(path: &Path) -> io::Result<Vec<u8>> {
    let mut data = Vec::new();
    fs::File::open(path)?
        .take((MEDIA_MAX_BYTES as u64).saturating_add(1))
        .read_to_end(&mut data)?;
    if data.len() > MEDIA_MAX_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "media file exceeds 6 MiB",
        ));
    }
    Ok(data)
}

#[cfg(test)]
#[test]
fn sql_sizes_reject_values_above_the_signed_storage_limit() {
    assert_eq!(sql_size(0u64).unwrap(), 0);
    assert_eq!(sql_size(i64::MAX.unsigned_abs()).unwrap(), i64::MAX);
    assert!(sql_size(i64::MAX.unsigned_abs() + 1).is_err());
    assert!(sql_size(u64::MAX).is_err());
}

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
            params![hash, sql_size(data.len())?, now],
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
        match read_bounded(&self.cache.join(hex(hash))) {
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
            .prepare("SELECT `hash` FROM `cache` WHERE `read` < ?")
            .and_then(|mut q| q.query_map([before], |row| row.get(0))?.collect())
            .map_err(storage_error)?;
        for hash in &hashes {
            remove_file(&self.cache.join(hex(hash)))?;
            db.execute("DELETE FROM `cache` WHERE `hash` = ?", [hash])
                .map_err(storage_error)?;
        }
        Ok(hashes.len())
    }

    pub(super) fn reconcile_missing(&self) -> Result<()> {
        let mut db = lock(&self.db)?;
        let hashes = {
            let mut query = db
                .prepare("SELECT DISTINCT `hash` FROM `blobs` WHERE `size` IS NOT NULL")
                .map_err(storage_error)?;
            let rows = query
                .query_map([], |row| row.get::<_, Vec<u8>>(0))
                .map_err(storage_error)?
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(storage_error)?;
            rows
        };
        let mut missing = Vec::new();
        for hash in hashes {
            match fs::metadata(self.path(&hash)) {
                Ok(metadata) if metadata.is_file() => {}
                Ok(_) => return Err(storage_error("stored media path is not a file")),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => missing.push(hash),
                Err(error) => return Err(storage_error(error)),
            }
        }
        if missing.is_empty() {
            return Ok(());
        }
        let tx = db.transaction().map_err(storage_error)?;
        for hash in missing {
            tx.execute(
                "UPDATE `blobs` SET `size` = NULL, `cost` = NULL WHERE `hash` = ?",
                [hash],
            )
            .map_err(storage_error)?;
        }
        tx.commit().map_err(storage_error)
    }

    /// The whole blob file, if stored.
    ///
    /// # Errors
    ///
    /// Returns an error if the blob file cannot be read for a reason other than absence.
    pub fn get(&self, hash: &[u8]) -> Result<Option<Vec<u8>>> {
        match read_bounded(&self.path(hash)) {
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
