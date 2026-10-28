//! Revoked write token keys and the signed revokes that carry them to other replicas.

use super::*;

/// Revoked keys kept per mailbox, and signed revokes: about 130 blocked contacts a month.
pub const MAX_REVOKED: i64 = 4096;

/// `revoked`: one row per (mailbox, key) until the latest revoke naming it expires. `revokes`:
/// the signed records, served to other replicas by repair.
pub(super) const SCHEMA: &str =
    "CREATE TABLE IF NOT EXISTS `revoked` (`mailbox` BLOB NOT NULL, `token_key` BLOB NOT NULL,
             `expires_at` INTEGER NOT NULL, PRIMARY KEY (`mailbox`, `token_key`)) WITHOUT ROWID;
         CREATE INDEX IF NOT EXISTS `revoked_expiry` ON `revoked` (`expires_at`);
         CREATE TABLE IF NOT EXISTS `revokes` (`seq` INTEGER PRIMARY KEY,
             `mailbox` BLOB NOT NULL, `record` BLOB NOT NULL, `expires_at` INTEGER NOT NULL,
             UNIQUE (`mailbox`, `record`));
         CREATE INDEX IF NOT EXISTS `revokes_expiry` ON `revokes` (`expires_at`);";

// Statements a test checks for scans.
pub(super) const DELETE_REVOKED: &str = "DELETE FROM `envelopes` \
     WHERE `mailbox` = ? AND `token_key` = ? AND `token_key` IS NOT NULL";
pub(super) const DELETE_CLOSED: &str = "DELETE FROM `envelopes` WHERE `mailbox` = ?";
pub(super) const IS_REVOKED: &str = "SELECT EXISTS (SELECT 1 FROM `revoked` \
     WHERE `mailbox` = ? AND `token_key` = ? AND `expires_at` > ?)";

