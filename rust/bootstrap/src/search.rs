//! The search role's index, `profiles-idx.db`: typed profile rows, interests, the signed records
//! and the per-key limit counters. Disposable: catch-up refills it from other search nodes.

use crate::error::Result;
use crate::storage::{self, lock, storage_error, Step};
use dyapp_identity::SignedRecord;
use dyapp_profile::Profile;
use prost::Message;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

pub const SEARCH_TOPICS: u32 = 16;
pub const INDEX_VERSION: u32 = 1;
pub const UPDATE_SECONDS: i64 = 600;
pub const HEARTBEATS_PER_DAY: i64 = 2;

/// Keys deleted per eviction or expiry batch, and batches per `expire` call.
const BATCH: u32 = 64;
const EXPIRE_BATCHES: u32 = 64;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS `profile` (`key` BLOB PRIMARY KEY, `topic` INTEGER NOT NULL,
  `version` INTEGER NOT NULL, `age` INTEGER, `country` TEXT, `income_from` INTEGER,
  `income_to` INTEGER, `has_kids` INTEGER, `wants_more_kids` INTEGER, `relationship_goal` TEXT,
  `career_ambition` TEXT, `education` TEXT, `orientation` TEXT, `role_preference` TEXT,
  `zodiac` TEXT, `financial_philosophy` TEXT, `employment_status` TEXT, `fitness_level` TEXT,
  `place` TEXT, `r` INTEGER NOT NULL) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS `profile_place` ON `profile` (`country`, `place`, `r`);
CREATE INDEX IF NOT EXISTS `profile_country` ON `profile` (`country`, `r`);
CREATE INDEX IF NOT EXISTS `profile_r` ON `profile` (`r`);
CREATE TABLE IF NOT EXISTS `interest` (`key` BLOB, `interest` TEXT,
  PRIMARY KEY (`key`, `interest`)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS `record` (`key` BLOB PRIMARY KEY, `record` BLOB NOT NULL) WITHOUT ROWID;
-- Every known key, tombstones too: version, limits and heartbeat for expiry and eviction.
CREATE TABLE IF NOT EXISTS `key` (`key` BLOB PRIMARY KEY, `topic` INTEGER NOT NULL,
  `version` INTEGER NOT NULL, `deleted` INTEGER NOT NULL, `updated` INTEGER NOT NULL,
  `heartbeat` INTEGER NOT NULL, `beat_day` INTEGER NOT NULL, `beats` INTEGER NOT NULL)
  WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS `key_heartbeat` ON `key` (`heartbeat`);
CREATE INDEX IF NOT EXISTS `key_topic` ON `key` (`topic`, `key`);
";
const STEPS: &[Step] = &[];

/// The gossip topic of a key: H(key) mod 16, where the key is already a hash.
pub fn topic(key: &[u8; 32]) -> u32 {
    u32::from(key[0] % 16)
}

#[derive(Debug, PartialEq, Eq)]
pub enum Put {
    Stored,
    Stale,
    Limited { retry_after: u64 },
}

#[derive(Debug, PartialEq, Eq)]
pub enum Beat {
    Ok,
    NotFound,
    Limited { retry_after: u64 },
}

pub struct SearchIndex {
    db: Mutex<Connection>,
    /// Read-only connections for `records` and `query`; WAL lets them run beside the writer.
    readers: Mutex<Vec<Connection>>,
    path: PathBuf,
    max_bytes: u64,
    /// Record bytes put since the last `usage()` check.
    written: AtomicU64,
    /// Set when usage passed `max_bytes`; each put evicts one batch until under 95 %.
    over: AtomicBool,
}

/// A reader connection on loan; goes back to the pool on drop.
pub(crate) struct Reader<'a> {
    index: &'a SearchIndex,
    db: Option<Connection>,
}

impl std::ops::Deref for Reader<'_> {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        self.db.as_ref().expect("reader present until drop")
    }
}

impl Drop for Reader<'_> {
    fn drop(&mut self) {
        if let (Some(db), Ok(mut pool)) = (self.db.take(), self.index.readers.lock()) {
            pool.push(db);
        }
    }
}

/// i64 for SQLite; `put` refuses versions above i64::MAX as stale.
fn sql(v: u64) -> Option<i64> {
    i64::try_from(v).ok()
}

