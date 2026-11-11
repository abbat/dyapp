//! Provisional ancestry is separate from live ownership and accepted history.
use super::{
    invalid, proto, sha256, storage_error, Connection, MediaStore, Message, Result, SignedRecord,
};
use rusqlite::{params, OptionalExtension};

pub(super) struct Session {
    pub(super) version: u64,
    pub(super) hash: Vec<u8>,
    pub(super) frontier: proto::MediaKeepCondition,
}

pub(super) fn load(db: &Connection, owner: &str) -> Result<Option<Session>> {
    let row: Option<(i64, Vec<u8>, Vec<u8>)> = db
        .query_row(
            "SELECT `version`, `target`, `frontier` FROM `keep_pending` WHERE `owner` = ?",
            [owner],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(storage_error)?;
    row.map(|(version, hash, bytes)| {
        Ok(Session {
            version: u64::try_from(version).map_err(storage_error)?,
            hash,
            frontier: proto::MediaKeepCondition::decode(bytes.as_slice()).map_err(storage_error)?,
        })
    })
    .transpose()
}

pub(super) fn prepare(
    db: &Connection,
    owner: &str,
    target: &SignedRecord,
    intent: &proto::MediaKeepIntent,
) -> Result<Option<Session>> {
    let hash = sha256(&target.encode_to_vec()).to_vec();
    if let Some(session) = load(db, owner)? {
        if session.hash == hash {
            return Ok(Some(session));
        }
        if session.version >= intent.version {
            return Ok(None);
        }
        discard(db, owner)?;
    }
    let frontier = intent
        .expected
        .clone()
        .ok_or_else(|| invalid("missing target predecessor"))?;
    db.execute(
        "INSERT INTO `keep_pending` (`owner`, `target`, `version`, `time`, `frontier`, `bytes`) VALUES (?, ?, ?, ?, ?, 0)",
        params![owner, hash, i64::try_from(intent.version).map_err(storage_error)?,
            i64::try_from(intent.time).map_err(storage_error)?, frontier.encode_to_vec()],
    ).map_err(storage_error)?;
    Ok(Some(Session {
        version: intent.version,
        hash,
        frontier,
    }))
}

pub(super) fn accepted(
    db: &Connection,
    owner: &str,
    version: u64,
    record: &SignedRecord,
) -> Result<Option<bool>> {
    let bytes: Option<Vec<u8>> = db
        .query_row(
            "SELECT `record` FROM `keep_history` WHERE `owner` = ? AND `version` = ?",
            params![owner, i64::try_from(version).map_err(storage_error)?],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)?;
    Ok(bytes.map(|bytes| bytes == record.encode_to_vec()))
}

pub(super) fn proof(db: &Connection, owner: &str, version: u64) -> Result<Option<SignedRecord>> {
    let bytes: Option<Vec<u8>> = db.query_row(
        "SELECT `record` FROM `keep_pending_records` WHERE `owner` = ?1 AND `version` = ?2
         UNION ALL SELECT `record` FROM `keep_history` WHERE `owner` = ?1 AND `version` = ?2 LIMIT 1",
        params![owner, i64::try_from(version).map_err(storage_error)?], |row| row.get(0),
    ).optional().map_err(storage_error)?;
    bytes
        .map(|bytes| SignedRecord::decode(bytes.as_slice()).map_err(storage_error))
        .transpose()
}

pub(super) fn known(
    db: &Connection,
    owner: &str,
    version: u64,
    record: &SignedRecord,
) -> Result<Option<bool>> {
    let bytes: Option<Vec<u8>> = db.query_row(
        "SELECT `record` FROM `keep_pending_records` WHERE `owner` = ?1 AND `version` = ?2
         UNION ALL SELECT `record` FROM `keep_history` WHERE `owner` = ?1 AND `version` = ?2 LIMIT 1",
        params![owner, i64::try_from(version).map_err(storage_error)?], |row| row.get(0),
    ).optional().map_err(storage_error)?;
    Ok(bytes.map(|bytes| bytes == record.encode_to_vec()))
}

pub(super) fn store(
    db: &Connection,
    owner: &str,
    record: &SignedRecord,
    intent: &proto::MediaKeepIntent,
) -> Result<bool> {
    if let Some(matches) = known(db, owner, intent.version, record)? {
        return Ok(matches);
    }
    let bytes = record.encode_to_vec();
    db.execute(
        "INSERT INTO `keep_pending_records` (`owner`, `version`, `record`) VALUES (?, ?, ?)",
        params![
            owner,
            i64::try_from(intent.version).map_err(storage_error)?,
            bytes
        ],
    )
    .map_err(storage_error)?;
    db.execute(
        "UPDATE `keep_pending` SET `bytes` = `bytes` + ? WHERE `owner` = ?",
        params![i64::try_from(bytes.len()).map_err(storage_error)?, owner],
    )
    .map_err(storage_error)?;
    Ok(true)
}

pub(super) fn save(db: &Connection, owner: &str, session: &Session) -> Result<()> {
    db.execute(
        "UPDATE `keep_pending` SET `frontier` = ? WHERE `owner` = ? AND `target` = ?",
        params![session.frontier.encode_to_vec(), owner, session.hash],
    )
    .map_err(storage_error)?;
    Ok(())
}

pub(crate) fn discard(db: &Connection, owner: &str) -> Result<()> {
    db.execute(
        "DELETE FROM `keep_pending_records` WHERE `owner` = ?",
        [owner],
    )
    .map_err(storage_error)?;
    db.execute("DELETE FROM `keep_pending` WHERE `owner` = ?", [owner])
        .map_err(storage_error)?;
    Ok(())
}

pub(super) fn trim(db: &Connection, target: u64) -> Result<usize> {
    let owners: Vec<String> = db
        .prepare("SELECT `owner` FROM `keep_pending` WHERE `bytes` > 0 ORDER BY `time`, `owner`")
        .and_then(|mut query| query.query_map([], |row| row.get(0))?.collect())
        .map_err(storage_error)?;
    let mut removed = 0;
    for owner in owners {
        if MediaStore::used(db)? <= target {
            break;
        }
        discard(db, &owner)?;
        removed += 1;
    }
    Ok(removed)
}

pub(crate) fn expire(db: &Connection, cutoff: i64) -> Result<()> {
    let owners: Vec<String> = db
        .prepare("SELECT `owner` FROM `keep_pending` WHERE `time` <= ?")
        .and_then(|mut query| query.query_map([cutoff], |row| row.get(0))?.collect())
        .map_err(storage_error)?;
    for owner in owners {
        discard(db, &owner)?;
    }
    Ok(())
}
