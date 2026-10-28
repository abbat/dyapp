//! Profile and mailbox stores, and the SQLite helpers shared by every store.

use crate::error::{BootstrapError, Result};
use dyapp_identity::SignedRecord;
use dyapp_profile::VerifiedProfile;
use prost::Message;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

#[path = "storage/envelopes.rs"]
mod envelopes;
pub use envelopes::{NewEnvelope, Put};
#[path = "storage/keypackages.rs"]
mod keypackages;
#[path = "storage/revocations.rs"]
mod revocations;
pub use revocations::MAX_REVOKED;

/// One SQLite file per data type in the storage directory.
// ponytail: one connection per store behind a mutex; a reader pool if reads contend.
pub struct BootstrapStore {
    profiles: Mutex<Connection>,
    messages: Mutex<Connection>,
}

pub(crate) fn storage_error(error: impl ToString) -> BootstrapError {
    BootstrapError::StorageError(error.to_string())
}

/// Opens a store file and creates `schema`. The format has no versions before the first release:
/// a file with a nonzero `PRAGMA user_version` is from a newer dyappd and is refused.
pub(crate) fn open(path: &Path, schema: &[&str]) -> Result<Mutex<Connection>> {
    let db = Connection::open(path).map_err(storage_error)?;
    let version: i64 = db
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(storage_error)?;
    if version != 0 {
        return Err(storage_error(format!(
            "{} has store format {version}, this dyappd knows up to 0; run a newer dyappd",
            path.display()
        )));
    }
    // auto_vacuum takes effect only before the first table is created.
    db.execute_batch(&format!(
        "PRAGMA auto_vacuum = INCREMENTAL; PRAGMA journal_mode = WAL; \
         PRAGMA synchronous = NORMAL; PRAGMA journal_size_limit = {WAL_LIMIT}; {}",
        schema.concat()
    ))
    .map_err(storage_error)?;
    Ok(Mutex::new(db))
}

/// `last_seen`: node time of a newer publish, or the latest validated heartbeat time.
const PROFILES: &str = "CREATE TABLE IF NOT EXISTS `profiles` (
         `peer_id` TEXT PRIMARY KEY,
         `record` BLOB NOT NULL,
         `live` INTEGER NOT NULL,
         `last_seen` INTEGER NOT NULL
     );
     CREATE INDEX IF NOT EXISTS `profiles_seen` ON `profiles` (`last_seen`);";

/// `seq` keeps arrival order; `size` is the stored record length; `class` is 1 for a signal, 0
/// for a message. The covering class index serves the per-class sums, the signal victim choice
/// and the mailbox lookups.
const ENVELOPES: &str = "CREATE TABLE IF NOT EXISTS `envelopes` (
         `seq` INTEGER PRIMARY KEY,
         `mailbox` BLOB NOT NULL,
         `id` BLOB NOT NULL,
         `record` BLOB NOT NULL,
         `size` INTEGER NOT NULL,
         `expires_at` INTEGER NOT NULL,
         `class` INTEGER NOT NULL DEFAULT 0,
         `token_key` BLOB,
         `via` BLOB NOT NULL DEFAULT x'',
         `source_group` BLOB NOT NULL DEFAULT x'',
         UNIQUE (`mailbox`, `id`)
     );
     CREATE INDEX IF NOT EXISTS `envelopes_expiry` ON `envelopes` (`expires_at`);
     CREATE INDEX IF NOT EXISTS `envelopes_class` ON `envelopes` (`mailbox`, `class`, `seq`, `size`);
";

const PROFILES_AFTER: &str =
    "SELECT `record` FROM `profiles` WHERE `live` AND `peer_id` > ? ORDER BY `peer_id` LIMIT ?";
const MAILBOXES_AFTER: &str = "SELECT DISTINCT `mailbox` FROM `envelopes` \
     WHERE `mailbox` > ? AND `expires_at` > ? ORDER BY `mailbox` LIMIT ?";
/// Bytes a WAL file is truncated to after a checkpoint.
const WAL_LIMIT: u64 = 64 * 1024 * 1024;

/// Steps a statement to the end: `incremental_vacuum` frees one page per step.
fn run_to_end(db: &Connection, sql: &str) -> Result<()> {
    let mut statement = db.prepare(sql).map_err(storage_error)?;
    let mut rows = statement.query([]).map_err(storage_error)?;
    while rows.next().map_err(storage_error)?.is_some() {}
    Ok(())
}