impl SearchIndex {
    pub fn open(path: &Path, max_bytes: u64) -> Result<Self> {
        Ok(Self {
            db: storage::open(path, SCHEMA, STEPS)?,
            readers: Mutex::new(Vec::new()),
            path: path.to_owned(),
            max_bytes,
            written: AtomicU64::new(0),
            over: AtomicBool::new(false),
        })
    }

    pub(crate) fn reader(&self) -> Result<Reader<'_>> {
        let pooled = self.readers.lock().map_err(storage_error)?.pop();
        let db = match pooled {
            Some(db) => db,
            None => Connection::open_with_flags(
                &self.path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .map_err(storage_error)?,
        };
        Ok(Reader {
            index: self,
            db: Some(db),
        })
    }

    /// Stores a new version or a tombstone (`p.deleted`). `limit = false` skips the update
    /// limit for catch-up; every other check stays.
    pub fn put(
        &self,
        key: &[u8; 32],
        p: &Profile,
        record: &SignedRecord,
        now: i64,
        limit: bool,
    ) -> Result<Put> {
        let put = self.write(key, p, record, now, limit)?;
        if put == Put::Stored {
            self.bound(record.encoded_len() as u64)?;
        }
        Ok(put)
    }

    /// Keeps the file near `max_bytes` at a bounded cost per put: `usage()` once per 1 % of
    /// `max_bytes` written, then one batch of oldest keys per put until under 95 %.
    fn bound(&self, bytes: u64) -> Result<()> {
        let step = (self.max_bytes / 100).max(1);
        if self.written.fetch_add(bytes, Ordering::Relaxed) + bytes >= step {
            self.written.store(0, Ordering::Relaxed);
            if self.usage()? > self.max_bytes {
                self.over.store(true, Ordering::Relaxed);
            }
        }
        if self.over.load(Ordering::Relaxed) {
            self.remove_batch(i64::MAX)?;
            if self.usage()? < self.max_bytes / 100 * 95 {
                self.over.store(false, Ordering::Relaxed);
            }
        }
        Ok(())
    }

    fn write(
        &self,
        key: &[u8; 32],
        p: &Profile,
        record: &SignedRecord,
        now: i64,
        limit: bool,
    ) -> Result<Put> {
        let Some(version) = sql(p.version) else {
            return Ok(Put::Stale);
        };
        let mut db = lock(&self.db)?;
        let tx = db.transaction().map_err(storage_error)?;
        let known: Option<(i64, i64)> = tx
            .query_row(
                "SELECT `version`, `updated` FROM `key` WHERE `key` = ?",
                [&key[..]],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(storage_error)?;
        match known {
            Some((stored, _)) if stored >= version => return Ok(Put::Stale),
            Some((_, updated)) if limit && now - updated < UPDATE_SECONDS => {
                let wait = UPDATE_SECONDS - (now - updated);
                return Ok(Put::Limited {
                    retry_after: wait.unsigned_abs(),
                });
            }
            _ => {}
        }
        for table in ["profile", "interest", "record"] {
            tx.execute(
                &format!("DELETE FROM `{table}` WHERE `key` = ?"),
                [&key[..]],
            )
            .map_err(storage_error)?;
        }
        let topic = topic(key);
        tx.execute(
            "INSERT INTO `key` (`key`, `topic`, `version`, `deleted`, `updated`, `heartbeat`,
               `beat_day`, `beats`) VALUES (?1, ?2, ?3, ?4, ?5, ?5, 0, 0)
             ON CONFLICT (`key`) DO UPDATE SET `version` = ?3, `deleted` = ?4, `updated` = ?5,
               `heartbeat` = ?5",
            params![&key[..], topic, version, p.deleted, now],
        )
        .map_err(storage_error)?;
        if !p.deleted {
            let r = i64::from_ne_bytes(rand_bytes());
            tx.execute(
                "INSERT INTO `profile` VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                   ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
                params![
                    &key[..],
                    topic,
                    version,
                    p.age,
                    p.country,
                    p.income_from.and_then(sql),
                    p.income_to.and_then(sql),
                    p.has_kids,
                    p.wants_more_kids,
                    p.relationship_goal,
                    p.career_ambition,
                    p.education,
                    p.orientation,
                    p.role_preference,
                    p.zodiac,
                    p.financial_philosophy,
                    p.employment_status,
                    p.fitness_level,
                    p.place,
                    r
                ],
            )
            .map_err(storage_error)?;
            for interest in &p.interests {
                tx.execute(
                    "INSERT OR IGNORE INTO `interest` VALUES (?, ?)",
                    params![&key[..], interest],
                )
                .map_err(storage_error)?;
            }
            tx.execute(
                "INSERT INTO `record` VALUES (?, ?)",
                params![&key[..], record.encode_to_vec()],
            )
            .map_err(storage_error)?;
        }
        tx.commit().map_err(storage_error)?;
        Ok(Put::Stored)
    }

