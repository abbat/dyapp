//! Builds the next index generation beside the serving file, in bounded background batches.

use super::*;
use crate::maintenance::RebuildBudget;
use rusqlite::types::Value;

/// Keys per copy or replay batch.
const COPY_KEYS: u32 = 256;
/// Changed keys a final unlocked round may take; activation replays the rest under the lock.
const SETTLED_KEYS: usize = 1024;
/// Unlocked replay rounds before the build is postponed.
const REPLAY_ROUNDS: u32 = 8;
/// The `key` row, copied as it is: version, tombstone, limits, heartbeat, newcomer bookkeeping.
const KEY_COLUMNS: &str = "`key`, `topic`, `version`, `deleted`, `updated`, `heartbeat`, \
    `beat_day`, `beats`, `first_seen`, `bytes`";
/// Estimated bytes of a key row and its index entries, for the I/O budget.
const KEY_ROW_BYTES: u64 = 128;

/// A generation file built beside the active one and not yet active.
#[derive(Debug)]
pub struct Candidate {
    /// The generation file, `profiles-idx-v{version}.db`.
    pub path: PathBuf,
    /// The index format version it is built for.
    pub version: u32,
}

/// Why a build stopped. All but `Postpone` stop tracking and delete the candidate; the node
/// retries after an hour.
#[derive(Debug)]
pub enum Abort {
    /// More keys changed than tracking holds.
    Overflow,
    /// Free space is below `min_free_mb`.
    Disk,
    /// A store error.
    Io(crate::error::BootstrapError),
    /// Replay did not settle within its rounds; the candidate and tracking stay for `replay`
    /// in 10 minutes.
    Postpone(Candidate),
}

impl From<crate::error::BootstrapError> for Abort {
    fn from(error: crate::error::BootstrapError) -> Self {
        Self::Io(error)
    }
}

/// The source state of one key.
struct Row {
    columns: Vec<Value>,
    version: i64,
    record: Option<Vec<u8>>,
    r: Option<i64>,
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    name.into()
}

fn file_len(path: &Path) -> u64 {
    std::fs::metadata(path).map_or(0, |meta| meta.len())
}

/// Deletes a database file with its `-wal` and `-shm`; a leftover is deleted at the next start.
fn remove_files(path: &Path) {
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(sibling(path, suffix));
    }
}

fn free(path: &Path) -> Result<u64> {
    let dir = path.parent().unwrap_or(Path::new("."));
    storage::free(&dir.to_string_lossy())
}

/// Reads the state of `keys` from the source in one read transaction; `None` for an absent key.
fn read(db: &Connection, keys: &[[u8; 32]]) -> Result<Vec<([u8; 32], Option<Row>)>> {
    let tx = db.unchecked_transaction().map_err(storage_error)?;
    let mut out = Vec::with_capacity(keys.len());
    for key in keys {
        let columns: Option<Vec<Value>> = tx
            .prepare_cached(&format!("SELECT {KEY_COLUMNS} FROM `key` WHERE `key` = ?"))
            .map_err(storage_error)?
            .query_row([&key[..]], |row| {
                (0..row.as_ref().column_count())
                    .map(|i| row.get(i))
                    .collect()
            })
            .optional()
            .map_err(storage_error)?;
        let Some(columns) = columns else {
            out.push((*key, None));
            continue;
        };
        let one = |sql: &str| -> Result<Option<Value>> {
            tx.prepare_cached(sql)
                .map_err(storage_error)?
                .query_row([&key[..]], |row| row.get(0))
                .optional()
                .map_err(storage_error)
        };
        let record = match one("SELECT `record` FROM `record` WHERE `key` = ?")? {
            Some(Value::Blob(record)) => Some(record),
            _ => None,
        };
        let r = match one("SELECT `r` FROM `profile` WHERE `key` = ?")? {
            Some(Value::Integer(r)) => Some(r),
            _ => None,
        };
        let Some(&Value::Integer(version)) = columns.get(2) else {
            return Err(storage_error("invalid search key version"));
        };
        out.push((
            *key,
            Some(Row {
                columns,
                version,
                record,
                r,
            }),
        ));
    }
    Ok(out)
}

/// Makes the candidate hold the source state of each key in one transaction: the old rows go
/// with their digest and place counts, the source rows come in, `profile` and `interest` rebuilt
/// from the record. Idempotent. Returns the estimated bytes written.
fn apply(db: &mut Connection, rows: &[([u8; 32], Option<Row>)]) -> Result<u64> {
    let tx = db.transaction().map_err(storage_error)?;
    let mut bytes = 0;
    for (key, row) in rows {
        let old: Option<i64> = tx
            .query_row(
                "SELECT `version` FROM `key` WHERE `key` = ?",
                [&key[..]],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage_error)?;
        if let Some(version) = old {
            flip(&tx, key, version.unsigned_abs())?;
            uncount(&tx, &key[..])?;
            for table in ["profile", "interest", "record", "key"] {
                tx.execute(
                    &format!("DELETE FROM `{table}` WHERE `key` = ?"),
                    [&key[..]],
                )
                .map_err(storage_error)?;
            }
        }
        bytes += KEY_ROW_BYTES;
        let Some(row) = row else {
            continue;
        };
        tx.execute(
            &format!("INSERT INTO `key` ({KEY_COLUMNS}) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"),
            rusqlite::params_from_iter(&row.columns),
        )
        .map_err(storage_error)?;
        flip(&tx, key, row.version.unsigned_abs())?;
        if let Some(record) = &row.record {
            let profile = SignedRecord::decode(record.as_slice())
                .ok()
                .and_then(|signed| Profile::decode(signed.payload.as_slice()).ok())
                .ok_or_else(|| storage_error("invalid indexed record"))?;
            let r = row.r.unwrap_or_else(|| i64::from_ne_bytes(rand_bytes()));
            insert_profile(&tx, key, &profile, r)?;
            tx.execute(
                "INSERT INTO `record` VALUES (?, ?)",
                params![&key[..], record],
            )
            .map_err(storage_error)?;
            bytes += record.len() as u64;
        }
    }
    tx.commit().map_err(storage_error)?;
    Ok(bytes)
}

