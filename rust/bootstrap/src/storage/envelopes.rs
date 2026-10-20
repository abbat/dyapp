//! Envelope writes: the per-mailbox quotas, the tokenless pool and its eviction rule.

use super::*;
use std::collections::BTreeMap;

/// An envelope to store, and where it came from.
pub struct NewEnvelope<'a> {
    /// The mailbox and the envelope id, unique per mailbox.
    pub mailbox: &'a [u8],
    /// See `mailbox`.
    pub id: &'a [u8],
    /// The signed envelope as stored and served.
    pub record: &'a SignedRecord,
    /// A signal rather than a message.
    pub signal: bool,
    /// Unix time the envelope expires at.
    pub expires_at: i64,
    /// The write token key; `None` for a tokenless envelope.
    pub token_key: Option<&'a [u8]>,
    /// Label of the peer that handed the envelope over.
    pub via: &'a [u8],
    /// The acceptor's label of the sender's address group.
    pub source_group: &'a [u8],
}

impl<'a> NewEnvelope<'a> {
    /// A tokenless envelope with empty labels.
    pub fn new(
        mailbox: &'a [u8],
        id: &'a [u8],
        record: &'a SignedRecord,
        signal: bool,
        expires_at: i64,
    ) -> Self {
        Self {
            mailbox,
            id,
            record,
            signal,
            expires_at,
            token_key: None,
            via: &[],
            source_group: &[],
        }
    }
}

/// What [`BootstrapStore::put_envelope`] did.
#[derive(Debug, PartialEq, Eq)]
pub enum Put {
    /// Stored now or before.
    Stored,
    /// Refused: the mailbox's messages are at their limit.
    Full,
    /// Not stored and nothing evicted: the envelope itself would be the eviction victim.
    Dropped,
}

/// `token_key` (NULL: tokenless), `via` and `source_group` per envelope; tokenless bytes per
/// (mailbox, class) and per class, kept by triggers so every delete path keeps them exact.
pub(super) fn messages_tokens(db: &Connection) -> rusqlite::Result<()> {
    add_column(db, "envelopes", "token_key", "BLOB")?;
    add_column(db, "envelopes", "via", "BLOB NOT NULL DEFAULT x''")?;
    add_column(db, "envelopes", "source_group", "BLOB NOT NULL DEFAULT x''")?;
    db.execute_batch(
        "CREATE INDEX IF NOT EXISTS `envelopes_tokenless` ON `envelopes`
             (`mailbox`, `class`, `via`, `source_group`, `seq`, `size`) WHERE `token_key` IS NULL;
         CREATE INDEX IF NOT EXISTS `envelopes_tokened` ON `envelopes`
             (`mailbox`, `token_key`, `class`, `seq`, `size`) WHERE `token_key` IS NOT NULL;
         CREATE TABLE IF NOT EXISTS `tokenless_mailbox` (`mailbox` BLOB NOT NULL,
             `class` INTEGER NOT NULL, `bytes` INTEGER NOT NULL, PRIMARY KEY (`mailbox`, `class`))
             WITHOUT ROWID;
         CREATE INDEX IF NOT EXISTS `tokenless_mailbox_bytes` ON `tokenless_mailbox` (`class`, `bytes`);
         CREATE TABLE IF NOT EXISTS `tokenless_total` (`class` INTEGER PRIMARY KEY,
             `bytes` INTEGER NOT NULL);
         DELETE FROM `tokenless_mailbox`;
         DELETE FROM `tokenless_total`;
         INSERT INTO `tokenless_mailbox` SELECT `mailbox`, `class`, SUM(`size`) FROM `envelopes`
             WHERE `token_key` IS NULL GROUP BY `mailbox`, `class`;
         INSERT INTO `tokenless_total` SELECT `class`, SUM(`bytes`) FROM `tokenless_mailbox`
             GROUP BY `class`;
         CREATE TRIGGER IF NOT EXISTS `tokenless_insert` AFTER INSERT ON `envelopes`
             WHEN NEW.`token_key` IS NULL BEGIN
             INSERT INTO `tokenless_mailbox` VALUES (NEW.`mailbox`, NEW.`class`, NEW.`size`)
                 ON CONFLICT (`mailbox`, `class`) DO UPDATE SET `bytes` = `bytes` + excluded.`bytes`;
             INSERT INTO `tokenless_total` VALUES (NEW.`class`, NEW.`size`)
                 ON CONFLICT (`class`) DO UPDATE SET `bytes` = `bytes` + excluded.`bytes`;
         END;
         CREATE TRIGGER IF NOT EXISTS `tokenless_delete` AFTER DELETE ON `envelopes`
             WHEN OLD.`token_key` IS NULL BEGIN
             UPDATE `tokenless_mailbox` SET `bytes` = `bytes` - OLD.`size`
                 WHERE `mailbox` = OLD.`mailbox` AND `class` = OLD.`class`;
             DELETE FROM `tokenless_mailbox`
                 WHERE `mailbox` = OLD.`mailbox` AND `class` = OLD.`class` AND `bytes` <= 0;
             UPDATE `tokenless_total` SET `bytes` = `bytes` - OLD.`size` WHERE `class` = OLD.`class`;
         END;",
    )
}

