//! The search role's index, `profiles-idx.db`: typed profile rows, interests, the signed records
//! and the per-key limit counters. Disposable: catch-up refills it from other search nodes.

use crate::error::Result;
use crate::storage::{self, lock, storage_error, Step};
use dyapp_identity::SignedRecord;
use dyapp_profile::Profile;
use prost::Message;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::Mutex;

/// Gossip topics of the search index (`/dyapp/profiles/<n>`).
pub const SEARCH_TOPICS: u32 = 16;
/// Index format version, reported in search replies.
pub const INDEX_VERSION: u32 = 4;
/// Least seconds between indexed versions of one key.
pub const UPDATE_SECONDS: i64 = 600;
/// Heartbeats of one key accepted per UTC day.
pub const HEARTBEATS_PER_DAY: i64 = 2;

/// Keys deleted per eviction or expiry batch, and batches per `expire` call.
const BATCH: u32 = 64;
const EXPIRE_BATCHES: u32 = 64;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS `profile` (`key` BLOB PRIMARY KEY, `topic` INTEGER NOT NULL,
  `version` INTEGER NOT NULL, `age` INTEGER, `country` TEXT, `income_from` INTEGER,
  `income_to` INTEGER, `has_kids` INTEGER, `wants_more_kids` INTEGER, `relationship_goal` TEXT,
  `career_ambition` TEXT, `education_level` INTEGER, `orientation` TEXT, `role_preference` TEXT,
  `zodiac` TEXT, `financial_philosophy` TEXT, `employment_status` TEXT, `fitness_level` TEXT,
  `place` TEXT, `gender` TEXT, `photo_count` INTEGER, `r` INTEGER NOT NULL) WITHOUT ROWID;
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
CREATE TABLE IF NOT EXISTS `digest` (`topic` INTEGER, `bucket` INTEGER, `hash` BLOB,
  PRIMARY KEY (`topic`, `bucket`)) WITHOUT ROWID;
";
const STEPS: &[Step] = &[counts::newcomers_and_places];

/// The gossip topic of a key: H(key) mod 16, where the key is already a hash.
pub fn topic(key: &[u8; 32]) -> u32 {
    u32::from(key[0] % 16)
}

/// Slots per topic: bucket = slot >> 8, sub-bucket = slot & 255.
pub const SLOTS: u32 = 1 << 16;

/// Position inside a topic, using the first 16 key bits after its topic nibble.
pub fn slot(key: &[u8; 32]) -> u32 {
    u32::from(key[0] >> 4) << 12 | u32::from(key[1]) << 4 | u32::from(key[2] >> 4)
}

/// Smallest key of slot `p` in `topic`.
///
/// # Panics
/// Panics if `topic >= SEARCH_TOPICS` or `p >= SLOTS`.
pub fn slot_start(topic: u32, p: u32) -> [u8; 32] {
    assert!(topic < SEARCH_TOPICS && p < SLOTS);
    let position = p.to_be_bytes();
    let mut key = [0; 32];
    key[0] = (position[2] & 0xf0) | topic.to_be_bytes()[3];
    key[1] = (position[2] << 4) | (position[3] >> 4);
    key[2] = position[3] << 4;
    key
}

fn row_hash(key: &[u8; 32], version: u64) -> [u8; 32] {
    let mut row = [0; 40];
    row[..32].copy_from_slice(key);
    row[32..].copy_from_slice(&version.to_be_bytes());
    dyapp_identity::sha256(&row)
}

fn xor(into: &mut [u8; 32], hash: &[u8; 32]) {
    into.iter_mut().zip(hash).for_each(|(a, b)| *a ^= b);
}

