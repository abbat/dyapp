//! The exact snapshot digest and applied phase belong to one pending operation.
use super::Error;
use dyapp_identity::sha256;
use rusqlite::{params, Connection, OptionalExtension};

pub(super) const KEY: &str = "link_state_import";

pub(super) fn read(db: &Connection, state: &[u8]) -> Result<Option<bool>, Error> {
    let row: Option<(i64, Vec<u8>)> = db
        .query_row(
            "SELECT `format_version`, `value` FROM `settings` WHERE `key` = ?1",
            [KEY],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((format, bytes)) = row else {
        return Ok(None);
    };
    if format != 1 || bytes.len() != 33 || bytes[0] > 1 || bytes[1..] != sha256(state) {
        return Err(Error::InvalidSetting);
    }
    Ok(Some(bytes[0] != 0))
}

// Caller holds the pending/import writer; never records progress independently of owned writes.
pub(super) fn save(db: &Connection, state: &[u8], applied: bool) -> Result<(), Error> {
    let mut bytes = vec![u8::from(applied)];
    bytes.extend(sha256(state));
    db.execute(
        "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, 1, ?2)
         ON CONFLICT (`key`) DO UPDATE SET `format_version` = 1, `value` = excluded.`value`",
        params![KEY, bytes],
    )?;
    Ok(())
}
