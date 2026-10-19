//! Search index migrations and per-place profile counts.

use super::*;
/// Newcomer and place-count bookkeeping; places outside the GeoNames set move to the country.
pub(super) fn newcomers_and_places(db: &Connection) -> rusqlite::Result<()> {
    storage::add_column(db, "key", "first_seen", "INTEGER NOT NULL DEFAULT 0")?;
    storage::add_column(db, "key", "bytes", "INTEGER NOT NULL DEFAULT 0")?;
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS `place_count` (`country` TEXT NOT NULL, `place` TEXT NOT NULL,
           `n` INTEGER NOT NULL, PRIMARY KEY (`country`, `place`)) WITHOUT ROWID;
         CREATE INDEX IF NOT EXISTS `place_count_n` ON `place_count` (`n`);
         CREATE INDEX IF NOT EXISTS `key_first_seen` ON `key` (`first_seen`);
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

/// Deletes `keys` from every table and the place counts; returns their record bytes.
pub(super) fn remove_keys(tx: &Connection, keys: &[Vec<u8>]) -> Result<u64> {
    let mut bytes = 0;
    for key in keys {
        uncount(tx, key)?;
        let size: Option<i64> = tx
            .query_row("SELECT `bytes` FROM `key` WHERE `key` = ?", [key], |row| {
                row.get(0)
            })
            .optional()
            .map_err(storage_error)?;
        bytes += size.unwrap_or(0).unsigned_abs();
        for table in ["profile", "interest", "record", "key"] {
            tx.execute(&format!("DELETE FROM `{table}` WHERE `key` = ?"), [key])
                .map_err(storage_error)?;
        }
    }
    Ok(bytes)
}
