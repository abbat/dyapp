use crate::error::{BootstrapError, Result};
use dyapp_identity::SignedRecord;
use dyapp_profile::VerifiedProfile;
use prost::Message;
use rocksdb::{Direction, IteratorMode, DB};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageBlob {
    pub id: String,
    pub sender_id: String,
    pub recipient_id: String,
    pub encrypted_payload: Vec<u8>,
    pub timestamp: i64,
    pub ttl_expires_at: i64,
}

pub struct BootstrapStore {
    db: Arc<DB>,
    // Serialises the read-compare-write of profile versions.
    profile_lock: Mutex<()>,
}

fn storage_error(error: impl ToString) -> BootstrapError {
    BootstrapError::StorageError(error.to_string())
}

impl BootstrapStore {
    pub fn new(path: &str) -> Result<Self> {
        let db = DB::open_default(path).map_err(storage_error)?;

        Ok(Self {
            db: Arc::new(db),
            profile_lock: Mutex::new(()),
        })
    }

    pub fn store_message(&self, msg: MessageBlob) -> Result<()> {
        let key = format!("msg:{}", msg.id);
        let value = serde_json::to_vec(&msg)
            .map_err(|e| BootstrapError::SerializationError(e.to_string()))?;

        self.db.put(key.as_bytes(), &value).map_err(storage_error)?;

        Ok(())
    }

    pub fn get_message(&self, message_id: &str) -> Result<Option<MessageBlob>> {
        let key = format!("msg:{}", message_id);
        match self.db.get(key.as_bytes()) {
            Ok(Some(value)) => {
                let msg = serde_json::from_slice(&value)
                    .map_err(|e| BootstrapError::SerializationError(e.to_string()))?;
                Ok(Some(msg))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(storage_error(e)),
        }
    }

    pub fn get_messages_for_peer(&self, peer_id: &str) -> Result<Vec<MessageBlob>> {
        let prefix = "msg:";
        let mut messages = Vec::new();

        let iter = self
            .db
            .iterator(IteratorMode::From(prefix.as_bytes(), Direction::Forward));
        for entry in iter {
            let (key, value) = entry.map_err(storage_error)?;
            if !key.starts_with(prefix.as_bytes()) {
                break;
            }
            if let Ok(msg) = serde_json::from_slice::<MessageBlob>(&value) {
                if msg.recipient_id == peer_id {
                    messages.push(msg);
                }
            }
        }

        Ok(messages)
    }

    pub fn delete_message(&self, message_id: &str) -> Result<()> {
        let key = format!("msg:{}", message_id);
        self.db.delete(key.as_bytes()).map_err(storage_error)?;
        Ok(())
    }

    /// Verifies a signed profile and stores it if it is newer than the owner's stored version.
    /// A tombstone replaces the profile and is kept so older versions cannot be re-imported.
    pub fn put_profile(&self, record: &SignedRecord) -> Result<VerifiedProfile> {
        let _guard = self.profile_lock.lock().map_err(storage_error)?;
        let verified = dyapp_profile::verify(record)?;
        let current = self
            .get_profile(&verified.peer_id)?
            .map(|stored| dyapp_profile::stored_version(&stored))
            .transpose()?;
        verified.check_newer(current)?;
        self.db
            .put(
                format!("profile:{}", verified.peer_id),
                record.encode_to_vec(),
            )
            .map_err(storage_error)?;
        Ok(verified)
    }

    pub fn get_profile(&self, peer_id: &str) -> Result<Option<SignedRecord>> {
        self.db
            .get(format!("profile:{peer_id}"))
            .map_err(storage_error)?
            .map(|value| SignedRecord::decode(value.as_slice()))
            .transpose()
            .map_err(|e| BootstrapError::SerializationError(e.to_string()))
    }

    /// Live profiles in key order; tombstones are skipped.
    pub fn list_profiles(&self, skip: u32, limit: u32) -> Result<Vec<SignedRecord>> {
        let prefix = "profile:";
        let iter = self
            .db
            .iterator(IteratorMode::From(prefix.as_bytes(), Direction::Forward));
        let mut profiles = Vec::new();
        for entry in iter {
            let (key, value) = entry.map_err(storage_error)?;
            if !key.starts_with(prefix.as_bytes()) {
                break;
            }
            let Ok(record) = SignedRecord::decode(&value[..]) else {
                continue;
            };
            if dyapp_profile::verify(&record).is_ok_and(|v| !v.profile.deleted) {
                profiles.push(record);
            }
        }
        Ok(profiles
            .into_iter()
            .skip(skip as usize)
            .take(limit as usize)
            .collect())
    }

    pub fn cleanup_expired(&self, now: i64) -> Result<usize> {
        let mut keys_to_delete = Vec::new();
        let prefix = "msg:";
        let iter = self
            .db
            .iterator(IteratorMode::From(prefix.as_bytes(), Direction::Forward));
        for entry in iter {
            let (key, value) = entry.map_err(storage_error)?;
            if !key.starts_with(prefix.as_bytes()) {
                break;
            }
            if let Ok(msg) = serde_json::from_slice::<MessageBlob>(&value) {
                if msg.ttl_expires_at <= now {
                    keys_to_delete.push(key.to_vec());
                }
            }
        }

        for key in &keys_to_delete {
            self.db.delete(key).map_err(storage_error)?;
        }

        Ok(keys_to_delete.len())
    }

    pub fn health_check(&self) -> Result<()> {
        let test_key = format!("health:{}", Uuid::new_v4());
        self.db
            .put(test_key.as_bytes(), b"ok")
            .map_err(storage_error)?;
        self.db.delete(test_key.as_bytes()).map_err(storage_error)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dyapp_identity::Identity;
    use dyapp_profile::Profile;

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

        store
            .put_profile(&Profile::tombstone(3).sign(&owner))
            .unwrap();
        assert!(store.list_profiles(0, 10).unwrap().is_empty());
        assert!(store.put_profile(&profile(2).sign(&owner)).is_err());
    }

    #[test]
    fn test_health_check() {
        assert!(temp_store().health_check().is_ok());
    }
}