/// XOR a row into or out of its bucket in the same transaction as the key mutation.
fn flip(tx: &Connection, key: &[u8; 32], version: u64) -> Result<()> {
    let (topic, bucket) = (topic(key), slot(key) >> 8);
    let stored: Option<Vec<u8>> = tx
        .query_row(
            "SELECT `hash` FROM `digest` WHERE `topic` = ? AND `bucket` = ?",
            params![topic, bucket],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)?;
    let mut hash = match stored {
        Some(hash) => hash
            .try_into()
            .map_err(|_| storage_error("invalid digest hash length"))?,
        None => [0; 32],
    };
    xor(&mut hash, &row_hash(key, version));
    if hash == [0; 32] {
        tx.execute(
            "DELETE FROM `digest` WHERE `topic` = ? AND `bucket` = ?",
            params![topic, bucket],
        )
    } else {
        tx.execute(
            "INSERT OR REPLACE INTO `digest` VALUES (?, ?, ?)",
            params![topic, bucket, &hash[..]],
        )
    }
    .map_err(storage_error)?;
    Ok(())
}

/// Inserts the typed rows of a live profile with random rank `r` and counts its place.
fn insert_profile(tx: &Connection, key: &[u8; 32], p: &Profile, r: i64) -> Result<()> {
    let place = crate::places::canonical(&p.country, &p.place);
    count_place(tx, &p.country, place, 1)?;
    tx.execute(
        "INSERT INTO `profile` VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
           ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22)",
        params![
            &key[..],
            topic(key),
            sql(p.version),
            p.age,
            p.country,
            p.income_from.and_then(sql),
            p.income_to.and_then(sql),
            p.has_kids,
            p.wants_more_kids,
            p.relationship_goal,
            p.career_ambition,
            p.education_level,
            p.orientation,
            p.role_preference,
            p.zodiac,
            p.financial_philosophy,
            p.employment_status,
            p.fitness_level,
            place,
            p.gender,
            i64::try_from(p.photos.len()).ok(),
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
    Ok(())
}

/// What became of an indexed publish.
#[derive(Debug, PartialEq, Eq)]
pub enum Put {
    /// The version is indexed.
    Stored,
    /// The version is not above the indexed one.
    Stale,
    /// Too soon after the previous version; retry after `retry_after` seconds.
    Limited {
        /// Seconds until a retry can succeed.
        retry_after: u64,
    },
}

/// What became of a heartbeat.
#[derive(Debug, PartialEq, Eq)]
pub enum Beat {
    /// The heartbeat is recorded.
    Ok,
    /// The key or version is not indexed.
    NotFound,
    /// The day's heartbeats are used; retry after `retry_after` seconds.
    Limited {
        /// Seconds until a retry can succeed.
        retry_after: u64,
    },
}

/// The search index database.
pub struct SearchIndex {
    db: Mutex<Writer>,
    /// The active generation and read-only connections to it for `records` and `query`; WAL
    /// lets them run beside the writer.
    readers: Mutex<Pool>,
    /// The first generation's file; later ones are named after it.
    path: PathBuf,
    /// `search.lock`, held while the index is open.
    _lock: std::fs::File,
    /// The index format this binary builds.
    target: u32,
    /// Set after a drop until a full catch-up round; persisted in `meta`.
    refilling: AtomicBool,
    /// Rebuild counters for the status line.
    rebuild: worker::Stats,
    max_bytes: u64,
    /// Record bytes put since the last `usage()` check.
    written: AtomicU64,
    /// Set when usage passed `max_bytes`; each put evicts one batch until under 95 %.
    over: AtomicBool,
    /// When the active generation was created (Unix seconds).
    created: AtomicI64,
    /// A key is a newcomer this long after its first put.
    newcomer_seconds: i64,
    /// Record bytes newcomers may hold together.
    newcomer_quota: u64,
    /// Running estimate of the newcomers' record bytes, recounted at most hourly when over.
    newcomer_bytes: AtomicU64,
    /// When `newcomer_bytes` was last recounted.
    newcomer_checked: AtomicI64,
    /// Newcomers evicted by the quota since the last status line.
    newcomer_evictions: AtomicU64,
}

/// i64 for SQLite; `put` refuses versions above i64::MAX as stale.
fn sql(v: u64) -> Option<i64> {
    i64::try_from(v).ok()
}

impl SearchIndex {
    /// When the file was created (Unix seconds).
    pub fn created(&self) -> i64 {
        self.created.load(Ordering::Relaxed)
    }

    /// Heal derived state on open, including files last written by an older binary.
    fn rebuild_digest(&self) -> Result<()> {
        let started = std::time::Instant::now();
        let mut rows_seen = 0u64;
        let mut db = lock(&self.db)?;
        let tx = db.derived()?;
        tx.execute("DELETE FROM `digest`", [])
            .map_err(storage_error)?;
        let mut sums: HashMap<(u32, u32), [u8; 32]> = HashMap::new();
        {
            let mut statement = tx
                .prepare("SELECT `key`, `version` FROM `key`")
                .map_err(storage_error)?;
            let mut rows = statement.query([]).map_err(storage_error)?;
            while let Some(row) = rows.next().map_err(storage_error)? {
                let key: Vec<u8> = row.get(0).map_err(storage_error)?;
                let version: i64 = row.get(1).map_err(storage_error)?;
                let key: [u8; 32] = key
                    .try_into()
                    .map_err(|_| storage_error("invalid search key length"))?;
                rows_seen += 1;
                xor(
                    sums.entry((topic(&key), slot(&key) >> 8)).or_default(),
                    &row_hash(&key, version.unsigned_abs()),
                );
            }
        }
        for ((topic, bucket), hash) in sums.into_iter().filter(|(_, hash)| *hash != [0; 32]) {
            tx.execute(
                "INSERT INTO `digest` VALUES (?, ?, ?)",
                params![topic, bucket, &hash[..]],
            )
            .map_err(storage_error)?;
        }
        tx.commit().map_err(storage_error)?;
        tracing::info!(rows = rows_seen, elapsed_ms = %started.elapsed().as_millis(), "search digest rebuilt");
        Ok(())
    }

    /// The 256 bucket hashes of a topic; an empty bucket has an all-zero hash.
    pub fn digest(&self, topic: u32) -> Result<Vec<[u8; 32]>> {
        let db = self.reader()?;
        let mut out = vec![[0; 32]; 256];
        let mut statement = db
            .prepare_cached("SELECT `bucket`, `hash` FROM `digest` WHERE `topic` = ?")
            .map_err(storage_error)?;
        let rows = statement
            .query_map([topic], |row| {
                Ok((row.get::<_, u32>(0)?, row.get::<_, Vec<u8>>(1)?))
            })
            .map_err(storage_error)?;
        for row in rows {
            let (bucket, hash) = row.map_err(storage_error)?;
            let target = out
                .get_mut(usize::try_from(bucket).map_err(storage_error)?)
                .ok_or_else(|| storage_error("invalid digest bucket"))?;
            *target = hash
                .try_into()
                .map_err(|_| storage_error("invalid digest hash length"))?;
        }
        Ok(out)
    }

    /// Hashes of the 256 sub-buckets, plus the number of scanned rows for service billing.
    pub fn sub_digest(&self, topic: u32, bucket: u8) -> Result<(Vec<[u8; 32]>, usize)> {
        if topic >= SEARCH_TOPICS {
            return Err(storage_error("invalid search topic"));
        }
        let first = u32::from(bucket) << 8;
        let after = slot_start(topic, first);
        let before = if first + 256 == SLOTS {
            Vec::new()
        } else {
            slot_start(topic, first + 256).to_vec()
        };
        let mut out = vec![[0; 32]; 256];
        let db = self.reader()?;
        // Separate statements let SQLite use both bounds of the key_topic range.
        let sql = if before.is_empty() {
            "SELECT `key`, `version` FROM `key` WHERE `topic` = ?1 AND `key` >= ?2"
        } else {
            "SELECT `key`, `version` FROM `key` WHERE `topic` = ?1 AND `key` >= ?2 AND `key` < ?3"
        };
        let mut statement = db.prepare_cached(sql).map_err(storage_error)?;
        let read = |row: &rusqlite::Row<'_>| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)?));
        let rows = if before.is_empty() {
            statement.query_map(params![topic, &after[..]], read)
        } else {
            statement.query_map(params![topic, &after[..], before], read)
        }
        .map_err(storage_error)?;
        let mut scanned = 0;
        for row in rows {
            let (key, version) = row.map_err(storage_error)?;
            let key: [u8; 32] = key
                .try_into()
                .map_err(|_| storage_error("invalid search key length"))?;
            scanned += 1;
            xor(
                &mut out[(slot(&key) & 255) as usize],
                &row_hash(&key, version.unsigned_abs()),
            );
        }
        Ok((out, scanned))
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
        let (put, newcomer) = self.write(key, p, record, now, limit)?;
        if put == Put::Stored {
            if let Some(delta) = newcomer {
                self.bound_newcomers(key, delta, now)?;
            }
            self.bound(record.encoded_len() as u64, now)?;
        }
        Ok(put)
    }

    /// Keeps the file near `max_bytes` at a bounded cost per put: `usage()` once per 1 % of
    /// `max_bytes` written, then one batch of established keys per put until under 95 %.
    fn bound(&self, bytes: u64, now: i64) -> Result<()> {
        let step = (self.max_bytes / 100).max(1);
        if self.written.fetch_add(bytes, Ordering::Relaxed) + bytes >= step {
            self.written.store(0, Ordering::Relaxed);
            if self.usage()? > self.max_bytes {
                self.over.store(true, Ordering::Relaxed);
            }
        }
        if self.over.load(Ordering::Relaxed) {
            self.evict_established(now)?;
            if self.usage()? < self.max_bytes / 100 * 95 {
                self.over.store(false, Ordering::Relaxed);
            }
        }
        Ok(())
    }

    /// Stores the put; for a newcomer key also returns the change of its record bytes.
    fn write(
        &self,
        key: &[u8; 32],
        p: &Profile,
        record: &SignedRecord,
        now: i64,
        limit: bool,
    ) -> Result<(Put, Option<i64>)> {
        let Some(version) = sql(p.version) else {
            return Ok((Put::Stale, None));
        };
        let mut db = lock(&self.db)?;
        let mut tx = db.change()?;
        let known: Option<(i64, i64, i64, i64)> = tx
            .query_row(
                "SELECT `version`, `updated`, `first_seen`, `bytes` FROM `key` WHERE `key` = ?",
                [&key[..]],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()
            .map_err(storage_error)?;
        match known {
            Some((stored, ..)) if stored >= version => return Ok((Put::Stale, None)),
            Some((_, updated, ..)) if limit && now - updated < UPDATE_SECONDS => {
                let wait = UPDATE_SECONDS - (now - updated);
                let limited = Put::Limited {
                    retry_after: wait.unsigned_abs(),
                };
                return Ok((limited, None));
            }
            _ => {}
        }
        tx.mark(*key);
        if let Some((stored, ..)) = known {
            flip(&tx, key, stored.unsigned_abs())?;
        }
        flip(&tx, key, version.unsigned_abs())?;
        uncount(&tx, &key[..])?;
        for table in ["profile", "interest", "record"] {
            tx.execute(
                &format!("DELETE FROM `{table}` WHERE `key` = ?"),
                [&key[..]],
            )
            .map_err(storage_error)?;
        }
        let topic = topic(key);
        let bytes = if p.deleted {
            0
        } else {
            record.encoded_len() as i64
        };
        // Replicas while the file is young are the existing network's keys, not newcomers.
        let grace = !limit && now < self.created() + self.newcomer_seconds;
        let (first_seen, old_bytes) =
            known.map_or((if grace { 0 } else { now }, 0), |k| (k.2, k.3));
        tx.execute(
            "INSERT INTO `key` (`key`, `topic`, `version`, `deleted`, `updated`, `heartbeat`,
               `beat_day`, `beats`, `first_seen`, `bytes`) VALUES (?1, ?2, ?3, ?4, ?5, ?5, 0, 0, ?7, ?6)
             ON CONFLICT (`key`) DO UPDATE SET `version` = ?3, `deleted` = ?4, `updated` = ?5,
               `heartbeat` = ?5, `bytes` = ?6",
            params![&key[..], topic, version, p.deleted, now, bytes, first_seen],
        )
        .map_err(storage_error)?;
        if !p.deleted {
            insert_profile(&tx, key, p, i64::from_ne_bytes(rand_bytes()))?;
            tx.execute(
                "INSERT INTO `record` VALUES (?, ?)",
                params![&key[..], record.encode_to_vec()],
            )
            .map_err(storage_error)?;
        }
        tx.commit()?;
        let newcomer = first_seen > now - self.newcomer_seconds;
        Ok((Put::Stored, newcomer.then_some(bytes - old_bytes)))
    }

    /// Refreshes the heartbeat of the stored version, at most `HEARTBEATS_PER_DAY` per UTC day.
    /// An unknown key, a tombstone or any other version is `NotFound`: the sender republishes
    /// or catch-up brings the newer version.
    pub fn heartbeat(&self, key: &[u8; 32], version: u64, now: i64) -> Result<Beat> {
        let mut db = lock(&self.db)?;
        let mut tx = db.change()?;
        let (day, beats) = match due(&tx, key, version, now)? {
            Ok(due) => due,
            Err(beat) => return Ok(beat),
        };
        tx.execute(
            "UPDATE `key` SET `heartbeat` = ?2, `beat_day` = ?3, `beats` = ?4 WHERE `key` = ?1",
            params![&key[..], now, day, beats + 1],
        )
        .map_err(storage_error)?;
        tx.mark(*key);
        tx.commit()?;
        Ok(Beat::Ok)
    }

    /// What [`SearchIndex::heartbeat`] would return, without counting the beat.
    pub fn can_beat(&self, key: &[u8; 32], version: u64, now: i64) -> Result<Beat> {
        let db = lock(&self.db)?;
        Ok(due(&db, key, version, now)?.err().unwrap_or(Beat::Ok))
    }
}

/// The UTC day and beats so far of a heartbeat that may be counted, or why it may not.
fn due(
    db: &Connection,
    key: &[u8; 32],
    version: u64,
    now: i64,
) -> Result<std::result::Result<(i64, i64), Beat>> {
    let known: Option<(i64, i64, i64, bool)> = db
        .query_row(
            "SELECT `version`, `beat_day`, `beats`, `deleted` FROM `key` WHERE `key` = ?",
            [&key[..]],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
        .map_err(storage_error)?;
    let Some((stored, beat_day, beats, false)) = known else {
        return Ok(Err(Beat::NotFound));
    };
    if sql(version) != Some(stored) {
        return Ok(Err(Beat::NotFound));
    }
    let day = now.div_euclid(86_400);
    let beats = if day == beat_day { beats } else { 0 };
    if beats >= HEARTBEATS_PER_DAY {
        return Ok(Err(Beat::Limited {
            retry_after: ((day + 1) * 86_400 - now).unsigned_abs(),
        }));
    }
    Ok(Ok((day, beats)))
}

impl SearchIndex {
    /// Difficulty advertised by the stored profile; missing or undecodable records have none.
    pub fn pow_bits(&self, key: &[u8; 32]) -> Result<Option<u32>> {
        let db = lock(&self.db)?;
        let bytes: Option<Vec<u8>> = db
            .query_row(
                "SELECT `record` FROM `record` WHERE `key` = ?",
                [&key[..]],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage_error)?;
        Ok(bytes.and_then(|bytes| {
            let record = SignedRecord::decode(bytes.as_slice()).ok()?;
            Profile::decode(record.payload.as_slice())
                .ok()
                .map(|profile| profile.pow_bits)
        }))
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

    /// Up to `limit` (key, version) of one topic, after `after` and before `before`.
    /// Empty `before` means no upper bound; keys are sorted.
    pub fn inventory(
        &self,
        topic: u32,
        after: &[u8],
        before: &[u8],
        limit: u32,
    ) -> Result<Vec<([u8; 32], u64)>> {
        let db = lock(&self.db)?;
        let sql = if before.is_empty() {
            "SELECT `key`, `version` FROM `key` WHERE `topic` = ?1 AND `key` > ?2
             ORDER BY `key` LIMIT ?3"
        } else {
            "SELECT `key`, `version` FROM `key` WHERE `topic` = ?1 AND `key` > ?2
             AND `key` < ?3 ORDER BY `key` LIMIT ?4"
        };
        let mut statement = db.prepare_cached(sql).map_err(storage_error)?;
        let read = |row: &rusqlite::Row<'_>| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)?));
        let rows = if before.is_empty() {
            statement.query_map(params![topic, after, limit], read)
        } else {
            statement.query_map(params![topic, after, before, limit], read)
        }
        .map_err(storage_error)?;
        rows.filter_map(|row| match row {
            Ok((key, version)) => key.try_into().ok().map(|k| Ok((k, version.unsigned_abs()))),
            Err(error) => Some(Err(storage_error(error))),
        })
        .collect()
    }

    /// Removes keys whose last heartbeat (or update) is before `before`, at most
    /// `EXPIRE_BATCHES` batches per call; the node calls it on its hourly cleanup. Returns keys
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

    /// Bytes used by the index.
    pub fn usage(&self) -> Result<u64> {
        storage::used(&*lock(&self.db)?)
    }

    /// `storage::maintain` on the active generation; returns the free pages left.
    pub fn maintain(&self, pages: u32) -> Result<i64> {
        storage::maintain(&*lock(&self.db)?, pages)
    }

    /// Deletes up to `BATCH` keys with the oldest heartbeats before `before`, in one short
    /// transaction so puts and heartbeats interleave.
    fn remove_batch(&self, before: i64) -> Result<usize> {
        let mut db = lock(&self.db)?;
        let mut tx = db.change()?;
        let keys = select_keys(
            &tx,
            "SELECT `key` FROM `key` WHERE `heartbeat` < ? ORDER BY `heartbeat` LIMIT ?",
            params![before, BATCH],
        )?;
        remove_keys(&mut tx, &keys)?;
        tx.commit()?;
        Ok(keys.len())
    }
}

fn rand_bytes() -> [u8; 8] {
    crate::random()
}

#[path = "search/counts.rs"]
mod counts;
use counts::{count_place, remove_keys, select_keys, uncount};
#[path = "search/find.rs"]
mod find;
pub use find::{Found, SEARCH_FIELDS};
#[path = "search/candidate.rs"]
mod candidate;
pub use candidate::{Abort, Candidate};
#[path = "search/generation.rs"]
mod generation;
pub use generation::Replay;
use generation::{generation_file, stamp, Pool};
#[path = "search/worker.rs"]
mod worker;
pub use worker::Report;
#[path = "search/writer.rs"]
mod writer;
pub use writer::Overflow;
use writer::Writer;

#[cfg(test)]
#[path = "search/rebuild.rs"]
mod rebuild_tests;
#[cfg(test)]
#[path = "search/tracking.rs"]
mod tracking_tests;

#[cfg(test)]
#[path = "search/newcomers.rs"]
mod newcomer_tests;

#[cfg(test)]
#[path = "search/digest.rs"]
mod digest_tests;

#[cfg(test)]
#[path = "search/lifecycle.rs"]
mod lifecycle_tests;

#[cfg(test)]
#[path = "search/query.rs"]
mod query_tests;

#[cfg(test)]
#[path = "search/tests.rs"]
mod tests;
