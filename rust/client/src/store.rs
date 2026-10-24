//! Versioned, identity-bound local SQLite store. The app supplies its private absolute path.

use dyapp_identity::Identity;
use dyapp_messaging::hlc::Timestamp;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

const VERSION: i64 = 1;

/// Why a profile database could not be opened safely.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The app supplied a relative database path.
    #[error("client store path must be absolute")]
    RelativePath,
    /// The database path itself is a symbolic link.
    #[error("client store path is a symbolic link")]
    Symlink,
    /// An existing database has no recognized schema version.
    #[error("client store has an unversioned schema")]
    Unversioned,
    /// The database was written by a newer client.
    #[error("client store schema version {0} is unsupported")]
    FutureVersion(i64),
    /// The selected identity differs from the database's bound identity.
    #[error("client store belongs to another identity")]
    WrongIdentity,
    /// A stored setting has an unsupported format or value.
    #[error("client store setting is invalid")]
    InvalidSetting,
    /// A clock value does not fit the SQLite integer representation.
    #[error("client store clock value is out of range")]
    ClockRange,
    /// SQLite refused the database or a migration.
    #[error("client store: {0}")]
    Sql(#[from] rusqlite::Error),
}

/// One profile's durable state; a second profile requires a separate database path.
pub struct Store {
    db: Connection,
}

impl Store {
    /// Open a profile database without replacing an unknown or mismatched one.
    pub fn open(path: &Path, identity: &Identity) -> Result<Self, Error> {
        if !path.is_absolute() {
            return Err(Error::RelativePath);
        }
        if path.is_symlink() {
            return Err(Error::Symlink);
        }
        let mut db = Connection::open(path)?;
        db.execute_batch(
            "PRAGMA journal_mode = WAL; PRAGMA synchronous = FULL; PRAGMA foreign_keys = ON;",
        )?;
        let version: i64 = db.pragma_query_value(None, "user_version", |row| row.get(0))?;
        match version {
            0 => {
                let tables: i64 = db.query_row(
                    "SELECT COUNT(*) FROM `sqlite_master` WHERE `name` NOT LIKE 'sqlite_%'",
                    [],
                    |row| row.get(0),
                )?;
                if tables != 0 {
                    return Err(Error::Unversioned);
                }
                let tx = db.transaction()?;
                tx.execute_batch(SCHEMA_V1)?;
                tx.execute(
                    "INSERT INTO `profile_binding` (`slot`, `peer_id`) VALUES (1, ?1)",
                    params![identity.peer_id()],
                )?;
                tx.commit()?;
            }
            VERSION => {}
            other => return Err(Error::FutureVersion(other)),
        }
        let bound: String = db.query_row(
            "SELECT `peer_id` FROM `profile_binding` WHERE `slot` = 1",
            [],
            |row| row.get(0),
        )?;
        if bound != identity.peer_id() {
            return Err(Error::WrongIdentity);
        }
        Ok(Self { db })
    }

    /// Whether this profile sends optional view signals; absent means off.
    pub fn send_profile_views(&self) -> Result<bool, Error> {
        let value: Option<(i64, Vec<u8>)> = self
            .db
            .query_row(
                "SELECT `format_version`, `value` FROM `settings` WHERE `key` = ?1",
                ["send_profile_views"],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        match value {
            None => Ok(false),
            Some((1, value)) if value == [0] => Ok(false),
            Some((1, value)) if value == [1] => Ok(true),
            _ => Err(Error::InvalidSetting),
        }
    }

    /// Persist this profile's choice to send optional view signals.
    pub fn set_send_profile_views(&self, enabled: bool) -> Result<(), Error> {
        self.db.execute(
            "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, 1, ?2)
             ON CONFLICT (`key`) DO UPDATE SET `format_version` = 1, `value` = excluded.`value`",
            params!["send_profile_views", [u8::from(enabled)]],
        )?;
        Ok(())
    }

    /// Whether this profile currently passes on `peer_id`; absent means no pass.
    pub fn passed(&self, peer_id: &str) -> Result<bool, Error> {
        let value: Option<i64> = self
            .db
            .query_row(
                "SELECT `passed` FROM `pass_history` WHERE `peer_id` = ?1",
                [peer_id],
                |row| row.get(0),
            )
            .optional()?;
        match value {
            None | Some(0) => Ok(false),
            Some(1) => Ok(true),
            _ => Err(Error::InvalidSetting),
        }
    }

    /// Set or undo a local pass. An older event cannot overwrite a newer one.
    /// Returns whether the stored state changed.
    pub fn set_pass(&self, peer_id: &str, passed: bool, at: Timestamp) -> Result<bool, Error> {
        let at = i64::try_from(u64::from(at)).map_err(|_| Error::ClockRange)?;
        let changed = self.db.execute(
            "INSERT INTO `pass_history` (`peer_id`, `passed`, `updated_hlc`) VALUES (?1, ?2, ?3)
             ON CONFLICT (`peer_id`) DO UPDATE SET `passed` = excluded.`passed`,
             `updated_hlc` = excluded.`updated_hlc`
             WHERE excluded.`updated_hlc` > `pass_history`.`updated_hlc`",
            params![peer_id, i64::from(passed), at],
        )?;
        Ok(changed != 0)
    }
}

const SCHEMA_V1: &str = r"
CREATE TABLE `profile_binding` (
    `slot` INTEGER PRIMARY KEY CHECK (`slot` = 1),
    `peer_id` TEXT NOT NULL
) STRICT;
CREATE TABLE `mls_state` (
    `slot` INTEGER PRIMARY KEY CHECK (`slot` = 1),
    `sealed` BLOB NOT NULL,
    `revision` INTEGER NOT NULL CHECK (`revision` >= 0)
) STRICT;
CREATE TABLE `inbox` (
    `mailbox` BLOB NOT NULL CHECK (length(`mailbox`) = 32),
    `envelope_id` BLOB NOT NULL CHECK (length(`envelope_id`) = 16),
    `kind` INTEGER NOT NULL,
    `payload` BLOB,
    `ack_pending` INTEGER NOT NULL CHECK (`ack_pending` IN (0, 1)),
    PRIMARY KEY (`mailbox`, `envelope_id`)
) STRICT;
CREATE TABLE `outbox` (
    `mailbox` BLOB NOT NULL CHECK (length(`mailbox`) = 32),
    `envelope_id` BLOB NOT NULL CHECK (length(`envelope_id`) = 16),
    `signed_envelope` BLOB NOT NULL,
    `retry_at_ms` INTEGER NOT NULL,
    `attempts` INTEGER NOT NULL CHECK (`attempts` >= 0),
    PRIMARY KEY (`mailbox`, `envelope_id`)
) STRICT;
CREATE TABLE `settings` (
    `key` TEXT PRIMARY KEY,
    `format_version` INTEGER NOT NULL CHECK (`format_version` > 0),
    `value` BLOB NOT NULL
) STRICT;
CREATE TABLE `pass_history` (
    `peer_id` TEXT PRIMARY KEY,
    `passed` INTEGER NOT NULL CHECK (`passed` IN (0, 1)),
    `updated_hlc` INTEGER NOT NULL
) STRICT;
PRAGMA user_version = 1;
";

#[cfg(test)]
mod tests {
    use super::{Error, Store};
    use dyapp_identity::Identity;
    use dyapp_messaging::hlc::Timestamp;
    use rusqlite::Connection;
    use std::path::PathBuf;