// The put-path statements; a test checks none of them scans.
pub(super) const EXISTS: &str =
    "SELECT EXISTS (SELECT 1 FROM `envelopes` WHERE `mailbox` = ? AND `id` = ?)";
pub(super) const CLASS_BYTES: &str =
    "SELECT COALESCE(SUM(`size`), 0) FROM `envelopes` WHERE `mailbox` = ? AND `class` = ?";
pub(super) const TOKENLESS_BYTES: &str =
    "SELECT COALESCE(MAX(`bytes`), 0) FROM `tokenless_mailbox` \
     WHERE `mailbox` = ? AND `class` = ?";
pub(super) const TOKENLESS_ROWS: &str = "SELECT `seq`, `size`, `via`, `source_group` \
     FROM `envelopes` WHERE `mailbox` = ? AND `class` = ? AND `token_key` IS NULL";
pub(super) const TOKENED_ROWS: &str = "SELECT `seq`, `size`, `token_key` FROM `envelopes` \
     WHERE `mailbox` = ? AND `token_key` IS NOT NULL AND `class` = ?";
pub(super) const DELETE: &str = "DELETE FROM `envelopes` WHERE `seq` = ?";
pub(super) const INSERT: &str = "INSERT INTO `envelopes` (`mailbox`, `id`, `record`, `size`, \
     `expires_at`, `class`, `token_key`, `via`, `source_group`) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)";
pub(super) const TOTAL: &str =
    "SELECT COALESCE(MAX(`bytes`), 0) FROM `tokenless_total` WHERE `class` = ?";
pub(super) const LARGEST: &str = "SELECT `bytes`, `mailbox` FROM `tokenless_mailbox` \
     WHERE `class` = ? ORDER BY `bytes` DESC LIMIT 2";

/// `seq` standing for the incoming envelope in a victim plan: newer than any stored one.
const INCOMING: i64 = i64::MAX;

/// An eviction candidate: its labels from the outermost level in.
struct Row {
    seq: i64,
    size: i64,
    labels: Vec<Vec<u8>>,
}

