//! Search index migrations, per-place profile counts, the newcomer quota and eviction.

use super::*;
/// Newcomer and place-count bookkeeping; places outside the GeoNames set move to the country.
pub(super) fn newcomers_and_places(db: &Connection) -> rusqlite::Result<()> {
    storage::add_column(db, "key", "first_seen", "INTEGER NOT NULL DEFAULT 0")?;
    storage::add_column(db, "key", "bytes", "INTEGER NOT NULL DEFAULT 0")?;
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS `place_count` (`country` TEXT NOT NULL, `place` TEXT NOT NULL,
           `n` INTEGER NOT NULL, PRIMARY KEY (`country`, `place`)) WITHOUT ROWID;
         CREATE INDEX IF NOT EXISTS `place_count_n` ON `place_count` (`n`);
         CREATE INDEX IF NOT EXISTS `key_first_seen` ON `key` (`first_seen`, `bytes`);
         CREATE TABLE IF NOT EXISTS `meta` (`name` TEXT PRIMARY KEY, `value` INTEGER NOT NULL)
           WITHOUT ROWID;
         UPDATE `key` SET `bytes` = COALESCE(
           (SELECT length(`record`) FROM `record` WHERE `record`.`key` = `key`.`key`), 0);
         UPDATE `profile` SET `country` = COALESCE(`country`, ''), `place` = COALESCE(`place`, '');",
    )?;
    let places: Vec<(String, String)> = db
        .prepare("SELECT DISTINCT `country`, `place` FROM `profile`")?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    for (country, place) in places {
        if crate::places::canonical(&country, &place) != place {
            db.execute(
                "UPDATE `profile` SET `place` = '' WHERE `country` = ? AND `place` = ?",
                params![country, place],
            )?;
        }
    }
    db.execute_batch(
        "DELETE FROM `place_count`;
         INSERT INTO `place_count` SELECT `country`, `place`, COUNT(*) FROM `profile` GROUP BY 1, 2;",
    )?;
    db.execute(
        "INSERT OR IGNORE INTO `meta` (`name`, `value`) VALUES ('created', ?)",
        [chrono::Utc::now().timestamp()],
    )?;
    Ok(())
}

/// Makes the old serving generation writable while its replacement is built from records.
pub(super) fn birth_bounds(db: &Connection) -> rusqlite::Result<()> {
    storage::add_column(db, "profile", "birth_earliest", "INTEGER")?;
    storage::add_column(db, "profile", "birth_latest", "INTEGER")?;
    Ok(())
}

/// Adds `delta` to the profiles indexed under (country, place); a zero count has no row.
pub(super) fn count_place(tx: &Connection, country: &str, place: &str, delta: i64) -> Result<()> {
    tx.execute(
        "INSERT INTO `place_count` VALUES (?1, ?2, ?3)
         ON CONFLICT (`country`, `place`) DO UPDATE SET `n` = `n` + excluded.`n`",
        params![country, place, delta],
    )
    .map_err(storage_error)?;
    tx.execute(
        "DELETE FROM `place_count` WHERE `country` = ? AND `place` = ? AND `n` <= 0",
        params![country, place],
    )
    .map_err(storage_error)?;
    Ok(())
}

