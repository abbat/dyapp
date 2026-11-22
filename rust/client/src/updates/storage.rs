//! Atomic app-wide update persistence, independent of identity and node operation leases.
use super::feed::Platform;
use super::snapshot::{self, Snapshot};
use rusqlite::{limits::Limit, params, Connection, OpenFlags, TransactionBehavior};
use std::fs::{self, File, OpenOptions, TryLockError};
use std::path::Path;

const APPLICATION: i64 = 0x4459_5550;
const SCHEMA: i64 = 1;

/// Storage/validation failures preserve existing data and never become a fresh checkpoint.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Filesystem, lock or synchronization failure.
    #[error("update storage IO failed: {0}")]
    Io(#[from] std::io::Error),
    /// SQLite could not complete the requested transaction.
    #[error("update storage transaction failed: {0}")]
    Sql(#[from] rusqlite::Error),
    /// OS randomness could not initialize the installation's local timer jitter.
    #[error("update timer randomness unavailable")]
    Entropy,
    /// Foreign schema, missing singleton or non-regular storage file.
    #[error("incompatible update storage")]
    Incompatible,
    /// Saved/candidate state failed the common snapshot validator.
    #[error(transparent)]
    Snapshot(#[from] snapshot::Error),
}

/// Hold this lease through one attempt, including durable admission and completion writes.
/// Dropping it releases the OS lock even after an abandoned request or unwinding.
/// No identity registry, chat or node lock is acquired.
pub struct LockedState {
    connection: Connection,
    _lease: File,
}

impl LockedState {
    /// Acquire the installation's update lease without waiting; busy means another attempt owns it.
    /// The app supplies its absolute private data root, never an identity-slot directory.
    ///
    /// # Errors
    /// Refuses relative roots, non-regular files, corrupt records, foreign schemas or failed IO.
    pub fn try_open(root: &Path) -> Result<Option<Self>, Error> {
        if !root.is_absolute() {
            return Err(Error::Incompatible);
        }
        fs::create_dir_all(root)?;
        let root = fs::canonicalize(root)?;
        let lease = regular_file(&root.join("updates.lock"))?;
        match lease.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Ok(None),
            Err(TryLockError::Error(error)) => return Err(error.into()),
        }
        let path = root.join("updates.sqlite");
        drop(regular_file(&path)?);
        let mut connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        let limit = i32::try_from(snapshot::MAX_BYTES).map_err(|_| Error::Incompatible)?;
        let _ = connection.set_limit(Limit::SQLITE_LIMIT_LENGTH, limit + 4096)?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        initialize(&mut connection, fresh_snapshot)?;
        let state = Self {
            connection,
            _lease: lease,
        };
        state.load()?;
        Ok(Some(state))
    }

    /// Read the exact saved record; a missing singleton is corruption, not first launch.
    ///
    /// # Errors
    /// Returns database or complete snapshot-validation failure.
    pub fn load(&self) -> Result<Snapshot, Error> {
        read_snapshot(&self.connection)
    }

    /// Validate first, then atomically replace the whole checkpoint/cache/high-water record.
    ///
    /// # Errors
    /// Returns validation or transaction failure; success requires the SQLite commit to complete.
    pub fn save(&mut self, snapshot: &Snapshot) -> Result<(), Error> {
        let bytes = snapshot.encode()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let previous = read_snapshot(&transaction)?;
        for platform in Platform::ALL {
            if previous
                .highest(platform)
                .is_some_and(|old| snapshot.highest(platform).is_none_or(|next| next < old))
            {
                return Err(snapshot::Error::Rollback.into());
            }
        }
        if snapshot.jitter() != previous.jitter() {
            return Err(Error::Incompatible);
        }
        let changed = transaction.execute(
            "UPDATE `state` SET `payload` = ?1 WHERE `id` = 1",
            params![bytes],
        )?;
        if changed != 1 {
            return Err(Error::Incompatible);
        }
        transaction.commit()?;
        Ok(())
    }
}

fn read_snapshot(connection: &Connection) -> Result<Snapshot, Error> {
    let bytes: Vec<u8> =
        connection.query_row("SELECT `payload` FROM `state` WHERE `id` = 1", [], |row| {
            row.get(0)
        })?;
    Ok(Snapshot::decode(&bytes)?)
}

fn regular_file(path: &Path) -> Result<File, Error> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.is_file() => return Err(Error::Incompatible),
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
        _ => {}
    }
    let mut options = OpenOptions::new();
    options.create(true).truncate(false).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    Ok(options.open(path)?)
}

fn fresh_snapshot() -> Result<Snapshot, Error> {
    let mut seed = [0; 8];
    getrandom::fill(&mut seed).map_err(|_| Error::Entropy)?;
    Ok(Snapshot::fresh(u64::from_le_bytes(seed)))
}

fn initialize(
    connection: &mut Connection,
    fresh: impl FnOnce() -> Result<Snapshot, Error>,
) -> Result<(), Error> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let application: i64 =
        transaction.pragma_query_value(None, "application_id", |row| row.get(0))?;
    let schema: i64 = transaction.pragma_query_value(None, "user_version", |row| row.get(0))?;
    match (application, schema) {
        (0, 0) => initialize_fresh(&transaction, fresh)?,
        (APPLICATION, SCHEMA) => {}
        _ => return Err(Error::Incompatible),
    }
    transaction.commit()?;
    Ok(())
}

fn initialize_fresh(
    transaction: &rusqlite::Transaction<'_>,
    fresh: impl FnOnce() -> Result<Snapshot, Error>,
) -> Result<(), Error> {
    let tables: i64 = transaction.query_row(
        "SELECT count(*) FROM `sqlite_schema` WHERE `name` NOT LIKE 'sqlite_%'",
        [],
        |row| row.get(0),
    )?;
    if tables != 0 {
        return Err(Error::Incompatible);
    }
    transaction.execute_batch(
        "CREATE TABLE `state` (`id` INTEGER PRIMARY KEY CHECK (`id` = 1), `payload` BLOB NOT NULL CHECK (typeof(`payload`) = 'blob'));",
    )?;
    transaction.execute(
        "INSERT INTO `state` (`id`, `payload`) VALUES (1, ?1)",
        params![fresh()?.encode()?],
    )?;
    transaction.pragma_update(None, "application_id", APPLICATION)?;
    transaction.pragma_update(None, "user_version", SCHEMA)?;
    Ok(())
}

#[cfg(test)]
mod tests;