impl BootstrapStore {
    /// Revokes `keys` of `mailbox` until `expires_at` (a key revoked longer keeps its later
    /// expiry), keeps `record` for repair and deletes the stored envelopes of the keys; an empty
    /// key marks the mailbox closed and deletes all its envelopes. `false`,
    /// with nothing changed, when the mailbox would hold more than [`MAX_REVOKED`] keys or
    /// records.
    pub fn revoke(
        &self,
        mailbox: &[u8],
        record: &SignedRecord,
        keys: &[Vec<u8>],
        expires_at: i64,
        now: i64,
    ) -> Result<bool> {
        let record = record.encode_to_vec();
        let mut db = lock(&self.messages)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        for table in ["revoked", "revokes"] {
            tx.execute(
                &format!("DELETE FROM `{table}` WHERE `mailbox` = ? AND `expires_at` <= ?"),
                params![mailbox, now],
            )
            .map_err(storage_error)?;
        }
        let count = |sql: &str| -> Result<i64> {
            tx.query_row(sql, [mailbox], |row| row.get(0))
                .map_err(storage_error)
        };
        let mut new = 0;
        for key in keys {
            let known: bool = tx
                .query_row(
                    "SELECT EXISTS (SELECT 1 FROM `revoked` WHERE `mailbox` = ? AND `token_key` = ?)",
                    params![mailbox, key],
                    |row| row.get(0),
                )
                .map_err(storage_error)?;
            new += i64::from(!known);
        }
        let stored: bool = tx
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM `revokes` WHERE `mailbox` = ? AND `record` = ?)",
                params![mailbox, record],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        if count("SELECT COUNT(*) FROM `revoked` WHERE `mailbox` = ?")? + new > MAX_REVOKED
            || (!stored
                && count("SELECT COUNT(*) FROM `revokes` WHERE `mailbox` = ?")? >= MAX_REVOKED)
        {
            return Ok(false);
        }
        for key in keys {
            tx.execute(
                "INSERT INTO `revoked` VALUES (?, ?, ?) ON CONFLICT (`mailbox`, `token_key`) \
                 DO UPDATE SET `expires_at` = MAX(`expires_at`, excluded.`expires_at`)",
                params![mailbox, key, expires_at],
            )
            .map_err(storage_error)?;
            if key.is_empty() {
                tx.execute(DELETE_CLOSED, [mailbox])
            } else {
                tx.execute(DELETE_REVOKED, params![mailbox, key])
            }
            .map_err(storage_error)?;
        }
        tx.execute(
            "INSERT OR IGNORE INTO `revokes` (`mailbox`, `record`, `expires_at`) VALUES (?, ?, ?)",
            params![mailbox, record, expires_at],
        )
        .map_err(storage_error)?;
        tx.commit().map_err(storage_error)?;
        Ok(true)
    }

    /// Whether `key` is revoked for `mailbox` at `now`.
    pub fn is_revoked(&self, mailbox: &[u8], key: &[u8], now: i64) -> Result<bool> {
        lock(&self.messages)?
            .query_row(IS_REVOKED, params![mailbox, key, now], |row| row.get(0))
            .map_err(storage_error)
    }

    /// The signed revokes of `mailbox` not expired at `now`, oldest first.
    pub fn revokes(&self, mailbox: &[u8], now: i64) -> Result<Vec<SignedRecord>> {
        let db = lock(&self.messages)?;
        let mut query = db
            .prepare(
                "SELECT `record` FROM `revokes` WHERE `mailbox` = ? AND `expires_at` > ? \
                 ORDER BY `seq`",
            )
            .map_err(storage_error)?;
        let rows: Vec<Vec<u8>> = query
            .query_map(params![mailbox, now], |row| row.get(0))
            .and_then(Iterator::collect)
            .map_err(storage_error)?;
        rows.into_iter().map(decode).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::tests::temp_store;

    fn record(n: u8) -> SignedRecord {
        SignedRecord {
            payload: vec![n],
            ..SignedRecord::default()
        }
    }

    fn put(store: &BootstrapStore, id: u8, key: Option<&[u8]>) {
        let (record, id) = (record(id), [id]);
        let envelope = NewEnvelope {
            token_key: key,
            ..NewEnvelope::new(&[1], &id, &record, false, i64::MAX)
        };
        assert_eq!(
            store.put_envelope(&envelope, u64::MAX, None).unwrap(),
            Put::Stored
        );
    }

    #[test]
    fn revoke_deletes_the_keys_envelopes_until_it_expires() {
        let store = temp_store();
        put(&store, 1, Some(b"a"));
        put(&store, 2, Some(b"b"));
        put(&store, 3, None);
        assert!(store
            .revoke(&[1], &record(9), &[b"a".to_vec()], 200, 100)
            .unwrap());
        let ids = store.envelope_ids(&[1], 10).unwrap();
        assert_eq!(ids, [vec![2], vec![3]]);
        assert!(store.is_revoked(&[1], b"a", 199).unwrap());
        assert!(!store.is_revoked(&[1], b"a", 200).unwrap());
        assert!(!store.is_revoked(&[2], b"a", 100).unwrap());
        // A replay keeps one record; an earlier expiry never shortens a later one.
        assert!(store
            .revoke(&[1], &record(9), &[b"a".to_vec()], 200, 100)
            .unwrap());
        assert!(store
            .revoke(&[1], &record(8), &[b"a".to_vec()], 150, 100)
            .unwrap());
        assert!(store.is_revoked(&[1], b"a", 199).unwrap());
        assert_eq!(store.revokes(&[1], 100).unwrap(), [record(9), record(8)]);
        assert_eq!(store.revokes(&[1], 150).unwrap(), [record(9)]);
        store.cleanup_expired(200).unwrap();
        let db = lock(&store.messages).unwrap();
        let rows: i64 = db
            .query_row(
                "SELECT (SELECT COUNT(*) FROM `revoked`) + (SELECT COUNT(*) FROM `revokes`)",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(rows, 0);
    }

    #[test]
    fn an_empty_key_deletes_every_envelope_of_the_mailbox() {
        let store = temp_store();
        put(&store, 1, Some(b"a"));
        put(&store, 2, None);
        assert!(store
            .revoke(&[1], &record(9), &[Vec::new()], 200, 100)
            .unwrap());
        assert!(store.envelope_ids(&[1], 10).unwrap().is_empty());
        assert!(store.is_revoked(&[1], b"", 199).unwrap());
    }

    #[test]
    fn revoked_keys_are_capped_per_mailbox() {
        let store = temp_store();
        let keys: Vec<Vec<u8>> = (0..MAX_REVOKED).map(|i| i.to_be_bytes().to_vec()).collect();
        for (i, chunk) in keys.chunks(64).enumerate() {
            let record = record(u8::try_from(i).unwrap());
            assert!(store.revoke(&[1], &record, chunk, 200, 100).unwrap());
        }
        let over = vec![b"over".to_vec()];
        assert!(!store.revoke(&[1], &record(255), &over, 200, 100).unwrap());
        assert!(!store.is_revoked(&[1], b"over", 100).unwrap());
        // Known keys still renew, another mailbox has its own cap, expired keys free room.
        assert!(store
            .revoke(&[1], &record(0), &keys[..1], 300, 100)
            .unwrap());
        assert!(store.revoke(&[2], &record(255), &over, 200, 100).unwrap());
        assert!(store.revoke(&[1], &record(255), &over, 400, 200).unwrap());
    }

    #[test]
    fn revocation_statements_use_indexes() {
        let store = temp_store();
        let db = lock(&store.messages).unwrap();
        for sql in [DELETE_REVOKED, DELETE_CLOSED, IS_REVOKED] {
            let mut query = db.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
            let mut rows = query.raw_query();
            let mut plan = String::new();
            while let Some(row) = rows.next().unwrap() {
                plan += &row
                    .get::<_, String>(3)
                    .unwrap()
                    .replace("SCAN CONSTANT ROW", "");
            }
            assert!(!plan.contains("SCAN"), "{sql}: {plan}");
        }
    }
}
