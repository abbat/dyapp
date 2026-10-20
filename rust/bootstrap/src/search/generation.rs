//! Index generations: the start that picks the active file, the reader pool, and the switch to a
//! rebuilt generation.

use super::candidate::{apply_rows, read, remove_files, SETTLED_KEYS};
use super::*;

/// The active generation and its idle read-only connections.
pub(super) struct Pool {
    path: PathBuf,
    version: u32,
    conns: Vec<Connection>,
}

/// A reader connection on loan; goes back to the pool on drop if its generation is still active.
pub(crate) struct Reader<'a> {
    index: &'a SearchIndex,
    version: u32,
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
            if pool.version == self.version {
                pool.conns.push(db);
            }
        }
    }
}

/// How activation treats the keys changed since the last replay round.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Replay {
    /// At most 1024 keys under the lock; more postpones the activation.
    Settled,
    /// Every key, however many: writes wait meanwhile.
    All,
    /// None: the candidate is an empty generation that refills by catch-up.
    Drop,
}

/// The file of generation `version` beside `base`: `base` itself for 1, else
/// `profiles-idx-v{version}.db`.
pub(super) fn generation_file(base: &Path, version: u32) -> PathBuf {
    if version < 2 {
        return base.to_owned();
    }
    let stem = base.file_stem().unwrap_or_default().to_string_lossy();
    base.with_file_name(format!("{stem}-v{version}.db"))
}

/// Marks a new generation file with its version, complete or not.
pub(super) fn stamp(db: &Connection, version: u32, complete: bool) -> Result<()> {
    let tx = db.unchecked_transaction().map_err(storage_error)?;
    tx.execute_batch(
        "CREATE TABLE `generation` (`version` INTEGER NOT NULL, `complete` INTEGER NOT NULL)",
    )
    .map_err(storage_error)?;
    tx.execute(
        "INSERT INTO `generation` VALUES (?, ?)",
        params![version, complete],
    )
    .map_err(storage_error)?;
    tx.commit().map_err(storage_error)
}

/// The generation files beside `base`, by version; other names are ignored.
fn generations(base: &Path) -> Result<Vec<(u32, PathBuf)>> {
    let dir = base.parent().unwrap_or(Path::new("."));
    let stem = base.file_stem().unwrap_or_default().to_string_lossy();
    let prefix = format!("{stem}-v");
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(storage_error)? {
        let path = entry.map_err(storage_error)?.path();
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let version = match name.strip_prefix(&prefix) {
            Some(rest) => rest
                .strip_suffix(".db")
                .and_then(|n| n.parse().ok())
                .unwrap_or(0),
            None => 1,
        };
        if version > 0 && generation_file(base, version) == path {
            found.push((version, path));
        }
    }
    found.sort();
    Ok(found)
}

/// Whether a generation file is complete; v1 always is.
fn complete(version: u32, path: &Path) -> Result<bool> {
    if version == 1 {
        return Ok(true);
    }
    let failed = |e: rusqlite::Error| storage_error(format!("{}: {e}", path.display()));
    let db = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(failed)?;
    let stamped: bool = db
        .query_row(
            "SELECT EXISTS (SELECT 1 FROM `sqlite_master` WHERE `name` = 'generation')",
            [],
            |row| row.get(0),
        )
        .map_err(failed)?;
    if !stamped {
        return Ok(false);
    }
    db.query_row("SELECT `complete` FROM `generation`", [], |row| row.get(0))
        .optional()
        .map(|complete| complete.unwrap_or(false))
        .map_err(failed)
}

/// Takes `search.lock` beside `base` for the life of the returned file.
fn hold(base: &Path) -> Result<std::fs::File> {
    let path = base.with_file_name("search.lock");
    let file = std::fs::File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(|e| storage_error(format!("{}: {e}", path.display())))?;
    file.try_lock().map_err(|e| {
        storage_error(format!(
            "{}: {e}; another dyappd uses this search index",
            path.display()
        ))
    })?;
    Ok(file)
}