/// Frees at most `pages` unused pages, checkpoints and truncates the WAL, refreshes planner
/// statistics. Returns the free pages left.
pub(crate) fn maintain(db: &Connection, pages: u32) -> Result<i64> {
    // incremental_vacuum(0) would free the whole freelist in one go.
    run_to_end(db, &format!("PRAGMA incremental_vacuum({})", pages.max(1)))?;
    run_to_end(db, "PRAGMA wal_checkpoint(TRUNCATE)")?;
    run_to_end(db, "PRAGMA optimize")?;
    db.query_row("PRAGMA freelist_count", [], |row| row.get(0))
        .map_err(storage_error)
}

/// Bytes of live data in a store: pages in use.
pub(crate) fn used(db: &Connection) -> Result<u64> {
    let pragma = |name: &str| -> Result<u64> {
        db.query_row(&format!("PRAGMA {name}"), [], |row| row.get::<_, i64>(0))
            .map(|value| value.unsigned_abs())
            .map_err(storage_error)
    };
    Ok((pragma("page_count")? - pragma("freelist_count")?) * pragma("page_size")?)
}

/// Rows deleted per eviction step and steps per call, so one call holds the lock briefly.
const EVICT_ROWS: u32 = 64;
const EVICT_STEPS: u32 = 64;

/// Deletes the rows of `table` with the lowest rowid, step by step, until the store holds less
/// than `target` bytes or the step budget runs out. Returns the rows deleted.
fn evict(db: &Mutex<Connection>, table: &'static str, target: u64) -> Result<usize> {
    let db = lock(db)?;
    let mut evicted = 0;
    for _ in 0..EVICT_STEPS {
        if used(&db)? < target {
            break;
        }
        let deleted = db
            .execute(
                &format!(
                    "DELETE FROM `{table}` WHERE `rowid` IN \
                     (SELECT `rowid` FROM `{table}` ORDER BY `rowid` LIMIT {EVICT_ROWS})"
                ),
                [],
            )
            .map_err(storage_error)?;
        if deleted == 0 {
            break;
        }
        evicted += deleted;
    }
    Ok(evicted)
}

/// Bytes of live data in a store and bytes free on its file system.
fn usage(db: &Mutex<Connection>) -> Result<(u64, u64)> {
    let db = lock(db)?;
    Ok((used(&db)?, free(db.path().unwrap_or_default())?))
}

/// Bytes free for this user on the file system of `path`.
pub(crate) fn free(path: &str) -> Result<u64> {
    let path = std::ffi::CString::new(path).map_err(storage_error)?;
    let mut fs = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: `path` is NUL-terminated and `fs` is written by statvfs before it is read.
    if unsafe { libc::statvfs(path.as_ptr(), fs.as_mut_ptr()) } != 0 {
        return Err(storage_error(std::io::Error::last_os_error()));
    }
    // SAFETY: statvfs returned 0, so it initialised `fs`.
    let fs = unsafe { fs.assume_init() };
    // The statvfs field types differ between targets.
    #[expect(
        clippy::useless_conversion,
        reason = "statvfs field types differ between targets"
    )]
    Ok(u64::from(fs.f_bavail) * u64::from(fs.f_frsize))
}

/// Panics if `EXPLAIN QUERY PLAN` shows a step that reads a table without an index.
#[cfg(test)]
pub(crate) fn assert_indexed(db: &Connection, sql: &str) {
    let mut query = db.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
    let count = query.parameter_count();
    let steps: Vec<String> = query
        .query_map(rusqlite::params_from_iter(vec![0; count]), |row| row.get(3))
        .and_then(Iterator::collect)
        .unwrap();
    assert!(
        steps
            .iter()
            .all(|step| !step.starts_with("SCAN") || step.contains(" USING ")),
        "{steps:?}"
    );
}