impl BootstrapStore {
    /// Stores a signed envelope once per (mailbox, id): a repeated put is a no-op and returns
    /// `Stored`, whatever its class.
    ///
    /// A message is refused (`Full`) when it would take the mailbox's messages over `max_bytes`.
    /// With a `pool`, tokenless messages count against it instead, and tokened ones alone against
    /// `max_bytes`; a tokenless message over the pool evicts by the pool rule. A signal always
    /// counts against `max_bytes` and evicts tokenless signals by the pool rule first, tokened
    /// ones only when none remain: the oldest of the token key holding most bytes. Messages are
    /// never evicted for a signal.
    pub fn put_envelope(
        &self,
        envelope: &NewEnvelope,
        max_bytes: u64,
        pool: Option<u64>,
    ) -> Result<Put> {
        let record = envelope.record.encode_to_vec();
        let mut db = lock(&self.messages)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let key = params![envelope.mailbox, envelope.id];
        if tx
            .query_row(EXISTS, key, |row| row.get(0))
            .map_err(storage_error)?
        {
            return Ok(Put::Stored);
        }
        let class = i64::from(envelope.signal);
        let bytes = |sql| -> Result<i64> {
            tx.query_row(sql, params![envelope.mailbox, class], |row| row.get(0))
                .map_err(storage_error)
        };
        let message = !envelope.signal;
        let pooled = message && pool.is_some() && envelope.token_key.is_none();
        let (used, limit) = match pool {
            Some(pool) if pooled => (bytes(TOKENLESS_BYTES)?, pool),
            Some(_) if message => (bytes(CLASS_BYTES)? - bytes(TOKENLESS_BYTES)?, max_bytes),
            _ => (bytes(CLASS_BYTES)?, max_bytes),
        };
        let size = i64::try_from(record.len()).map_err(storage_error)?;
        let over = used + size - i64::try_from(limit).unwrap_or(i64::MAX);
        if over > 0 {
            if message && !pooled {
                return Ok(Put::Full);
            }
            let incoming = Row {
                seq: INCOMING,
                size,
                labels: Vec::new(),
            };
            let mut tokenless = rows(&tx, TOKENLESS_ROWS, envelope.mailbox, class)?;
            let mut tokened = Vec::new();
            if envelope.signal {
                tokened = rows(&tx, TOKENED_ROWS, envelope.mailbox, class)?;
            }
            match envelope.token_key {
                None => tokenless.push(Row {
                    labels: vec![envelope.via.to_vec(), envelope.source_group.to_vec()],
                    ..incoming
                }),
                Some(key) => tokened.push(Row {
                    labels: vec![key.to_vec()],
                    ..incoming
                }),
            }
            let Some(victims) = plan(vec![tokenless, tokened], over) else {
                return Ok(Put::Dropped);
            };
            for seq in victims {
                tx.execute(DELETE, [seq]).map_err(storage_error)?;
            }
        }
        tx.execute(
            INSERT,
            params![
                envelope.mailbox,
                envelope.id,
                record,
                size,
                envelope.expires_at,
                class,
                envelope.token_key,
                envelope.via,
                envelope.source_group
            ],
        )
        .map_err(storage_error)?;
        tx.commit().map_err(storage_error)?;
        Ok(Put::Stored)
    }