impl SearchIndex {
    /// Opens or creates the index whose first generation is `path`, keeping it below
    /// `search.max_bytes`. Holds `search.lock` beside it until dropped.
    pub fn open(path: &Path, search: &crate::config::Search) -> Result<Self> {
        Self::open_target(path, search, INDEX_VERSION)
    }

    /// `open` for a binary of index format `target`; only tests raise it above `INDEX_VERSION`.
    pub(super) fn open_target(
        path: &Path,
        search: &crate::config::Search,
        target: u32,
    ) -> Result<Self> {
        let held = hold(path)?;
        let mut done = Vec::new();
        let mut stale = Vec::new();
        for (version, file) in generations(path)? {
            if complete(version, &file)? {
                done.push((version, file));
            } else {
                stale.push(file);
            }
        }
        if let Some((version, file)) = done.iter().find(|(version, _)| *version > target) {
            return Err(storage_error(format!(
                "{} has search index format {version}, this dyappd knows up to {target}; \
                 run a newer dyappd or delete the file to roll back",
                file.display()
            )));
        }
        // Never downgrade: the highest complete generation is active, the rest is left over.
        let version = done.last().map_or(target, |(version, _)| *version);
        stale.extend(done.into_iter().filter(|g| g.0 < version).map(|g| g.1));
        for file in stale {
            tracing::info!(file = %file.display(), "old search index generation deleted");
            remove_files(&file);
        }
        let active = generation_file(path, version);
        let fresh = version >= 2 && !active.exists();
        let conn = storage::open(&active, SCHEMA, STEPS)?
            .into_inner()
            .map_err(storage_error)?;
        if fresh {
            stamp(&conn, version, true)?;
        }
        let meta = |name: &str| {
            conn.query_row(
                "SELECT `value` FROM `meta` WHERE `name` = ?",
                [name],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(storage_error)
        };
        let created = meta("created")?.unwrap_or_default();
        let refilling = meta("refilling")?.is_some();
        let index = Self {
            db: Mutex::new(Writer::new(conn)),
            readers: Mutex::new(Pool {
                path: active,
                version,
                conns: Vec::new(),
            }),
            path: path.to_owned(),
            _lock: held,
            target,
            refilling: AtomicBool::new(refilling),
            rebuild: worker::Stats::default(),
            max_bytes: search.max_bytes,
            written: AtomicU64::new(0),
            over: AtomicBool::new(false),
            created: AtomicI64::new(created),
            newcomer_seconds: i64::from(search.newcomer_days) * 86_400,
            newcomer_quota: (search.max_bytes as f64 * search.newcomer_share).round() as u64,
            newcomer_bytes: AtomicU64::new(0),
            newcomer_checked: AtomicI64::new(i64::MIN),
            newcomer_evictions: AtomicU64::new(0),
        };
        index.rebuild_digest()?;
        Ok(index)
    }

    /// The index format version of the active generation.
    pub fn index_version(&self) -> u32 {
        self.readers.lock().map_or(self.target, |pool| pool.version)
    }

    /// The file of the active generation.
    pub(super) fn active_path(&self) -> PathBuf {
        self.readers
            .lock()
            .map_or_else(|_| self.path.clone(), |pool| pool.path.clone())
    }

    /// A read-only connection to the active generation. A path deleted by an activation since
    /// it was read fails to open, as no file is created, and is read once more.
    pub(crate) fn reader(&self) -> Result<Reader<'_>> {
        let mut retried = false;
        loop {
            let (path, version) = {
                let mut pool = lock(&self.readers)?;
                if let Some(db) = pool.conns.pop() {
                    let version = pool.version;
                    return Ok(Reader {
                        index: self,
                        version,
                        db: Some(db),
                    });
                }
                (pool.path.clone(), pool.version)
            };
            match Connection::open_with_flags(
                &path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            ) {
                Ok(db) => {
                    return Ok(Reader {
                        index: self,
                        version,
                        db: Some(db),
                    })
                }
                Err(_) if !retried => retried = true,
                Err(error) => return Err(storage_error(error)),
            }
        }
    }

