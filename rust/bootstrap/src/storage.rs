use crate::error::{BootstrapError, Result};
use dyapp_identity::SignedRecord;
use dyapp_profile::VerifiedProfile;
use prost::Message;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

/// One SQLite file per data type in the storage directory.
// ponytail: one connection per store behind a mutex; a reader pool if reads contend.
pub struct BootstrapStore {
    profiles: Mutex<Connection>,
    messages: Mutex<Connection>,
}

fn storage_error(error: impl ToString) -> BootstrapError {
    BootstrapError::StorageError(error.to_string())
}

fn open(path: &Path, schema: &str) -> Result<Mutex<Connection>> {
    let db = Connection::open(path).map_err(storage_error)?;
    // auto_vacuum takes effect only before the first table is created.
    db.execute_batch(&format!(
        "PRAGMA auto_vacuum = INCREMENTAL; PRAGMA journal_mode = WAL; \
         PRAGMA synchronous = NORMAL; PRAGMA journal_size_limit = {WAL_LIMIT}; {schema}"
    ))
    .map_err(storage_error)?;
    Ok(Mutex::new(db))
}

/// Bytes a WAL file is truncated to after a checkpoint.
const WAL_LIMIT: u64 = 64 * 1024 * 1024;

/// Steps a statement to the end: `incremental_vacuum` frees one page per step.
fn run_to_end(db: &Connection, sql: &str) -> Result<()> {
    let mut statement = db.prepare(sql).map_err(storage_error)?;
    let mut rows = statement.query([]).map_err(storage_error)?;
    while rows.next().map_err(storage_error)?.is_some() {}
    Ok(())
}

/// Bytes of live data in a store (pages in use) and bytes free on its file system.
fn usage(db: &Mutex<Connection>) -> Result<(u64, u64)> {
    let db = lock(db)?;
    let pragma = |name: &str| -> Result<u64> {
        db.query_row(&format!("PRAGMA {name}"), [], |row| row.get::<_, i64>(0))
            .map(|value| value.unsigned_abs())
            .map_err(storage_error)
    };
    let used = (pragma("page_count")? - pragma("freelist_count")?) * pragma("page_size")?;
    let path = std::ffi::CString::new(db.path().unwrap_or_default()).map_err(storage_error)?;
    let mut fs = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: `path` is NUL-terminated and `fs` is written by statvfs before it is read.
    if unsafe { libc::statvfs(path.as_ptr(), fs.as_mut_ptr()) } != 0 {
        return Err(storage_error(std::io::Error::last_os_error()));
    }
    let fs = unsafe { fs.assume_init() };
    // The statvfs field types differ between targets.
    #[allow(clippy::useless_conversion)]
    let free = u64::from(fs.f_bavail) * u64::from(fs.f_frsize);
    Ok((used, free))
}

fn lock(db: &Mutex<Connection>) -> Result<MutexGuard<'_, Connection>> {
    db.lock().map_err(storage_error)
}

fn decode(bytes: Vec<u8>) -> Result<SignedRecord> {
    SignedRecord::decode(bytes.as_slice())
        .map_err(|e| BootstrapError::SerializationError(e.to_string()))
}

impl BootstrapStore {
    /// Opens `profiles.db` and `messages.db` in one directory.
    pub fn new(path: &str) -> Result<Self> {
        let dir = Path::new(path);
        std::fs::create_dir_all(dir).map_err(storage_error)?;
        Self::open(&dir.join("profiles.db"), &dir.join("messages.db"))
    }