fn keys(rows: Vec<Vec<u8>>) -> Result<Vec<[u8; 32]>> {
    rows.into_iter()
        .map(|key| {
            key.try_into()
                .map_err(|_| storage_error("invalid search key length"))
        })
        .collect()
}

impl SearchIndex {
    /// The file of generation `version` (2 and up) beside this one: `profiles-idx-v2.db`.
    pub(crate) fn generation_path(&self, version: u32) -> PathBuf {
        let stem = self.path.file_stem().unwrap_or_default().to_string_lossy();
        self.path.with_file_name(format!("{stem}-v{version}.db"))
    }

    /// Copies the index into a new generation file, then replays the keys changed meanwhile
    /// until a round takes at most 1024. The old generation serves throughout; `min_free`
    /// bytes stay free on its disk.
    // ponytail: batches run inline on the calling task; call it from a task of its own.
    pub async fn build(
        &self,
        version: u32,
        budget: &RebuildBudget,
        min_free: u64,
    ) -> std::result::Result<Candidate, Abort> {
        let candidate = Candidate {
            path: self.generation_path(version),
            version,
        };
        match self.copy(&candidate, budget, min_free).await {
            Ok(()) => self.replay(candidate, budget, min_free).await,
            Err(abort) => {
                self.discard(&candidate.path);
                Err(abort)
            }
        }
    }

    /// Replays changed keys into a built or postponed candidate.
    pub async fn replay(
        &self,
        candidate: Candidate,
        budget: &RebuildBudget,
        min_free: u64,
    ) -> std::result::Result<Candidate, Abort> {
        match self.rounds(&candidate, budget, min_free).await {
            Ok(true) => Ok(candidate),
            Ok(false) => Err(Abort::Postpone(candidate)),
            Err(abort) => {
                self.discard(&candidate.path);
                Err(abort)
            }
        }
    }

    fn discard(&self, path: &Path) {
        let _ = self.track_stop();
        remove_files(path);
    }

    async fn copy(
        &self,
        candidate: &Candidate,
        budget: &RebuildBudget,
        min_free: u64,
    ) -> std::result::Result<(), Abort> {
        let need = (file_len(&self.path) + file_len(&sibling(&self.path, "-wal")))
            .saturating_add(min_free);
        if free(&self.path)? < need {
            return Err(Abort::Disk);
        }
        // A candidate of an earlier attempt is stale: tracking restarts below.
        remove_files(&candidate.path);
        self.track_start()?;
        let mut db = storage::open(&candidate.path, SCHEMA, STEPS)?
            .into_inner()
            .map_err(storage_error)?;
        db.execute_batch(
            "CREATE TABLE `generation` (`version` INTEGER NOT NULL, `complete` INTEGER NOT NULL)",
        )
        .map_err(storage_error)?;
        db.execute(
            "INSERT INTO `generation` VALUES (?, 0)",
            [candidate.version],
        )
        .map_err(storage_error)?;
        db.execute(
            "UPDATE `meta` SET `value` = ? WHERE `name` = 'created'",
            [self.created],
        )
        .map_err(storage_error)?;
        let file = candidate.path.to_string_lossy();
        let mut after = Vec::new();
        loop {
            let batch = keys(select_keys(
                &*self.reader()?,
                "SELECT `key` FROM `key` WHERE `key` > ? ORDER BY `key` LIMIT ?",
                params![after, COPY_KEYS],
            )?)?;
            let Some(last) = batch.last() else {
                return Ok(());
            };
            let rows = read(&*self.reader()?, &batch)?;
            let bytes = apply(&mut db, &rows)?;
            let percent = u32::from(last[0]) * 100 / 256;
            after = last.to_vec();
            self.pace(budget, bytes, &file, percent as u8, min_free)
                .await?;
        }
    }

    /// Unlocked replay rounds; true once a round took at most `SETTLED_KEYS` keys.
    async fn rounds(
        &self,
        candidate: &Candidate,
        budget: &RebuildBudget,
        min_free: u64,
    ) -> std::result::Result<bool, Abort> {
        let mut db = Connection::open(&candidate.path).map_err(storage_error)?;
        let file = candidate.path.to_string_lossy();
        for _ in 0..REPLAY_ROUNDS {
            let changed = self.take_dirty()?.map_err(|Overflow| Abort::Overflow)?;
            for batch in changed.chunks(COPY_KEYS as usize) {
                let rows = read(&*self.reader()?, batch)?;
                let bytes = apply(&mut db, &rows)?;
                self.pace(budget, bytes, &file, 99, min_free).await?;
            }
            if changed.len() <= SETTLED_KEYS {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Waits for the I/O budget after a batch, then checks the free space.
    async fn pace(
        &self,
        budget: &RebuildBudget,
        bytes: u64,
        file: &str,
        percent: u8,
        min_free: u64,
    ) -> std::result::Result<(), Abort> {
        budget.after_batch(bytes, file, percent).await;
        if free(&self.path)? < min_free {
            return Err(Abort::Disk);
        }
        Ok(())
    }
}
