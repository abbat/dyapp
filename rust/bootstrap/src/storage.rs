use crate::error::{BootstrapError, Result};
use dyapp_identity::SignedRecord;
use dyapp_profile::VerifiedProfile;
use prost::Message;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageBlob {
    pub id: String,
    pub sender_id: String,
    pub recipient_id: String,
    pub encrypted_payload: Vec<u8>,
    pub timestamp: i64,
    pub ttl_expires_at: i64,
}

/// One SQLite file per data type in the storage directory.
// ponytail: one connection per store behind a mutex; a reader pool if reads contend.
pub struct BootstrapStore {
    profiles: Mutex<Connection>,
    messages: Mutex<Connection>,
}

fn storage_error(error: impl ToString) -> BootstrapError {
    BootstrapError::StorageError(error.to_string())
}

fn open(path: &Path, schema: &str) -> Result<Mutex<Connection>> {
    let db = Connection::open(path).map_err(storage_error)?;
    // auto_vacuum takes effect only before the first table is created.
    db.execute_batch(&format!(
        "PRAGMA auto_vacuum = INCREMENTAL; PRAGMA journal_mode = WAL; \
         PRAGMA synchronous = NORMAL; {schema}"
    ))
    .map_err(storage_error)?;
    Ok(Mutex::new(db))
}

fn lock(db: &Mutex<Connection>) -> Result<MutexGuard<'_, Connection>> {
    db.lock().map_err(storage_error)
}

const MESSAGE_COLUMNS: &str =
    "id, sender_id, recipient_id, encrypted_payload, timestamp, ttl_expires_at";

fn message_row(row: &Row) -> rusqlite::Result<MessageBlob> {
    Ok(MessageBlob {
        id: row.get(0)?,
        sender_id: row.get(1)?,
        recipient_id: row.get(2)?,
        encrypted_payload: row.get(3)?,
        timestamp: row.get(4)?,
        ttl_expires_at: row.get(5)?,
    })
}

fn decode(bytes: Vec<u8>) -> Result<SignedRecord> {
    SignedRecord::decode(bytes.as_slice())
        .map_err(|e| BootstrapError::SerializationError(e.to_string()))
}