    /// Makes `candidate` the active generation under the writer lock: replays the keys changed
    /// since the last round and sets it complete in one transaction, switches the writer and
    /// the reader pool, then deletes the old generation. On failure the candidate is deleted,
    /// except on `Postpone`.
    pub fn activate(&self, candidate: Candidate, replay: Replay) -> std::result::Result<(), Abort> {
        let started = std::time::Instant::now();
        let mut db = lock(&self.db)?;
        let bound = match replay {
            Replay::Settled => SETTLED_KEYS,
            Replay::All | Replay::Drop => usize::MAX,
        };
        let keys = match (replay, db.take_at_most(bound)) {
            (Replay::Drop, _) => Vec::new(),
            (_, Ok(Some(keys))) => keys,
            (_, Ok(None)) => return Err(Abort::Postpone(candidate)),
            (_, Err(Overflow)) => {
                drop(db);
                self.discard(&candidate.path);
                return Err(Abort::Overflow);
            }
        };
        let finish = || -> Result<(Connection, i64)> {
            let mut conn = storage::open(&candidate.path, SCHEMA, STEPS)?
                .into_inner()
                .map_err(storage_error)?;
            let rows = read(&db, &keys)?;
            let tx = conn.transaction().map_err(storage_error)?;
            apply_rows(&tx, &rows)?;
            if replay == Replay::Drop {
                tx.execute(
                    "INSERT OR REPLACE INTO `meta` (`name`, `value`) VALUES ('refilling', 1)",
                    [],
                )
                .map_err(storage_error)?;
            }
            tx.execute("UPDATE `generation` SET `complete` = 1", [])
                .map_err(storage_error)?;
            tx.commit().map_err(storage_error)?;
            let created = conn
                .query_row(
                    "SELECT `value` FROM `meta` WHERE `name` = 'created'",
                    [],
                    |row| row.get(0),
                )
                .map_err(storage_error)?;
            Ok((conn, created))
        };
        let (conn, created) = match finish() {
            Ok(done) => done,
            Err(error) => {
                drop(db);
                self.discard(&candidate.path);
                return Err(error.into());
            }
        };
        let old = db.replace(conn);
        db.stop();
        let old_path = {
            let mut pool = self
                .readers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            pool.conns.clear();
            pool.version = candidate.version;
            std::mem::replace(&mut pool.path, candidate.path)
        };
        if replay == Replay::Drop {
            self.refilling.store(true, Ordering::Relaxed);
        }
        self.created.store(created, Ordering::Relaxed);
        self.newcomer_checked.store(i64::MIN, Ordering::Relaxed);
        self.written.store(0, Ordering::Relaxed);
        let used = storage::used(&db).unwrap_or(0);
        self.over.store(used > self.max_bytes, Ordering::Relaxed);
        drop(db);
        drop(old);
        remove_files(&old_path);
        tracing::info!(
            version = candidate.version,
            replayed = keys.len(),
            elapsed_ms = %started.elapsed().as_millis(),
            "search index generation activated"
        );
        Ok(())
    }

    /// Drops the index for an empty complete generation `target` that refills by catch-up.
    pub fn refill(&self) -> std::result::Result<(), Abort> {
        let candidate = Candidate {
            path: self.generation_path(self.target),
            version: self.target,
        };
        remove_files(&candidate.path);
        let db = storage::open(&candidate.path, SCHEMA, STEPS)?
            .into_inner()
            .map_err(storage_error)?;
        stamp(&db, candidate.version, false)?;
        drop(db);
        self.activate(candidate, Replay::Drop)
    }

    /// Whether the index is refilling after a drop; search answers are then `partial`.
    pub fn refilling(&self) -> bool {
        self.refilling.load(Ordering::Relaxed)
    }

    /// Ends refilling after a full catch-up round.
    pub fn refilled(&self) -> Result<()> {
        if !self.refilling() {
            return Ok(());
        }
        lock(&self.db)?
            .execute("DELETE FROM `meta` WHERE `name` = 'refilling'", [])
            .map_err(storage_error)?;
        self.refilling.store(false, Ordering::Relaxed);
        tracing::info!("search index refilled by catch-up");
        Ok(())
    }
}

#[cfg(test)]
#[path = "generations.rs"]
mod tests;