    /// Refreshes the heartbeat of the stored version, at most `HEARTBEATS_PER_DAY` per UTC day.
    /// An unknown key, a tombstone or any other version is `NotFound`: the sender republishes
    /// or catch-up brings the newer version.
    pub fn heartbeat(&self, key: &[u8; 32], version: u64, now: i64) -> Result<Beat> {
        let db = lock(&self.db)?;
        let known: Option<(i64, i64, i64, bool)> = db
            .query_row(
                "SELECT `version`, `beat_day`, `beats`, `deleted` FROM `key` WHERE `key` = ?",
                [&key[..]],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()
            .map_err(storage_error)?;
        let Some((stored, beat_day, beats, false)) = known else {
            return Ok(Beat::NotFound);
        };
        if sql(version) != Some(stored) {
            return Ok(Beat::NotFound);
        }
        let day = now.div_euclid(86_400);
        let beats = if day == beat_day { beats } else { 0 };
        if beats >= HEARTBEATS_PER_DAY {
            return Ok(Beat::Limited {
                retry_after: ((day + 1) * 86_400 - now).unsigned_abs(),
            });
        }
        db.execute(
            "UPDATE `key` SET `heartbeat` = ?2, `beat_day` = ?3, `beats` = ?4 WHERE `key` = ?1",
            params![&key[..], now, day, beats + 1],
        )
        .map_err(storage_error)?;
        Ok(Beat::Ok)
    }

    /// The signed records of `keys` in the given order, missing ones skipped, cut before the
    /// total passes `max_bytes`.
    pub fn records(&self, keys: &[[u8; 32]], max_bytes: usize) -> Result<Vec<SignedRecord>> {
        let db = self.reader()?;
        let mut statement = db
            .prepare_cached("SELECT `record` FROM `record` WHERE `key` = ?")
            .map_err(storage_error)?;
        let (mut out, mut size) = (Vec::new(), 0);
        for key in keys {
            let bytes: Option<Vec<u8>> = statement
                .query_row([&key[..]], |row| row.get(0))
                .optional()
                .map_err(storage_error)?;
            let Some(bytes) = bytes else { continue };
            if size + bytes.len() > max_bytes {
                break;
            }
            size += bytes.len();
            out.push(SignedRecord::decode(bytes.as_slice()).map_err(storage_error)?);
        }
        Ok(out)
    }

    /// The stored version of each key, tombstones included.
    pub fn versions(&self, keys: &[[u8; 32]]) -> Result<Vec<Option<u64>>> {
        let db = lock(&self.db)?;
        let mut statement = db
            .prepare_cached("SELECT `version` FROM `key` WHERE `key` = ?")
            .map_err(storage_error)?;
        keys.iter()
            .map(|key| {
                statement
                    .query_row([&key[..]], |row| row.get::<_, i64>(0))
                    .optional()
                    .map(|v| v.map(i64::unsigned_abs))
                    .map_err(storage_error)
            })
            .collect()
    }

    /// Up to `limit` (key, version) of one topic, sorted by key, after `after`.
    pub fn inventory(&self, topic: u32, after: &[u8], limit: u32) -> Result<Vec<([u8; 32], u64)>> {
        let db = lock(&self.db)?;
        let mut statement = db
            .prepare_cached(
                "SELECT `key`, `version` FROM `key` WHERE `topic` = ? AND `key` > ?
                 ORDER BY `key` LIMIT ?",
            )
            .map_err(storage_error)?;
        let rows = statement
            .query_map(params![topic, after, limit], |row| {
                Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(storage_error)?;
        rows.filter_map(|row| match row {
            Ok((key, version)) => key.try_into().ok().map(|k| Ok((k, version.unsigned_abs()))),
            Err(error) => Some(Err(storage_error(error))),
        })
        .collect()
    }

    /// Removes keys whose last heartbeat (or update) is before `before`, at most
    /// `EXPIRE_BATCHES` batches per call; the node calls it every `maintain` tick. Returns keys
    /// removed.
    pub fn expire(&self, before: i64) -> Result<usize> {
        let mut removed = 0;
        for _ in 0..EXPIRE_BATCHES {
            match self.remove_batch(before)? {
                0 => break,
                n => removed += n,
            }
        }
        Ok(removed)
    }

    pub fn usage(&self) -> Result<u64> {
        storage::used(&*lock(&self.db)?)
    }

    /// Deletes up to `BATCH` keys with the oldest heartbeats before `before`, in one short
    /// transaction so puts and heartbeats interleave.
    fn remove_batch(&self, before: i64) -> Result<usize> {
        let mut db = lock(&self.db)?;
        let tx = db.transaction().map_err(storage_error)?;
        let keys: Vec<Vec<u8>> = {
            let mut statement = tx
                .prepare_cached(
                    "SELECT `key` FROM `key` WHERE `heartbeat` < ? ORDER BY `heartbeat` LIMIT ?",
                )
                .map_err(storage_error)?;
            let rows = statement
                .query_map(params![before, BATCH], |row| row.get(0))
                .map_err(storage_error)?;
            rows.collect::<rusqlite::Result<_>>()
                .map_err(storage_error)?
        };
        for key in &keys {
            for table in ["profile", "interest", "record", "key"] {
                tx.execute(&format!("DELETE FROM `{table}` WHERE `key` = ?"), [key])
                    .map_err(storage_error)?;
            }
        }
        tx.commit().map_err(storage_error)?;
        Ok(keys.len())
    }
}

fn rand_bytes() -> [u8; 8] {
    let mut bytes = [0; 8];
    getrandom::fill(&mut bytes).expect("OS random source");
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use dyapp_identity::Identity;

    fn index() -> SearchIndex {
        sized(1 << 40)
    }

    fn sized(max_bytes: u64) -> SearchIndex {
        let dir = format!("/tmp/ai/test-search-{}", uuid::Uuid::new_v4());
        std::fs::create_dir_all(&dir).unwrap();
        SearchIndex::open(&Path::new(&dir).join("profiles-idx.db"), max_bytes).unwrap()
    }

    fn signed(identity: &Identity, profile: Profile) -> ([u8; 32], Profile, SignedRecord) {
        let record = profile.sign(identity);
        let key = dyapp_identity::key_hash(&identity.public_key());
        (key, profile, record)
    }

    fn profile(version: u64) -> Profile {
        Profile {
            version,
            age: 30,
            country: "DE".into(),
            place: "berlin".into(),
            interests: vec!["chess".into()],
            ..Profile::default()
        }
    }

    fn fill(index: &SearchIndex, n: u64, from: i64) -> Vec<[u8; 32]> {
        (0..n)
            .map(|i| {
                let (key, p, r) = signed(&Identity::generate(), profile(1));
                let now = from + i64::try_from(i).unwrap();
                index.put(&key, &p, &r, now, true).unwrap();
                key
            })
            .collect()
    }

    #[test]
    fn versions_limits_and_tombstones() {
        let index = index();
        let id = Identity::generate();
        let (key, p1, r1) = signed(&id, profile(1));
        assert_eq!(index.put(&key, &p1, &r1, 1_000, true).unwrap(), Put::Stored);
        assert_eq!(index.put(&key, &p1, &r1, 2_000, true).unwrap(), Put::Stale);
        let (_, p2, r2) = signed(&id, profile(2));
        assert_eq!(
            index.put(&key, &p2, &r2, 1_100, true).unwrap(),
            Put::Limited { retry_after: 500 }
        );
        assert_eq!(index.put(&key, &p2, &r2, 1_600, true).unwrap(), Put::Stored);
        assert_eq!(index.records(&[key], 1 << 20).unwrap(), vec![r2.clone()]);
        let (_, p3, r3) = signed(&id, Profile::tombstone(3));
        assert_eq!(index.put(&key, &p3, &r3, 2_300, true).unwrap(), Put::Stored);
        assert!(index.records(&[key], 1 << 20).unwrap().is_empty());
        assert_eq!(index.versions(&[key]).unwrap(), vec![Some(3)]);
        assert_eq!(index.put(&key, &p2, &r2, 9_000, true).unwrap(), Put::Stale);
    }

    #[test]
    fn heartbeats_two_per_day() {
        let index = index();
        let id = Identity::generate();
        let (key, p, r) = signed(&id, profile(4));
        let day = 86_400 * 20_000;
        assert_eq!(index.heartbeat(&key, 4, day).unwrap(), Beat::NotFound);
        index.put(&key, &p, &r, day, true).unwrap();
        assert_eq!(index.heartbeat(&key, 5, day).unwrap(), Beat::NotFound);
        assert_eq!(index.heartbeat(&key, 3, day).unwrap(), Beat::NotFound);
        assert_eq!(index.heartbeat(&key, 4, day + 10).unwrap(), Beat::Ok);
        assert_eq!(index.heartbeat(&key, 4, day + 20).unwrap(), Beat::Ok);
        assert_eq!(
            index.heartbeat(&key, 4, day + 30).unwrap(),
            Beat::Limited {
                retry_after: 86_400 - 30
            }
        );
        assert_eq!(index.heartbeat(&key, 4, day + 86_400).unwrap(), Beat::Ok);
    }

    #[test]
    fn catch_up_skips_the_update_limit() {
        let index = index();
        let id = Identity::generate();
        let (key, p1, r1) = signed(&id, profile(1));
        let (_, p2, r2) = signed(&id, profile(2));
        index.put(&key, &p1, &r1, 1_000, false).unwrap();
        assert_eq!(
            index.put(&key, &p2, &r2, 1_001, false).unwrap(),
            Put::Stored
        );
        assert_eq!(index.put(&key, &p1, &r1, 1_002, false).unwrap(), Put::Stale);
    }

    #[test]
    fn reader_works_during_a_write_transaction() {
        let index = index();
        let (key, p, r) = signed(&Identity::generate(), profile(1));
        index.put(&key, &p, &r, 1_000, true).unwrap();
        let mut db = lock(&index.db).unwrap();
        let tx = db.transaction().unwrap();
        tx.execute("DELETE FROM `record`", []).unwrap();
        // The reader sees the committed state.
        assert_eq!(index.records(&[key], 1 << 20).unwrap(), vec![r]);
        drop(tx);
    }

    #[test]
    fn puts_past_max_bytes_evict_the_oldest() {
        let probe = index();
        fill(&probe, 2_000, 0);
        let max = probe.usage().unwrap();
        let index = sized(max);
        let old = fill(&index, 2_000, 0);
        let new = fill(&index, 2_000, 10_000);
        // One 1 % check window plus a batch.
        assert!(index.usage().unwrap() <= max + max / 50);
        assert_eq!(index.versions(&old[..64]).unwrap(), vec![None; 64]);
        assert!(index.versions(&new[1_999..]).unwrap()[0].is_some());
    }

    #[test]
    fn expiry_and_inventory() {
        let index = index();
        let keys: Vec<_> = (0..40)
            .map(|i| {
                let (key, p, r) = signed(&Identity::generate(), profile(1));
                index.put(&key, &p, &r, i * 100, true).unwrap();
                key
            })
            .collect();
        let all: usize = (0..16)
            .map(|t| index.inventory(t, &[], 10_000).unwrap().len())
            .sum();
        assert_eq!(all, 40);
        let t = topic(&keys[0]);
        let listed = index.inventory(t, &[], 10_000).unwrap();
        assert!(listed.windows(2).all(|w| w[0].0 < w[1].0));
        assert!(listed.iter().all(|(k, _)| topic(k) == t));
        let after = index.inventory(t, &listed[0].0, 10_000).unwrap();
        assert_eq!(after.len(), listed.len() - 1);
        // Heartbeats 0..900 are before 1 000.
        assert_eq!(index.expire(1_000).unwrap(), 10);
        assert_eq!(index.versions(&keys[..10]).unwrap(), vec![None; 10]);
        assert!(index
            .versions(&keys[10..])
            .unwrap()
            .iter()
            .all(Option::is_some));
    }

    #[test]
    fn expire_is_capped_per_call() {
        let index = index();
        let cap = usize::try_from(BATCH * EXPIRE_BATCHES).unwrap();
        fill(&index, u64::try_from(cap).unwrap() + 10, 0);
        assert_eq!(index.expire(i64::MAX).unwrap(), cap);
        assert_eq!(index.expire(i64::MAX).unwrap(), 10);
    }
}