impl BootstrapStore {
    pub fn new(path: &str) -> Result<Self> {
        let dir = Path::new(path);
        std::fs::create_dir_all(dir).map_err(storage_error)?;
        Ok(Self {
            profiles: open(
                &dir.join("profiles.db"),
                "CREATE TABLE IF NOT EXISTS profiles (
                     peer_id TEXT PRIMARY KEY,
                     record BLOB NOT NULL,
                     live INTEGER NOT NULL
                 );",
            )?,
            messages: open(
                &dir.join("messages.db"),
                "CREATE TABLE IF NOT EXISTS messages (
                     id TEXT PRIMARY KEY,
                     sender_id TEXT NOT NULL,
                     recipient_id TEXT NOT NULL,
                     encrypted_payload BLOB NOT NULL,
                     timestamp INTEGER NOT NULL,
                     ttl_expires_at INTEGER NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS messages_recipient ON messages (recipient_id);
                 CREATE INDEX IF NOT EXISTS messages_ttl ON messages (ttl_expires_at);",
            )?,
        })
    }

    pub fn store_message(&self, msg: MessageBlob) -> Result<()> {
        lock(&self.messages)?
            .execute(
                &format!(
                    "INSERT OR REPLACE INTO messages ({MESSAGE_COLUMNS}) VALUES (?, ?, ?, ?, ?, ?)"
                ),
                params![
                    msg.id,
                    msg.sender_id,
                    msg.recipient_id,
                    msg.encrypted_payload,
                    msg.timestamp,
                    msg.ttl_expires_at
                ],
            )
            .map_err(storage_error)?;
        Ok(())
    }

    pub fn get_message(&self, message_id: &str) -> Result<Option<MessageBlob>> {
        lock(&self.messages)?
            .query_row(
                &format!("SELECT {MESSAGE_COLUMNS} FROM messages WHERE id = ?"),
                [message_id],
                message_row,
            )
            .optional()
            .map_err(storage_error)
    }

    pub fn get_messages_for_peer(&self, peer_id: &str) -> Result<Vec<MessageBlob>> {
        let db = lock(&self.messages)?;
        let mut query = db
            .prepare(&format!(
                "SELECT {MESSAGE_COLUMNS} FROM messages WHERE recipient_id = ? ORDER BY id"
            ))
            .map_err(storage_error)?;
        let messages = query
            .query_map([peer_id], message_row)
            .and_then(Iterator::collect)
            .map_err(storage_error);
        messages
    }

    pub fn delete_message(&self, message_id: &str) -> Result<()> {
        lock(&self.messages)?
            .execute("DELETE FROM messages WHERE id = ?", [message_id])
            .map_err(storage_error)?;
        Ok(())
    }

    /// Verifies a signed profile and stores it if it is newer than the owner's stored version.
    /// A tombstone replaces the profile and is kept so older versions cannot be re-imported.
    pub fn put_profile(&self, record: &SignedRecord) -> Result<VerifiedProfile> {
        let verified = dyapp_profile::verify(record)?;
        // The connection lock serialises the read-compare-write of profile versions.
        let db = lock(&self.profiles)?;
        let current = Self::profile(&db, &verified.peer_id)?
            .map(|stored| dyapp_profile::stored_version(&stored))
            .transpose()?;
        verified.check_newer(current)?;
        db.execute(
            "INSERT OR REPLACE INTO profiles (peer_id, record, live) VALUES (?, ?, ?)",
            params![
                verified.peer_id,
                record.encode_to_vec(),
                !verified.profile.deleted
            ],
        )
        .map_err(storage_error)?;
        Ok(verified)
    }

    fn profile(db: &Connection, peer_id: &str) -> Result<Option<SignedRecord>> {
        db.query_row(
            "SELECT record FROM profiles WHERE peer_id = ?",
            [peer_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)?
        .map(decode)
        .transpose()
    }

    pub fn get_profile(&self, peer_id: &str) -> Result<Option<SignedRecord>> {
        Self::profile(&*lock(&self.profiles)?, peer_id)
    }

    /// Live profiles in key order; tombstones are skipped.
    pub fn list_profiles(&self, skip: u32, limit: u32) -> Result<Vec<SignedRecord>> {
        let db = lock(&self.profiles)?;
        let mut query = db
            .prepare("SELECT record FROM profiles WHERE live ORDER BY peer_id LIMIT ? OFFSET ?")
            .map_err(storage_error)?;
        let records: Vec<Vec<u8>> = query
            .query_map([limit, skip], |row| row.get(0))
            .and_then(Iterator::collect)
            .map_err(storage_error)?;
        records.into_iter().map(decode).collect()
    }

    pub fn cleanup_expired(&self, now: i64) -> Result<usize> {
        lock(&self.messages)?
            .execute("DELETE FROM messages WHERE ttl_expires_at <= ?", [now])
            .map_err(storage_error)
    }

    /// Takes and releases the write lock of every store.
    pub fn health_check(&self) -> Result<()> {
        for db in [&self.profiles, &self.messages] {
            lock(db)?
                .execute_batch("BEGIN IMMEDIATE; ROLLBACK;")
                .map_err(storage_error)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dyapp_identity::Identity;
    use dyapp_profile::Profile;
    use uuid::Uuid;

    fn temp_store() -> BootstrapStore {
        BootstrapStore::new(&format!("/tmp/ai/test-bootstrap-{}", Uuid::new_v4())).unwrap()
    }

    #[test]
    fn test_store_and_retrieve_message() {
        let store = temp_store();

        let msg = MessageBlob {
            id: "msg1".to_string(),
            sender_id: "alice".to_string(),
            recipient_id: "bob".to_string(),
            encrypted_payload: vec![1, 2, 3],
            timestamp: 1000,
            ttl_expires_at: 2000,
        };

        store.store_message(msg).unwrap();
        assert!(store.get_message("msg1").unwrap().is_some());
        assert_eq!(store.get_messages_for_peer("bob").unwrap().len(), 1);
        assert_eq!(store.cleanup_expired(2000).unwrap(), 1);
        assert!(store.get_message("msg1").unwrap().is_none());
    }

    #[test]
    fn signed_profile_versions_and_tombstone() {
        let store = temp_store();
        let owner = Identity::generate();
        let profile = |version| Profile {
            version,
            age: 35,
            ..Profile::default()
        };

        store.put_profile(&profile(2).sign(&owner)).unwrap();
        assert!(matches!(
            store.put_profile(&profile(1).sign(&owner)),
            Err(BootstrapError::Profile(dyapp_profile::Error::Stale))
        ));
        let mut forged = profile(3).sign(&owner);
        forged.payload[1] ^= 1;
        assert!(store.put_profile(&forged).is_err());

        let stored = store.get_profile(&owner.peer_id()).unwrap().unwrap();
        assert_eq!(dyapp_profile::stored_version(&stored), Ok(2));
        assert_eq!(store.list_profiles(0, 10).unwrap().len(), 1);
        assert!(store.list_profiles(1, 10).unwrap().is_empty());

        store
            .put_profile(&Profile::tombstone(3).sign(&owner))
            .unwrap();
        assert!(store.list_profiles(0, 10).unwrap().is_empty());
        assert!(store.put_profile(&profile(2).sign(&owner)).is_err());
    }

    #[test]
    fn data_survives_reopen() {
        let path = format!("/tmp/ai/test-bootstrap-{}", Uuid::new_v4());
        let owner = Identity::generate();
        BootstrapStore::new(&path)
            .unwrap()
            .put_profile(
                &Profile {
                    version: 1,
                    age: 35,
                    ..Profile::default()
                }
                .sign(&owner),
            )
            .unwrap();
        let store = BootstrapStore::new(&path).unwrap();
        assert!(store.get_profile(&owner.peer_id()).unwrap().is_some());
    }

    #[test]
    fn test_health_check() {
        assert!(temp_store().health_check().is_ok());
    }
}
