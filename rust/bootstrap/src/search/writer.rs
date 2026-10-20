//! The index writer connection and the keys changed while a rebuild copies the index.

use super::*;
use std::collections::HashSet;

/// Most changed keys tracked (about 16 MiB); past it the rebuild must copy again.
pub(super) const DIRTY_KEYS: usize = 262_144;

/// More keys changed than tracking holds; the changed set is lost.
#[derive(Debug, PartialEq, Eq)]
pub struct Overflow;

/// The writer connection and the keys changed since `track_start`.
pub(super) struct Writer {
    conn: Connection,
    dirty: Option<HashSet<[u8; 32]>>,
    overflow: bool,
}

impl std::ops::Deref for Writer {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        &self.conn
    }
}

impl Writer {
    pub(super) fn new(conn: Connection) -> Self {
        Self {
            conn,
            dirty: None,
            overflow: false,
        }
    }

    /// A transaction whose marked keys join the changed set on commit, in the same lock hold.
    // ponytail: rusqlite also writes through `&Connection`, so the types cannot stop a write
    // outside `change`; every index mutation goes through it.
    pub(super) fn change(&mut self) -> Result<Change<'_>> {
        Ok(Change {
            tx: self.conn.transaction().map_err(storage_error)?,
            dirty: &mut self.dirty,
            overflow: &mut self.overflow,
            keys: Vec::new(),
        })
    }

    /// A transaction for derived state only (the digest): its writes are not tracked.
    pub(super) fn derived(&mut self) -> Result<rusqlite::Transaction<'_>> {
        self.conn.transaction().map_err(storage_error)
    }

    /// Puts the writer on a newly active generation; returns the old connection.
    pub(super) fn replace(&mut self, conn: Connection) -> Connection {
        std::mem::replace(&mut self.conn, conn)
    }

    /// Takes the changed keys if at most `bound`; `None` leaves them for another round.
    pub(super) fn take_at_most(
        &mut self,
        bound: usize,
    ) -> std::result::Result<Option<Vec<[u8; 32]>>, Overflow> {
        if self.overflow {
            return Err(Overflow);
        }
        match self.dirty.as_mut() {
            Some(dirty) if dirty.len() > bound => Ok(None),
            Some(dirty) => Ok(Some(dirty.drain().collect())),
            None => Ok(Some(Vec::new())),
        }
    }

    /// Stops tracking and frees the set.
    pub(super) fn stop(&mut self) {
        self.dirty = None;
        self.overflow = false;
    }
}

/// An index transaction; dropping it without `commit` rolls back and marks nothing.
pub(super) struct Change<'a> {
    tx: rusqlite::Transaction<'a>,
    dirty: &'a mut Option<HashSet<[u8; 32]>>,
    overflow: &'a mut bool,
    keys: Vec<[u8; 32]>,
}

impl std::ops::Deref for Change<'_> {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        &self.tx
    }
}

impl Change<'_> {
    /// Records that this transaction changes `key`.
    pub(super) fn mark(&mut self, key: [u8; 32]) {
        self.keys.push(key);
    }

    pub(super) fn commit(self) -> Result<()> {
        self.tx.commit().map_err(storage_error)?;
        if let (Some(dirty), false) = (self.dirty.as_mut(), *self.overflow) {
            dirty.extend(self.keys);
            if dirty.len() > DIRTY_KEYS {
                *self.overflow = true;
                *dirty = HashSet::new();
            }
        }
        Ok(())
    }
}

impl SearchIndex {
    /// Starts tracking changed keys for a rebuild, dropping any earlier set and overflow.
    pub fn track_start(&self) -> Result<()> {
        let mut db = lock(&self.db)?;
        db.dirty = Some(HashSet::new());
        db.overflow = false;
        Ok(())
    }

    /// Takes the keys changed since `track_start` or the previous call; empty when not tracking.
    pub fn take_dirty(&self) -> Result<std::result::Result<Vec<[u8; 32]>, Overflow>> {
        let mut db = lock(&self.db)?;
        if db.overflow {
            return Ok(Err(Overflow));
        }
        Ok(Ok(db
            .dirty
            .as_mut()
            .map(|dirty| dirty.drain().collect())
            .unwrap_or_default()))
    }

    /// Stops tracking and frees the set.
    pub fn track_stop(&self) -> Result<()> {
        lock(&self.db)?.stop();
        Ok(())
    }
}