    /// The first `limit` envelopes of a mailbox in fetch order, each with its source group, for
    /// repair.
    pub fn held_envelopes(
        &self,
        mailbox: &[u8],
        limit: u32,
    ) -> Result<Vec<(SignedRecord, Vec<u8>)>> {
        let db = lock(&self.messages)?;
        let mut query = db
            .prepare(
                "SELECT `record`, `source_group` FROM `envelopes` WHERE `mailbox` = ? \
                 ORDER BY `class`, `seq` LIMIT ?",
            )
            .map_err(storage_error)?;
        let rows: Vec<(Vec<u8>, Vec<u8>)> = query
            .query_map(params![mailbox, limit], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .and_then(Iterator::collect)
            .map_err(storage_error)?;
        rows.into_iter()
            .map(|(record, group)| Ok((decode(record)?, group)))
            .collect()
    }

    /// Deletes tokenless envelopes while they take more than `limit` bytes in the store: from
    /// the (mailbox, class) holding most of them, by the pool rule, until it is no longer the
    /// largest. Signals only unless `messages`. Bounded per call; returns the envelopes deleted.
    // ponytail: picks the largest (mailbox, class) row, not a mailbox's sum over both classes.
    pub fn evict_tokenless(&self, limit: u64, messages: bool) -> Result<usize> {
        let classes: &[i64] = if messages { &[0, 1] } else { &[1] };
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let mut db = lock(&self.messages)?;
        let mut evicted = 0;
        for _ in 0..EVICT_STEPS {
            let tx = db.transaction().map_err(storage_error)?;
            let mut total = 0;
            let mut largest: Vec<(i64, Vec<u8>, i64)> = Vec::new();
            for &class in classes {
                total += tx
                    .query_row(TOTAL, [class], |row| row.get::<_, i64>(0))
                    .map_err(storage_error)?;
                let mut query = tx.prepare(LARGEST).map_err(storage_error)?;
                let rows = query
                    .query_map([class], |row| Ok((row.get(0)?, row.get(1)?, class)))
                    .and_then(Iterator::collect::<rusqlite::Result<Vec<_>>>)
                    .map_err(storage_error)?;
                largest.extend(rows);
            }
            largest.sort_by_key(|row| std::cmp::Reverse(row.0));
            let Some((bytes, mailbox, class)) = largest.first() else {
                break;
            };
            if total <= limit {
                break;
            }
            let next = largest.get(1).map_or(0, |row| row.0);
            let over = (total - limit).min(bytes - next + 1);
            let rows = rows(&tx, TOKENLESS_ROWS, mailbox, *class)?;
            let victims = plan(vec![rows], over).unwrap_or_default();
            if victims.is_empty() {
                break;
            }
            for seq in &victims {
                tx.execute(DELETE, [seq]).map_err(storage_error)?;
            }
            tx.commit().map_err(storage_error)?;
            evicted += victims.len();
        }
        Ok(evicted)
    }
}

/// The eviction candidates of one mailbox and class, with their labels.
fn rows(db: &Connection, sql: &str, mailbox: &[u8], class: i64) -> Result<Vec<Row>> {
    let mut query = db.prepare(sql).map_err(storage_error)?;
    let count = query.column_count();
    query
        .query_map(params![mailbox, class], |row| {
            Ok(Row {
                seq: row.get(0)?,
                size: row.get(1)?,
                labels: (2..count)
                    .map(|i| row.get(i))
                    .collect::<rusqlite::Result<_>>()?,
            })
        })
        .and_then(Iterator::collect)
        .map_err(storage_error)
}

/// The `seq`s to delete to free `over` bytes, taking each tier in turn; `None` when the
/// incoming envelope would be a victim.
// ponytail: recomputes the label sums per victim, O(rows × victims) over one mailbox's tier.
fn plan(tiers: Vec<Vec<Row>>, mut over: i64) -> Option<Vec<i64>> {
    let mut victims = Vec::new();
    for mut rows in tiers {
        rows.sort_by_key(|row| row.seq);
        while over > 0 && !rows.is_empty() {
            let row = rows.remove(pick(&rows));
            if row.seq == INCOMING {
                return None;
            }
            over -= row.size;
            victims.push(row.seq);
        }
    }
    Some(victims)
}

/// The index of the next victim among `rows` (oldest first): at each label level the label
/// holding most bytes (ties: the one holding the oldest row), then its oldest row.
fn pick(rows: &[Row]) -> usize {
    let mut candidates: Vec<usize> = (0..rows.len()).collect();
    for level in 0..rows[0].labels.len() {
        let mut bytes: BTreeMap<&[u8], i64> = BTreeMap::new();
        for &i in &candidates {
            *bytes.entry(&rows[i].labels[level]).or_default() += rows[i].size;
        }
        let most = bytes.values().copied().max().unwrap_or_default();
        let label = candidates
            .iter()
            .map(|&i| rows[i].labels[level].as_slice())
            .find(|label| bytes[label] == most)
            .unwrap_or_default()
            .to_vec();
        candidates.retain(|&i| rows[i].labels[level] == label);
    }
    candidates[0]
}

#[cfg(test)]
#[path = "envelopes_tests.rs"]
mod tests;