/// Takes the indexed profile of `key`, if any, out of the place counts.
pub(super) fn uncount(tx: &Connection, key: &[u8]) -> Result<()> {
    let old: Option<(String, String)> = tx
        .query_row(
            "SELECT `country`, `place` FROM `profile` WHERE `key` = ?",
            [key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(storage_error)?;
    match old {
        Some((country, place)) => count_place(tx, &country, &place, -1),
        None => Ok(()),
    }
}

/// Deletes `keys` from every table, the digest and the place counts; returns their record bytes.
pub(super) fn remove_keys(tx: &mut writer::Change<'_>, keys: &[Vec<u8>]) -> Result<u64> {
    let mut bytes = 0;
    for key in keys {
        let row: Option<(i64, i64)> = tx
            .query_row(
                "SELECT `version`, `bytes` FROM `key` WHERE `key` = ?",
                [key],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(storage_error)?;
        let Some((version, size)) = row else {
            continue;
        };
        let key_array = <&[u8; 32]>::try_from(key.as_slice())
            .map_err(|_| storage_error("invalid search key length"))?;
        tx.mark(*key_array);
        flip(tx, key_array, version.unsigned_abs())?;
        uncount(tx, key)?;
        bytes += size.unsigned_abs();
        for table in ["profile", "interest", "record", "key"] {
            tx.execute(&format!("DELETE FROM `{table}` WHERE `key` = ?"), [key])
                .map_err(storage_error)?;
        }
    }
    Ok(bytes)
}

/// The first column of each row of `sql`.
pub(super) fn select_keys(
    tx: &Connection,
    sql: &str,
    params: impl rusqlite::Params,
) -> Result<Vec<Vec<u8>>> {
    let mut statement = tx.prepare_cached(sql).map_err(storage_error)?;
    let rows = statement
        .query_map(params, |row| row.get(0))
        .map_err(storage_error)?;
    rows.collect::<rusqlite::Result<_>>().map_err(storage_error)
}

/// Places tried, largest first, before eviction falls back to any established key.
const PLACE_TRIES: u32 = 16;

impl SearchIndex {
    /// Newcomers evicted by the quota since the index opened.
    pub fn newcomer_evictions(&self) -> u64 {
        self.newcomer_evictions.load(Ordering::Relaxed)
    }

    /// Newcomers evicted by the quota since the last call; the total goes on.
    pub fn take_newcomer_evictions(&self) -> u64 {
        let total = self.newcomer_evictions();
        let taken = self
            .newcomer_evictions_taken
            .fetch_max(total, Ordering::Relaxed);
        total.saturating_sub(taken)
    }

    /// Adds `delta` record bytes of newcomer `key`; past the quota, evicts other newcomers,
    /// oldest heartbeat first.
    pub(super) fn bound_newcomers(&self, key: &[u8; 32], delta: i64, now: i64) -> Result<()> {
        let since = now - self.newcomer_seconds;
        let mut held = self
            .newcomer_bytes
            .load(Ordering::Relaxed)
            .saturating_add_signed(delta);
        // Graduations and removals only lower the true sum: recount before evicting.
        if held > self.newcomer_quota
            && now.saturating_sub(self.newcomer_checked.load(Ordering::Relaxed)) >= 3_600
        {
            self.newcomer_checked.store(now, Ordering::Relaxed);
            held = lock(&self.db)?
                .query_row(
                    "SELECT COALESCE(SUM(`bytes`), 0) FROM `key` WHERE `first_seen` > ?",
                    [since],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(storage_error)?
                .unsigned_abs();
        }
        while held > self.newcomer_quota {
            let mut db = lock(&self.db)?;
            let mut tx = db.change()?;
            // ponytail: walks heartbeat order past established keys; index newcomers by
            // heartbeat if this scan shows up in profiles.
            let mut statement = tx
                .prepare_cached(
                    "SELECT `key`, `bytes` FROM `key` WHERE `first_seen` > ?1 AND `bytes` > 0
                       AND `key` != ?2 ORDER BY `heartbeat` LIMIT ?3",
                )
                .map_err(storage_error)?;
            let rows = statement
                .query_map(params![since, &key[..], BATCH], |row| {
                    Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)?))
                })
                .map_err(storage_error)?;
            let (mut keys, mut freed) = (vec![], 0);
            for row in rows {
                let (key, bytes) = row.map_err(storage_error)?;
                if held.saturating_sub(freed) <= self.newcomer_quota {
                    break;
                }
                freed += bytes.unsigned_abs();
                keys.push(key);
            }
            drop(statement);
            if keys.is_empty() {
                break;
            }
            remove_keys(&mut tx, &keys)?;
            tx.commit()?;
            held = held.saturating_sub(freed);
            let evicted = keys.len() as u64;
            self.newcomer_evictions
                .fetch_add(evicted, Ordering::Relaxed);
        }
        self.newcomer_bytes.store(held, Ordering::Relaxed);
        Ok(())
    }

    /// Removes up to `BATCH` established keys from the place with the most indexed profiles,
    /// highest `r` first. Tombstones and newcomers are never removed by size.
    pub(super) fn evict_established(&self, now: i64) -> Result<usize> {
        let since = now - self.newcomer_seconds;
        let mut db = lock(&self.db)?;
        let mut tx = db.change()?;
        let mut keys = vec![];
        for offset in 0..PLACE_TRIES {
            let place: Option<(String, String)> = tx
                .query_row(
                    "SELECT `country`, `place` FROM `place_count` ORDER BY `n` DESC LIMIT 1 OFFSET ?",
                    [offset],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(storage_error)?;
            let Some((country, place)) = place else {
                break;
            };
            keys = select_keys(
                &tx,
                "SELECT `profile`.`key` FROM `profile` JOIN `key` ON `key`.`key` = `profile`.`key`
                 WHERE `profile`.`country` = ?1 AND `profile`.`place` = ?2
                   AND `key`.`first_seen` <= ?3 ORDER BY `profile`.`r` DESC LIMIT ?4",
                params![country, place, since, BATCH],
            )?;
            if !keys.is_empty() {
                break;
            }
        }
        if keys.is_empty() {
            keys = select_keys(
                &tx,
                "SELECT `profile`.`key` FROM `profile` JOIN `key` ON `key`.`key` = `profile`.`key`
                 WHERE `key`.`first_seen` <= ?1 ORDER BY `profile`.`r` DESC LIMIT ?2",
                params![since, BATCH],
            )?;
        }
        remove_keys(&mut tx, &keys)?;
        tx.commit()?;
        Ok(keys.len())
    }
}
