//! Recheck staged hash links before copying authenticated proofs into accepted history.
use super::super::storage;
use crate::media::{history, storage_error};
use crate::Result;
use dyapp_identity::SignedRecord;
use dyapp_p2p_net::proto;
use prost::Message;
use rusqlite::{params, Connection};
use std::borrow::Cow;
use std::collections::BTreeMap;

struct Part {
    record: SignedRecord,
    intent: proto::MediaKeepIntent,
}

pub(super) struct Parts {
    values: BTreeMap<u64, Part>,
}

fn cached(
    bytes: &[u8],
    version: i64,
    target: &SignedRecord,
    low: u64,
    high: u64,
    upper_time: u64,
) -> Result<Part> {
    if bytes.len() > crate::replication::MEDIA_OBJECT_BYTES {
        return Err(storage_error("oversized staged proof"));
    }
    let record = SignedRecord::decode(bytes).map_err(storage_error)?;
    let intent =
        proto::MediaKeepIntent::decode(record.payload.as_slice()).map_err(storage_error)?;
    if intent.version != u64::try_from(version).map_err(storage_error)?
        || intent.version <= low
        || intent.version > high
        || intent.time > upper_time
        || record.public_key != target.public_key
        || record.signature.len() != 64
    {
        return Err(storage_error("invalid staged proof metadata"));
    }
    Ok(Part { record, intent })
}

impl Parts {
    pub(super) fn validated(
        db: &Connection,
        owner: &str,
        target: &SignedRecord,
        session: &storage::Session,
        upper_time: u64,
    ) -> Result<Self> {
        let parts = Self::load(
            db,
            owner,
            target,
            session.frontier.version,
            session.version,
            upper_time,
        )?;
        parts.check(db, owner, target, &session.frontier, session.version)?;
        Ok(parts)
    }

    fn load(
        db: &Connection,
        owner: &str,
        target: &SignedRecord,
        low: u64,
        high: u64,
        upper_time: u64,
    ) -> Result<Self> {
        let mut query = db.prepare(
            "SELECT `version`, `record` FROM `keep_pending_records` WHERE `owner` = ? ORDER BY `version`",
        ).map_err(storage_error)?;
        let mut rows = query.query([owner]).map_err(storage_error)?;
        let mut values = BTreeMap::new();
        while let Some(row) = rows.next().map_err(storage_error)? {
            let bytes = row
                .get_ref(1)
                .map_err(storage_error)?
                .as_blob()
                .map_err(storage_error)?;
            let part = cached(
                bytes,
                row.get(0).map_err(storage_error)?,
                target,
                low,
                high,
                upper_time,
            )?;
            values.insert(part.intent.version, part);
        }
        Ok(Self { values })
    }

    fn proof(&self, db: &Connection, owner: &str, version: u64) -> Result<Cow<'_, SignedRecord>> {
        if let Some(part) = self.values.get(&version) {
            return Ok(Cow::Borrowed(&part.record));
        }
        let record = storage::proof(db, owner, version)?
            .ok_or_else(|| storage_error("missing staged history link"))?;
        history::verified(owner, version, &record)?;
        Ok(Cow::Owned(record))
    }

    pub(super) fn check(
        &self,
        db: &Connection,
        owner: &str,
        target: &SignedRecord,
        frontier: &proto::MediaKeepCondition,
        target_version: u64,
    ) -> Result<()> {
        // Staging verified every signature. Rechecking the raw signed-record hash links catches
        // changed bytes without repeating all signature work under the ownership writer lock.
        for (&version, part) in &self.values {
            if storage::accepted(db, owner, version, &part.record)? == Some(false) {
                return Err(storage_error(
                    "accepted history conflicts with staged proof",
                ));
            }
            if version == target_version {
                if part.record != *target {
                    return Err(storage_error("staged target changed"));
                }
            } else {
                let next = self.proof(db, owner, version + 1)?;
                history::follows(&part.record, &next)?;
            }
            if frontier.version.checked_add(1) == Some(version) {
                if part.intent.expected.as_ref() != Some(frontier) {
                    return Err(storage_error("staged frontier changed"));
                }
            } else {
                let previous = self.proof(db, owner, version - 1)?;
                history::follows(&previous, &part.record)?;
            }
        }
        Ok(())
    }

    pub(super) fn latest_time(&self, target_time: u64) -> u64 {
        self.values
            .values()
            .fold(target_time, |time, part| time.max(part.intent.time))
    }
}

pub(super) fn archive(db: &Connection, owner: &str) -> Result<()> {
    let bytes: i64 = db
        .query_row(
            "SELECT COALESCE(SUM(length(`p`.`record`)), 0) FROM `keep_pending_records` AS `p`
         WHERE `p`.`owner` = ? AND NOT EXISTS (SELECT 1 FROM `keep_history` AS `h`
         WHERE `h`.`owner` = `p`.`owner` AND `h`.`version` = `p`.`version`)",
            [owner],
            |row| row.get(0),
        )
        .map_err(storage_error)?;
    db.execute(
        "INSERT OR IGNORE INTO `keep_history` (`owner`, `version`, `record`)
         SELECT `owner`, `version`, `record` FROM `keep_pending_records` WHERE `owner` = ? ORDER BY `version`",
        [owner],
    ).map_err(storage_error)?;
    db.execute(
        "UPDATE `owners` SET `history_bytes` = `history_bytes` + ? WHERE `owner` = ?",
        params![bytes, owner],
    )
    .map_err(storage_error)?;
    db.execute(
        "DELETE FROM `keep_pending_records` WHERE `owner` = ?",
        [owner],
    )
    .map_err(storage_error)?;
    db.execute(
        "UPDATE `keep_pending` SET `bytes` = 0 WHERE `owner` = ?",
        [owner],
    )
    .map_err(storage_error)?;
    Ok(())
}
