//! Recents are a local keyboard preference, updated only with successful local outbound staging.
use crate::{
    chat::sticker::Sticker,
    store::{Error, Store},
};
use rusqlite::{params, Transaction};

/// One recent entry; availability and pixels still require a validated saved pack.
#[derive(Clone, PartialEq, Eq)]
pub struct RecentSticker {
    /// Immutable manifest digest.
    pub pack_id: [u8; 32],
    /// Selected sticker digest, never an attachment capability.
    pub sha256: [u8; 32],
}

pub(crate) fn record(tx: &Transaction<'_>, sticker: &Sticker) -> Result<(), Error> {
    let hlc = i64::try_from(sticker.hlc).map_err(|_| Error::ClockRange)?;
    tx.execute(
        "INSERT INTO `sticker_recents` (`pack_id`, `sticker_hash`, `used_hlc`) VALUES (?1, ?2, ?3)
         ON CONFLICT (`pack_id`, `sticker_hash`) DO UPDATE SET `used_hlc` = excluded.`used_hlc`",
        params![sticker.pack_id.as_slice(), sticker.sha256.as_slice(), hlc],
    )?;
    tx.execute(
        "DELETE FROM `sticker_recents` WHERE (`pack_id`, `sticker_hash`) IN (
         SELECT `pack_id`, `sticker_hash` FROM `sticker_recents`
         ORDER BY `used_hlc` DESC, `pack_id`, `sticker_hash` LIMIT -1 OFFSET 20)",
        [],
    )?;
    Ok(())
}
impl Store {
    /// Read at most twenty recent entries, newest first, without labels or media keys.
    pub fn recent_stickers(&self) -> Result<Vec<RecentSticker>, Error> {
        let mut query = self.db.prepare(
            "SELECT `pack_id`, `sticker_hash` FROM `sticker_recents`
             ORDER BY `used_hlc` DESC, `pack_id`, `sticker_hash` LIMIT 20",
        )?;
        let rows = query.query_map([], |row| {
            Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?))
        })?;
        rows.map(|row| {
            let (pack, hash) = row?;
            Ok(RecentSticker {
                pack_id: pack.try_into().map_err(|_| Error::InboxConflict)?,
                sha256: hash.try_into().map_err(|_| Error::InboxConflict)?,
            })
        })
        .collect()
    }
}
#[cfg(test)]
mod tests;