    pub fn open(profiles: &Path, messages: &Path) -> Result<Self> {
        Ok(Self {
            profiles: open(
                profiles,
                "CREATE TABLE IF NOT EXISTS profiles (
                     peer_id TEXT PRIMARY KEY,
                     record BLOB NOT NULL,
                     live INTEGER NOT NULL
                 );",
            )?,
            messages: open(
                messages,
                // `seq` keeps arrival order; `size` is the stored record length.
                "CREATE TABLE IF NOT EXISTS envelopes (
                     seq INTEGER PRIMARY KEY,
                     mailbox BLOB NOT NULL,
                     id BLOB NOT NULL,
                     record BLOB NOT NULL,
                     size INTEGER NOT NULL,
                     expires_at INTEGER NOT NULL,
                     UNIQUE (mailbox, id)
                 );
                 CREATE INDEX IF NOT EXISTS envelopes_mailbox ON envelopes (mailbox, seq);
                 CREATE INDEX IF NOT EXISTS envelopes_expiry ON envelopes (expires_at);",
            )?,
        })
    }

    /// Stores a signed envelope once per (mailbox, id): a repeated put is a no-op and returns
    /// true. Returns false, storing nothing, when the record would take the mailbox over
    /// `max_mailbox_bytes`.
    pub fn put_envelope(
        &self,
        mailbox: &[u8],
        id: &[u8],
        record: &SignedRecord,
        expires_at: i64,
        max_mailbox_bytes: u64,
    ) -> Result<bool> {
        let record = record.encode_to_vec();
        // The connection lock serialises the size check and the insert.
        let db = lock(&self.messages)?;
        let stored: bool = db
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM envelopes WHERE mailbox = ? AND id = ?)",
                params![mailbox, id],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        if stored {
            return Ok(true);
        }
        // ponytail: sums the mailbox on every put; keep a per-mailbox total if puts get slow.
        let used: i64 = db
            .query_row(
                "SELECT COALESCE(SUM(size), 0) FROM envelopes WHERE mailbox = ?",
                [mailbox],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        let size = i64::try_from(record.len()).map_err(storage_error)?;
        if u64::try_from(used + size).map_err(storage_error)? > max_mailbox_bytes {
            return Ok(false);
        }
        db.execute(
            "INSERT INTO envelopes (mailbox, id, record, size, expires_at) VALUES (?, ?, ?, ?, ?)",
            params![mailbox, id, record, size, expires_at],
        )
        .map_err(storage_error)?;
        Ok(true)
    }

    /// The oldest envelopes of a mailbox: at most `limit`, and no more than `max_bytes` of
    /// records unless the first alone is larger. The flag says more are waiting.
    pub fn fetch_envelopes(
        &self,
        mailbox: &[u8],
        limit: u32,
        max_bytes: u64,
    ) -> Result<(Vec<SignedRecord>, bool)> {
        let db = lock(&self.messages)?;
        let mut query = db
            .prepare("SELECT record FROM envelopes WHERE mailbox = ? ORDER BY seq LIMIT ?")
            .map_err(storage_error)?;
        let rows: Vec<Vec<u8>> = query
            .query_map(params![mailbox, limit + 1], |row| row.get(0))
            .and_then(Iterator::collect)
            .map_err(storage_error)?;
        let total = rows.len();
        let mut bytes = 0;
        let mut records = Vec::new();
        for row in rows.into_iter().take(limit as usize) {
            bytes += row.len() as u64;
            if bytes > max_bytes && !records.is_empty() {
                break;
            }
            records.push(decode(row)?);
        }
        let more = total > records.len();
        Ok((records, more))
    }

    /// Deletes envelopes from a mailbox; unknown ids are ignored.
    pub fn ack_envelopes(&self, mailbox: &[u8], ids: &[Vec<u8>]) -> Result<()> {
        let mut db = lock(&self.messages)?;
        let tx = db.transaction().map_err(storage_error)?;
        for id in ids {
            tx.execute(
                "DELETE FROM envelopes WHERE mailbox = ? AND id = ?",
                params![mailbox, id],
            )
            .map_err(storage_error)?;
        }
        tx.commit().map_err(storage_error)
    }

    /// Verifies a signed profile and stores it if it is newer than the owner's stored version.
    /// A tombstone replaces the profile and is kept so older versions cannot be re-imported.
    pub fn put_profile(&self, record: &SignedRecord) -> Result<VerifiedProfile> {
        let verified = dyapp_profile::verify(record)?;
        // The connection lock serialises the read-compare-write of profile versions.
        let db = lock(&self.profiles)?;
        let current = Self::profile(&db, &verified.peer_id)?
            .map(|stored| dyapp_profile::stored_version(&stored))
            .transpose()?;
        verified.check_newer(current)?;
        db.execute(
            "INSERT OR REPLACE INTO profiles (peer_id, record, live) VALUES (?, ?, ?)",
            params![
                verified.peer_id,
                record.encode_to_vec(),
                !verified.profile.deleted
            ],
        )
        .map_err(storage_error)?;
        Ok(verified)
    }

    fn profile(db: &Connection, peer_id: &str) -> Result<Option<SignedRecord>> {
        db.query_row(
            "SELECT record FROM profiles WHERE peer_id = ?",
            [peer_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)?
        .map(decode)
        .transpose()
    }

    pub fn get_profile(&self, peer_id: &str) -> Result<Option<SignedRecord>> {
        Self::profile(&*lock(&self.profiles)?, peer_id)
    }

    /// Live profiles in key order; tombstones are skipped.
    pub fn list_profiles(&self, skip: u32, limit: u32) -> Result<Vec<SignedRecord>> {
        let db = lock(&self.profiles)?;
        let mut query = db
            .prepare("SELECT record FROM profiles WHERE live ORDER BY peer_id LIMIT ? OFFSET ?")
            .map_err(storage_error)?;
        let records: Vec<Vec<u8>> = query
            .query_map([limit, skip], |row| row.get(0))
            .and_then(Iterator::collect)
            .map_err(storage_error)?;
        records.into_iter().map(decode).collect()
    }

    pub fn cleanup_expired(&self, now: i64) -> Result<usize> {
        lock(&self.messages)?
            .execute("DELETE FROM envelopes WHERE expires_at <= ?", [now])
            .map_err(storage_error)
    }

    /// For each store: frees at most `pages` unused pages, checkpoints and truncates the WAL,
    /// refreshes planner statistics. Holds one store's lock at a time, so a run stays short.
    /// Returns the free pages left in `profiles.db` and `messages.db`.
    pub fn maintain(&self, pages: u32) -> Result<[i64; 2]> {
        // incremental_vacuum(0) would free the whole freelist in one go.
        let pages = pages.max(1);
        let mut left = [0; 2];
        for (db, left) in [&self.profiles, &self.messages].into_iter().zip(&mut left) {
            let db = lock(db)?;
            run_to_end(&db, &format!("PRAGMA incremental_vacuum({pages})"))?;
            run_to_end(&db, "PRAGMA wal_checkpoint(TRUNCATE)")?;
            run_to_end(&db, "PRAGMA optimize")?;
            *left = db
                .query_row("PRAGMA freelist_count", [], |row| row.get(0))
                .map_err(storage_error)?;
        }
        Ok(left)
    }

    pub fn profiles_usage(&self) -> Result<(u64, u64)> {
        usage(&self.profiles)
    }

    pub fn messages_usage(&self) -> Result<(u64, u64)> {
        usage(&self.messages)
    }

    /// Takes and releases the write lock of every store.
    pub fn health_check(&self) -> Result<()> {
        for db in [&self.profiles, &self.messages] {
            lock(db)?
                .execute_batch("BEGIN IMMEDIATE; ROLLBACK;")
                .map_err(storage_error)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dyapp_identity::Identity;
    use dyapp_profile::Profile;
    use uuid::Uuid;

    fn temp_store() -> BootstrapStore {
        BootstrapStore::new(&format!("/tmp/ai/test-bootstrap-{}", Uuid::new_v4())).unwrap()
    }

    #[test]
    fn maintenance_frees_pages_in_bounded_steps() {
        let dir = format!("/tmp/ai/test-bootstrap-{}", Uuid::new_v4());
        let store = BootstrapStore::new(&dir).unwrap();
        let record = SignedRecord {
            payload: vec![7; 10_000],
            ..SignedRecord::default()
        };
        for id in 0..500u32 {
            store
                .put_envelope(&[1; 32], &id.to_be_bytes(), &record, 0, u64::MAX)
                .unwrap();
        }
        assert_eq!(store.cleanup_expired(1).unwrap(), 500);
        store.maintain(1).unwrap();
        let size = || {
            std::fs::metadata(format!("{dir}/messages.db"))
                .unwrap()
                .len()
        };
        let before = size();
        let [_, free] = store.maintain(10).unwrap();
        let [_, after] = store.maintain(10).unwrap();
        assert!(free > 1000);
        assert_eq!(after, free - 10);
        let (used, free) = store.messages_usage().unwrap();
        assert!(used < 100_000 && free > 0, "{used} {free}");
        assert_eq!(store.maintain(u32::MAX).unwrap()[1], 0);
        assert!(size() < before / 10, "file did not shrink");
        assert_eq!(
            std::fs::metadata(format!("{dir}/messages.db-wal"))
                .unwrap()
                .len(),
            0
        );
    }

    #[test]
    fn envelopes_once_per_id_limits_and_expiry() {
        let store = temp_store();
        let record = |n: u8| SignedRecord {
            payload: vec![n; 100],
            ..SignedRecord::default()
        };
        let size = record(0).encode_to_vec().len() as u64;
        let (a, b) = ([1u8; 32], [2u8; 32]);

        assert!(store
            .put_envelope(&a, &[1], &record(1), 2000, 2 * size)
            .unwrap());
        assert!(store
            .put_envelope(&a, &[1], &record(9), 2000, 2 * size)
            .unwrap());
        assert!(store
            .put_envelope(&a, &[2], &record(2), 3000, 2 * size)
            .unwrap());
        // Full, but a repeated put still succeeds.
        assert!(!store
            .put_envelope(&a, &[3], &record(3), 2000, 2 * size)
            .unwrap());
        assert!(store
            .put_envelope(&a, &[2], &record(2), 2000, 2 * size)
            .unwrap());
        assert!(store
            .put_envelope(&b, &[1], &record(4), 2000, size)
            .unwrap());

        let (all, more) = store.fetch_envelopes(&a, 10, u64::MAX).unwrap();
        assert_eq!((all, more), (vec![record(1), record(2)], false));
        assert_eq!(
            store.fetch_envelopes(&a, 1, u64::MAX).unwrap(),
            (vec![record(1)], true)
        );
        assert_eq!(
            store.fetch_envelopes(&a, 10, 1).unwrap(),
            (vec![record(1)], true)
        );

        store.ack_envelopes(&a, &[vec![1], vec![7]]).unwrap();
        assert_eq!(
            store.fetch_envelopes(&a, 10, u64::MAX).unwrap().0,
            vec![record(2)]
        );
        assert_eq!(store.cleanup_expired(2000).unwrap(), 1);
        assert_eq!(
            store.fetch_envelopes(&a, 10, u64::MAX).unwrap().0,
            vec![record(2)]
        );
        assert!(store
            .fetch_envelopes(&b, 10, u64::MAX)
            .unwrap()
            .0
            .is_empty());
    }

    #[test]
    fn signed_profile_versions_and_tombstone() {
        let store = temp_store();
        let owner = Identity::generate();
        let profile = |version| Profile {
            version,
            age: 35,
            ..Profile::default()
        };

        store.put_profile(&profile(2).sign(&owner)).unwrap();
        assert!(matches!(
            store.put_profile(&profile(1).sign(&owner)),
            Err(BootstrapError::Profile(dyapp_profile::Error::Stale))
        ));
        let mut forged = profile(3).sign(&owner);
        forged.payload[1] ^= 1;
        assert!(store.put_profile(&forged).is_err());

        let stored = store.get_profile(&owner.peer_id()).unwrap().unwrap();
        assert_eq!(dyapp_profile::stored_version(&stored), Ok(2));
        assert_eq!(store.list_profiles(0, 10).unwrap().len(), 1);
        assert!(store.list_profiles(1, 10).unwrap().is_empty());

        store
            .put_profile(&Profile::tombstone(3).sign(&owner))
            .unwrap();
        assert!(store.list_profiles(0, 10).unwrap().is_empty());
        assert!(store.put_profile(&profile(2).sign(&owner)).is_err());
    }

    #[test]
    fn data_survives_reopen() {
        let path = format!("/tmp/ai/test-bootstrap-{}", Uuid::new_v4());
        let owner = Identity::generate();
        BootstrapStore::new(&path)
            .unwrap()
            .put_profile(
                &Profile {
                    version: 1,
                    age: 35,
                    ..Profile::default()
                }
                .sign(&owner),
            )
            .unwrap();
        let store = BootstrapStore::new(&path).unwrap();
        assert!(store.get_profile(&owner.peer_id()).unwrap().is_some());
    }

    #[test]
    fn test_health_check() {
        assert!(temp_store().health_check().is_ok());
    }
}
