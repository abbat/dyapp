//! The search role's index, `profiles-idx.db`: typed profile rows, interests, the signed records
//! and the per-key limit counters. Disposable: catch-up refills it from other search nodes.

use crate::error::Result;
use crate::storage::{self, lock, storage_error, Step};
use dyapp_identity::SignedRecord;
use dyapp_p2p_net::proto::{Condition, Op};
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
pub const INDEX_VERSION: u32 = 1;
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
           ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
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
            p.education,
            p.orientation,
            p.role_preference,
            p.zodiac,
            p.financial_philosophy,
            p.employment_status,
            p.fitness_level,
            place,
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
    /// Read-only connections for `records` and `query`; WAL lets them run beside the writer.
    readers: Mutex<Vec<Connection>>,
    path: PathBuf,
    max_bytes: u64,
    /// Record bytes put since the last `usage()` check.
    written: AtomicU64,
    /// Set when usage passed `max_bytes`; each put evicts one batch until under 95 %.
    over: AtomicBool,
    /// When the file was created (Unix seconds).
    created: i64,
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
    /// Opens or creates the index at `path`, keeping it below `search.max_bytes`.
    pub fn open(path: &Path, search: &crate::config::Search) -> Result<Self> {
        let conn = storage::open(path, SCHEMA, STEPS)?
            .into_inner()
            .map_err(storage_error)?;
        let created = conn
            .query_row(
                "SELECT `value` FROM `meta` WHERE `name` = 'created'",
                [],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        let index = Self {
            db: Mutex::new(Writer::new(conn)),
            readers: Mutex::new(Vec::new()),
            path: path.to_owned(),
            max_bytes: search.max_bytes,
            written: AtomicU64::new(0),
            over: AtomicBool::new(false),
            created,
            newcomer_seconds: i64::from(search.newcomer_days) * 86_400,
            newcomer_quota: (search.max_bytes as f64 * search.newcomer_share).round() as u64,
            newcomer_bytes: AtomicU64::new(0),
            newcomer_checked: AtomicI64::new(i64::MIN),
            newcomer_evictions: AtomicU64::new(0),
        };
        index.rebuild_digest()?;
        Ok(index)
    }

    /// When the file was created (Unix seconds).
    pub fn created(&self) -> i64 {
        self.created
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
        let grace = !limit && now < self.created + self.newcomer_seconds;
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

/// Profile field number → column and whether it holds integers. Constant identifiers only.
const COLUMNS: &[(u32, &str, bool)] = &[
    (3, "age", true),
    (4, "country", false),
    (6, "income_from", true),
    (7, "income_to", true),
    (8, "has_kids", true),
    (9, "wants_more_kids", true),
    (10, "relationship_goal", false),
    (11, "career_ambition", false),
    (12, "education", false),
    (13, "orientation", false),
    (14, "role_preference", false),
    (15, "zodiac", false),
    (16, "financial_philosophy", false),
    (17, "employment_status", false),
    (18, "fitness_level", false),
    (20, "place", false),
];
const INTERESTS: u32 = 19;
/// Profile field numbers a query may filter on.
pub const SEARCH_FIELDS: &[u32] = &[3, 4, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20];

/// Result of a query.
#[derive(Debug)]
pub struct Found {
    /// Matching keys with their indexed versions.
    pub hits: Vec<([u8; 32], u64)>,
    /// Fields of the conditions that were applied.
    pub applied: Vec<u32>,
    /// Whether the scan stopped at its row limit before `limit` hits.
    pub partial: bool,
}

/// A condition turned into SQL over the `p` alias, with its bound values.
struct Filter {
    field: u32,
    sql: String,
    values: Vec<rusqlite::types::Value>,
}

/// `Ok(None)`: an unknown field or op, or `range` on a string field — skipped.
/// `Err(())`: a malformed condition — the query is `INVALID`.
fn filter(c: &Condition) -> std::result::Result<Option<Filter>, ()> {
    use rusqlite::types::Value;
    let op = Op::try_from(c.op).unwrap_or(Op::Unspecified);
    let interests = c.field == INTERESTS;
    let Some(&(_, column, int)) = COLUMNS
        .iter()
        .find(|(f, ..)| *f == c.field)
        .or(interests.then_some(&(INTERESTS, "", false)))
    else {
        return Ok(None);
    };
    let values: Vec<Value> = if int {
        if !c.strings.is_empty() {
            return Err(());
        }
        c.ints.iter().map(|v| Value::Integer(*v)).collect()
    } else {
        if !c.ints.is_empty() {
            return Err(());
        }
        c.strings.iter().map(|v| Value::Text(v.clone())).collect()
    };
    let n = values.len();
    let target = if interests {
        "EXISTS (SELECT 1 FROM `interest` AS `i` WHERE `i`.`key` = `p`.`key` AND `i`.`interest`"
            .to_string()
    } else {
        format!("`p`.`{column}`")
    };
    let close = if interests { ")" } else { "" };
    let sql = match op {
        Op::Unspecified => return Ok(None),
        Op::Range if !int => return Ok(None),
        Op::Eq if n == 1 => format!("{target} = ?{close}"),
        Op::In if (1..=8).contains(&n) => {
            format!("{target} IN ({}){close}", vec!["?"; n].join(", "))
        }
        Op::Range if n == 2 && c.ints[0] <= c.ints[1] => format!("{target} BETWEEN ? AND ?"),
        _ => return Err(()),
    };
    Ok(Some(Filter {
        field: c.field,
        sql,
        values,
    }))
}

impl SearchIndex {
    /// A random sample of up to `limit` matching keys, reading at most `max_scan_rows` rows.
    /// The inner `Err(())` means the conditions are `INVALID`.
    pub fn query(
        &self,
        conditions: &[Condition],
        limit: u32,
        max_scan_rows: u32,
    ) -> Result<std::result::Result<Found, ()>> {
        use rusqlite::types::Value;
        if conditions.len() > 16 {
            return Ok(Err(()));
        }
        let mut seen = std::collections::HashSet::new();
        if !conditions.iter().all(|c| seen.insert(c.field)) {
            return Ok(Err(()));
        }
        let mut filters = Vec::new();
        for c in conditions {
            match filter(c) {
                Ok(Some(f)) => filters.push(f),
                Ok(None) => {}
                Err(()) => return Ok(Err(())),
            }
        }
        let applied = filters.iter().map(|f| f.field).collect();
        let limit = match limit {
            0 => 50,
            n => n.min(200),
        } as usize;
        // Scopes: (country, place) pairs on the index prefix; the rest filters inside the scan.
        // ponytail: 8 × 8 statements per query; a country `in` with many places multiplies
        // scans, cap holds.
        let take = |field: u32, filters: &mut Vec<Filter>| {
            filters
                .iter()
                .position(|f| f.field == field)
                .map(|i| filters.remove(i).values)
        };
        let countries = take(4, &mut filters);
        let places = if countries.is_some() {
            take(20, &mut filters)
        } else {
            None
        };
        let mut scopes: Vec<(&str, Vec<Value>)> = Vec::new();
        match (&countries, &places) {
            (Some(cs), Some(ps)) => {
                for c in cs {
                    for p in ps {
                        scopes.push((
                            "`p`.`country` = ? AND `p`.`place` = ?",
                            vec![c.clone(), p.clone()],
                        ));
                    }
                }
            }
            (Some(cs), None) => {
                for c in cs {
                    scopes.push(("`p`.`country` = ?", vec![c.clone()]));
                }
            }
            _ => scopes.push(("1", vec![])),
        }
        let rest = if filters.is_empty() {
            "1".to_string()
        } else {
            filters
                .iter()
                .map(|f| f.sql.as_str())
                .collect::<Vec<_>>()
                .join(" AND ")
        };
        let rest_values: Vec<Value> = filters.into_iter().flat_map(|f| f.values).collect();
        let db = self.reader()?;
        let s = i64::from_ne_bytes(rand_bytes());
        let (mut hits, mut scanned) = (Vec::new(), 0u32);
        // From `s` up, then wrap to below `s`.
        for (lo, hi) in [(s, i64::MAX), (i64::MIN, s.saturating_sub(1))] {
            if hits.len() >= limit || scanned >= max_scan_rows || lo > hi {
                break;
            }
            let budget = max_scan_rows - scanned;
            let mut statements = Vec::new();
            for (scope, _) in &scopes {
                statements.push(
                    db.prepare(&format!(
                        "SELECT `p`.`key`, `p`.`version`, `p`.`r`, ({rest}) FROM `profile` AS `p`
                     WHERE {scope} AND `p`.`r` BETWEEN ? AND ? ORDER BY `p`.`r` LIMIT ?"
                    ))
                    .map_err(storage_error)?,
                );
            }
            // The `({rest})` placeholders come first, then the scope, the range and LIMIT.
            let mut cursors = Vec::new();
            for (statement, (_, scope_values)) in statements.iter_mut().zip(&scopes) {
                let mut values = rest_values.clone();
                values.extend(scope_values.iter().cloned());
                values.extend([
                    Value::Integer(lo),
                    Value::Integer(hi),
                    Value::Integer(i64::from(budget)),
                ]);
                cursors.push(
                    statement
                        .query(rusqlite::params_from_iter(values))
                        .map_err(storage_error)?,
                );
            }
            let mut heads = Vec::new();
            for rows in &mut cursors {
                heads.push(next(rows)?);
            }
            // Merge the scopes by `r`.
            while hits.len() < limit && scanned < max_scan_rows {
                let Some(i) = (0..heads.len())
                    .filter(|i| heads[*i].is_some())
                    .min_by_key(|i| heads[*i].as_ref().map(|h| h.2))
                else {
                    break;
                };
                let (key, version, _, ok) = heads[i].take().expect("picked a live head");
                scanned += 1;
                if ok {
                    hits.push((key, version));
                }
                heads[i] = next(&mut cursors[i])?;
            }
        }
        let partial = hits.len() < limit && scanned >= max_scan_rows;
        Ok(Ok(Found {
            hits,
            applied,
            partial,
        }))
    }
}

/// A scanned row: key, version, `r` and whether the other filters matched.
type Head = ([u8; 32], u64, i64, bool);

fn next(rows: &mut rusqlite::Rows<'_>) -> Result<Option<Head>> {
    let Some(row) = rows.next().map_err(storage_error)? else {
        return Ok(None);
    };
    let key: Vec<u8> = row.get(0).map_err(storage_error)?;
    Ok(Some((
        key.try_into().map_err(|_| storage_error("bad key"))?,
        row.get::<_, i64>(1).map_err(storage_error)?.unsigned_abs(),
        row.get(2).map_err(storage_error)?,
        row.get(3).map_err(storage_error)?,
    )))
}

fn rand_bytes() -> [u8; 8] {
    crate::random()
}

#[path = "search/counts.rs"]
mod counts;
use counts::{count_place, remove_keys, select_keys, uncount};
#[path = "search/candidate.rs"]
mod candidate;
pub use candidate::{Abort, Candidate};
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