pub(crate) fn lock<T>(db: &Mutex<T>) -> Result<MutexGuard<'_, T>> {
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

    /// Opens or creates the profile and mailbox databases.
    pub fn open(profiles: &Path, messages: &Path) -> Result<Self> {
        Ok(Self {
            profiles: open(profiles, &[PROFILES, keypackages::SCHEMA])?,
            messages: open(
                messages,
                &[ENVELOPES, envelopes::SCHEMA, revocations::SCHEMA],
            )?,
        })
    }

    /// The envelopes of a mailbox, messages first, then signals, oldest first in each: at most
    /// `limit`, and no more than `max_bytes` of records unless the first alone is larger. The flag says more are waiting.
    pub fn fetch_envelopes(
        &self,
        mailbox: &[u8],
        limit: u32,
        max_bytes: u64,
    ) -> Result<(Vec<SignedRecord>, bool)> {
        let db = lock(&self.messages)?;
        let mut query = db
            .prepare("SELECT `record` FROM `envelopes` WHERE `mailbox` = ? ORDER BY `class`, `seq` LIMIT ?")
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

    /// The ids of the first `limit` envelopes of a mailbox in fetch order, without reading the
    /// records.
    pub fn envelope_ids(&self, mailbox: &[u8], limit: u32) -> Result<Vec<Vec<u8>>> {
        let db = lock(&self.messages)?;
        let mut query = db
            .prepare(
                "SELECT `id` FROM `envelopes` WHERE `mailbox` = ? ORDER BY `class`, `seq` LIMIT ?",
            )
            .map_err(storage_error)?;
        query
            .query_map(params![mailbox, limit], |row| row.get(0))
            .and_then(Iterator::collect)
            .map_err(storage_error)
    }

    /// Deletes envelopes from a mailbox; unknown ids are ignored.
    pub fn ack_envelopes(&self, mailbox: &[u8], ids: &[Vec<u8>]) -> Result<()> {
        let mut db = lock(&self.messages)?;
        let tx = db.transaction().map_err(storage_error)?;
        for id in ids {
            tx.execute(
                "DELETE FROM `envelopes` WHERE `mailbox` = ? AND `id` = ?",
                params![mailbox, id],
            )
            .map_err(storage_error)?;
        }
        tx.commit().map_err(storage_error)
    }

    /// Verifies a signed profile and stores it if it is newer than the owner's stored version.
    /// A tombstone replaces the profile and is kept so older versions cannot be re-imported.
    pub fn put_profile(&self, record: &SignedRecord) -> Result<VerifiedProfile> {
        self.put_profile_seen(record, chrono::Utc::now().timestamp())
    }

    /// [`Self::put_profile`] with the owner last seen at `last_seen`; a later stored time stays.
    pub fn put_profile_seen(
        &self,
        record: &SignedRecord,
        last_seen: i64,
    ) -> Result<VerifiedProfile> {
        let verified = dyapp_profile::verify(record)?;
        // The connection lock serialises the read-compare-write of profile versions.
        let db = lock(&self.profiles)?;
        let current = Self::profile(&db, &verified.peer_id)?
            .map(|stored| dyapp_profile::stored_version(&stored))
            .transpose()?;
        verified.check_newer(current)?;
        db.execute(
            // REPLACE gives a new rowid, moving the owner to the end of the eviction order.
            "INSERT OR REPLACE INTO `profiles` (`peer_id`, `record`, `live`, `last_seen`) \
             VALUES (?1, ?2, ?3, MAX(?4, COALESCE((SELECT `last_seen` FROM `profiles` \
             WHERE `peer_id` = ?1), ?4)))",
            params![
                verified.peer_id,
                record.encode_to_vec(),
                !verified.profile.deleted,
                last_seen
            ],
        )
        .map_err(storage_error)?;
        Ok(verified)
    }

    fn profile(db: &Connection, peer_id: &str) -> Result<Option<SignedRecord>> {
        db.query_row(
            "SELECT `record` FROM `profiles` WHERE `peer_id` = ?",
            [peer_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)?
        .map(decode)
        .transpose()
    }

    /// When the owner of `peer_id`'s stored profile was last seen.
    pub fn profile_last_seen(&self, peer_id: &str) -> Result<Option<i64>> {
        lock(&self.profiles)?
            .query_row(
                "SELECT `last_seen` FROM `profiles` WHERE `peer_id` = ?",
                [peer_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage_error)
    }

    /// The stored profile record of `peer_id`.
    pub fn get_profile(&self, peer_id: &str) -> Result<Option<SignedRecord>> {
        Self::profile(&*lock(&self.profiles)?, peer_id)
    }

    /// Live profiles in key order; tombstones are skipped.
    pub fn list_profiles(&self, skip: u32, limit: u32) -> Result<Vec<SignedRecord>> {
        let db = lock(&self.profiles)?;
        let mut query = db
            .prepare(
                "SELECT `record` FROM `profiles` WHERE `live` ORDER BY `peer_id` LIMIT ? OFFSET ?",
            )
            .map_err(storage_error)?;
        let records: Vec<Vec<u8>> = query
            .query_map([limit, skip], |row| row.get(0))
            .and_then(Iterator::collect)
            .map_err(storage_error)?;
        records.into_iter().map(decode).collect()
    }

    /// Live profiles with a peer ID after `after`, in peer ID order; tombstones are skipped.
    pub fn profiles_after(&self, after: &str, limit: u32) -> Result<Vec<SignedRecord>> {
        let db = lock(&self.profiles)?;
        let mut query = db.prepare(PROFILES_AFTER).map_err(storage_error)?;
        let records: Vec<Vec<u8>> = query
            .query_map(params![after, limit], |row| row.get(0))
            .and_then(Iterator::collect)
            .map_err(storage_error)?;
        records.into_iter().map(decode).collect()
    }

    /// Mailboxes after `after` that hold an unexpired envelope, in mailbox order.
    pub fn mailboxes_after(&self, after: &[u8], limit: u32) -> Result<Vec<Vec<u8>>> {
        let db = lock(&self.messages)?;
        let mut query = db.prepare(MAILBOXES_AFTER).map_err(storage_error)?;
        query
            .query_map(
                params![after, chrono::Utc::now().timestamp(), limit],
                |row| row.get(0),
            )
            .and_then(Iterator::collect)
            .map_err(storage_error)
    }

    /// Advances liveness to the validated, signed heartbeat time; false if no profile is stored.
    pub fn touch_profile(&self, peer_id: &str, now: i64) -> Result<bool> {
        let touched = lock(&self.profiles)?
            .execute(
                "UPDATE `profiles` SET `last_seen` = MAX(`last_seen`, ?) WHERE `peer_id` = ?",
                params![now, peer_id],
            )
            .map_err(storage_error)?;
        Ok(touched > 0)
    }

    /// Deletes profiles and tombstones whose owner was last seen at `cutoff` or before.
    pub fn expire_profiles(&self, cutoff: i64) -> Result<usize> {
        lock(&self.profiles)?
            .execute("DELETE FROM `profiles` WHERE `last_seen` <= ?", [cutoff])
            .map_err(storage_error)
    }

    /// Deletes envelopes, revoked keys and revokes expired at `now` (Unix seconds); returns how
    /// many envelopes.
    pub fn cleanup_expired(&self, now: i64) -> Result<usize> {
        let db = lock(&self.messages)?;
        for table in ["revoked", "revokes"] {
            db.execute(
                &format!("DELETE FROM `{table}` WHERE `expires_at` <= ?"),
                [now],
            )
            .map_err(storage_error)?;
        }
        db.execute("DELETE FROM `envelopes` WHERE `expires_at` <= ?", [now])
            .map_err(storage_error)
    }

    /// Runs the free `maintain` on each store, holding one store's lock at a time so a run stays short.
    /// Returns the free pages left in `profiles.db` and `messages.db`.
    pub fn maintain(&self, pages: u32) -> Result<[i64; 2]> {
        Ok([
            maintain(&*lock(&self.profiles)?, pages)?,
            maintain(&*lock(&self.messages)?, pages)?,
        ])
    }

    /// Bytes used by `profiles.db`, and bytes free on its file system.
    pub fn profiles_usage(&self) -> Result<(u64, u64)> {
        usage(&self.profiles)
    }

    /// Bytes used by `messages.db`, and bytes free on its file system.
    pub fn messages_usage(&self) -> Result<(u64, u64)> {
        usage(&self.messages)
    }

    /// Deletes the profiles published longest ago until `profiles.db` holds less than `target`
    /// bytes: `INSERT OR REPLACE` gives a republished profile the highest rowid, so rowid order
    /// is publish order. Bounded per call; returns the profiles deleted.
    pub fn evict_profiles(&self, target: u64) -> Result<usize> {
        evict(&self.profiles, "profiles", target)
    }

    /// Deletes the oldest envelopes (arrival order) until `messages.db` holds less than `target`
    /// bytes. A device that fetches and acks keeps its mailbox young, so the oldest envelopes are
    /// mostly those of mailboxes inactive longest.
    // ponytail: arrival order, not per-mailbox activity; a spam flood ages out undelivered mail
    // of inactive users first (ADR 0009 accepts this; spam defences are ADR 0008).
    pub fn evict_envelopes(&self, target: u64) -> Result<usize> {
        evict(&self.messages, "envelopes", target)
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

    mod records;

    pub(super) fn temp_store() -> BootstrapStore {
        BootstrapStore::new(&format!("/tmp/ai/test-bootstrap-{}", Uuid::new_v4())).unwrap()
    }

    impl BootstrapStore {
        /// A tokenless put without a pool: true when stored.
        pub(super) fn put_plain(
            &self,
            mailbox: &[u8],
            id: &[u8],
            record: &SignedRecord,
            signal: bool,
            expires_at: i64,
            max_bytes: u64,
        ) -> Result<bool> {
            let envelope = NewEnvelope::new(mailbox, id, record, signal, expires_at);
            Ok(self.put_envelope(&envelope, max_bytes, None)? == Put::Stored)
        }
    }

    #[test]
    fn profiles_expire_after_the_owner_was_last_seen() {
        let dir = format!("/tmp/ai/test-bootstrap-{}", Uuid::new_v4());
        std::fs::create_dir_all(&dir).unwrap();
        let store = BootstrapStore::new(&dir).unwrap();
        let now = chrono::Utc::now().timestamp();
        let owner = Identity::generate();
        let profile = Profile {
            version: 1,
            ..Profile::default()
        };
        store.put_profile(&profile.sign(&owner)).unwrap();
        assert_eq!(store.expire_profiles(now - 60).unwrap(), 0);
        assert!(store.touch_profile(&owner.peer_id(), now + 100).unwrap());
        assert!(!store.touch_profile("unknown", now).unwrap());
        assert_eq!(store.expire_profiles(now + 50).unwrap(), 0);
        assert!(store.get_profile(&owner.peer_id()).unwrap().is_some());
        assert_eq!(store.expire_profiles(now + 100).unwrap(), 1);
    }

    #[test]
    fn a_newer_store_file_is_refused() {
        let dir = format!("/tmp/ai/test-bootstrap-{}", Uuid::new_v4());
        drop(BootstrapStore::new(&dir).unwrap());
        drop(BootstrapStore::new(&dir).unwrap());
        Connection::open(format!("{dir}/profiles.db"))
            .unwrap()
            .pragma_update(None, "user_version", 1)
            .unwrap();
        let error = BootstrapStore::new(&dir).err().unwrap().to_string();
        assert!(
            error.contains("profiles.db") && error.contains("format 1"),
            "{error}"
        );
    }

    #[test]
    fn the_envelope_lookups_use_the_class_index() {
        let dir = format!("/tmp/ai/test-bootstrap-{}", Uuid::new_v4());
        let store = BootstrapStore::new(&dir).unwrap();
        let db = lock(&store.messages).unwrap();
        let plan = |sql: &str| -> String {
            db.prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
                .unwrap()
                .query_map([], |row| row.get::<_, String>(3))
                .unwrap()
                .collect::<std::result::Result<Vec<_>, _>>()
                .unwrap()
                .join("\n")
        };
        let sql = "SELECT COALESCE(SUM(`size`), 0) FROM `envelopes` WHERE `mailbox` = x'01' AND `class` = 0";
        let sum = plan(sql);
        assert!(
            sum.contains("USING COVERING INDEX envelopes_class"),
            "{sql}: {sum}"
        );
        for sql in [
            "SELECT `record` FROM `envelopes` WHERE `mailbox` = x'01' ORDER BY `class`, `seq` LIMIT 5",
            "SELECT `id` FROM `envelopes` WHERE `mailbox` = x'01' ORDER BY `class`, `seq` LIMIT 5",
            "DELETE FROM `envelopes` WHERE `mailbox` = x'01' AND `id` = x'02'",
        ] {
            let plan = plan(sql);
            assert!(
                !plan.contains("SCAN") && !plan.contains("TEMP B-TREE"),
                "{sql}: {plan}"
            );
        }
    }
}