    fn temp_db_path() -> PathBuf {
        let mut random = [0; 16];
        getrandom::fill(&mut random).unwrap();
        std::env::temp_dir().join(format!(
            "dyapp-client-store-{:032x}.db",
            u128::from_be_bytes(random)
        ))
    }

    #[test]
    fn binds_database_to_one_identity_across_reopens() {
        let path = temp_db_path();
        let alice = Identity::generate();
        let bob = Identity::generate();
        let store = Store::open(&path, &alice).unwrap();
        drop(store);
        assert!(matches!(
            Store::open(&path, &bob),
            Err(Error::WrongIdentity)
        ));
        drop(Store::open(&path, &alice).unwrap());
    }

    #[test]
    fn refuses_future_and_unknown_schemas_without_replacing_them() {
        let path = temp_db_path();
        let alice = Identity::generate();
        drop(Store::open(&path, &alice).unwrap());
        let db = Connection::open(&path).unwrap();
        db.execute_batch("PRAGMA user_version = 2").unwrap();
        drop(db);
        assert!(matches!(
            Store::open(&path, &alice),
            Err(Error::FutureVersion(2))
        ));

        let unknown = temp_db_path();
        let db = Connection::open(&unknown).unwrap();
        db.execute_batch("CREATE TABLE `unknown` (`value` INTEGER)")
            .unwrap();
        drop(db);
        assert!(matches!(
            Store::open(&unknown, &alice),
            Err(Error::Unversioned)
        ));
    }

    #[test]
    fn refuses_relative_path_and_corrupt_database() {
        let alice = Identity::generate();
        assert!(matches!(
            Store::open(std::path::Path::new("client.db"), &alice),
            Err(Error::RelativePath)
        ));
        let path = temp_db_path();
        std::fs::write(&path, b"not a SQLite database").unwrap();
        assert!(matches!(Store::open(&path, &alice), Err(Error::Sql(_))));
    }

    #[test]
    fn settings_and_pass_history_survive_restart_without_crossing_profiles() {
        let path = temp_db_path();
        let alice = Identity::generate();
        let bob = Identity::generate();
        let peer = Identity::generate().peer_id();
        let store = Store::open(&path, &alice).unwrap();
        assert!(!store.send_profile_views().unwrap());
        assert!(!store.passed(&peer).unwrap());
        store.set_send_profile_views(true).unwrap();
        assert!(store.set_pass(&peer, true, Timestamp::new(10, 0)).unwrap());
        assert!(!store.set_pass(&peer, false, Timestamp::new(9, 0)).unwrap());
        drop(store);

        let store = Store::open(&path, &alice).unwrap();
        assert!(store.send_profile_views().unwrap());
        assert!(store.passed(&peer).unwrap());
        assert!(store.set_pass(&peer, false, Timestamp::new(11, 0)).unwrap());
        assert!(!store.passed(&peer).unwrap());
        let other = Store::open(&temp_db_path(), &bob).unwrap();
        assert!(!other.send_profile_views().unwrap());
        assert!(!other.passed(&peer).unwrap());
    }

    #[test]
    fn unknown_setting_format_fails_closed() {
        let store = Store::open(&temp_db_path(), &Identity::generate()).unwrap();
        store
            .db
            .execute(
                "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, 2, ?2)",
                rusqlite::params!["send_profile_views", vec![1_u8]],
            )
            .unwrap();
        assert!(matches!(
            store.send_profile_views(),
            Err(Error::InvalidSetting)
        ));
    }
}
