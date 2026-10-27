//! Versioned, identity-bound local SQLite store. The app supplies its private absolute path.

use crate::PreparedEnvelope;
use dyapp_identity::Identity;
use dyapp_messaging::hlc::Timestamp;
use dyapp_messaging::mls::{Device, Group};
use dyapp_profile::VerifiedProfile;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

mod inbox;
mod likes;
pub mod messages;
mod outbox;
mod peers;
mod presence;
mod profile_draft;
mod search_publish;
mod tokens;
pub use likes::Likes;
pub use peers::ContactName;

const VERSION: i64 = 11;

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
    /// A prepared write conflicts with stored bytes or an outbox row is corrupt.
    #[error("client store outbox record conflicts or is corrupt")]
    OutboxConflict,
    /// A group ID is empty or already belongs to another peer.
    #[error("client store group record conflicts or is corrupt")]
    GroupConflict,
    /// A cataloged group is absent from the sealed Device or lacks its recorded peer.
    #[error("client store group is missing or belongs to another peer")]
    GroupMismatch,
    /// MLS rejected a stored group.
    #[error("client store group: {0}")]
    Mls(#[from] dyapp_messaging::mls::Error),
    /// A caller tried to persist an empty sealed MLS snapshot.
    #[error("client store MLS snapshot is empty")]
    EmptyMlsState,
    /// A chat message or inbox row conflicts or is corrupt.
    #[error("client store message or inbox record conflicts or is corrupt")]
    InboxConflict,
    /// An interaction reuses an event ID with other bytes, or does not involve this profile.
    #[error("client store interaction conflicts or names another profile")]
    InteractionConflict,
    /// A received timestamp is too far ahead of the local clock.
    #[error("client store interaction: {0}")]
    ClockSkew(#[from] dyapp_messaging::hlc::ClockSkew),
    /// A token row is missing, opens under another key, or names another contact.
    #[error("client store write token is missing, conflicts or is corrupt")]
    TokenConflict,
    /// A local contact name is longer than 64 characters or has hidden characters.
    #[error("client store local name is too long or not clean text")]
    InvalidName,
    /// The system random source failed.
    #[error("client store could not read the system random source")]
    Random,
    /// SQLite refused the database or a migration.
    #[error("client store: {0}")]
    Sql(#[from] rusqlite::Error),
}

/// One profile's durable state; a second profile requires a separate database path.
pub struct Store {
    db: Connection,
    owner_peer_id: String,
}

/// The device mailbox and envelope ID that identify one received delivery.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct InboxKey {
    /// The receiving device's mailbox address.
    pub mailbox: [u8; 32],
    /// The sender's envelope ID.
    pub id: [u8; 16],
}

impl std::fmt::Debug for InboxKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InboxKey").finish_non_exhaustive()
    }
}

/// Whether the processed MLS envelope contained a visible message.
#[repr(i64)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum InboxKind {
    /// A message whose versioned payload the app will show from local history.
    Message = 1,
    /// A Welcome, commit or other MLS state change without visible message text.
    StateChange = 2,
    /// An envelope no group accepted; only marks it processed so it is acknowledged once.
    Refused = 3,
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
                tx.execute_batch(messages::SCHEMA)?;
                tx.execute(
                    "INSERT INTO `profile_binding` (`slot`, `peer_id`) VALUES (1, ?1)",
                    params![identity.peer_id()],
                )?;
                tx.commit()?;
            }
            1..=VERSION => {}
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
        if version < VERSION {
            let tx = db.transaction()?;
            if version < 2 {
                tx.execute_batch(SCHEMA_V2)?;
            }
            if version < 3 {
                tx.execute_batch(SCHEMA_V3)?;
            }
            if version < 4 {
                tx.execute_batch(SCHEMA_V4)?;
            }
            if version < 5 {
                tx.execute_batch(SCHEMA_V5)?;
            }
            if version < 6 {
                tx.execute_batch(outbox::SCHEMA_V6)?;
            }
            if version < 7 {
                tx.execute_batch(outbox::SCHEMA_V7)?;
            }
            if version < 8 {
                tx.execute_batch(peers::SCHEMA_V8)?;
            }
            if version < 9 {
                tx.execute_batch(peers::SCHEMA_V9)?;
            }
            if version < 10 {
                tx.execute_batch(peers::SCHEMA_V10)?;
            }
            tx.execute_batch(presence::SCHEMA_V11)?;
            tx.commit()?;
        }
        Ok(Self {
            db,
            owner_peer_id: bound,
        })
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

    /// Commit a new sealed MLS snapshot and exact mailbox writes before any network send.
    pub fn stage_outbound(
        &mut self,
        sealed_device: &[u8],
        writes: &[PreparedEnvelope],
    ) -> Result<(), Error> {
        self.stage_outbound_inner(None, sealed_device, writes, true)
    }

    /// Register a new conversation in the same commit as its MLS state and Welcome writes.
    pub fn stage_group(
        &mut self,
        group: &Group,
        peer: &VerifiedProfile,
        sealed_device: &[u8],
        writes: &[PreparedEnvelope],
    ) -> Result<(), Error> {
        if group.id().is_empty()
            || peer.peer_id == self.owner_peer_id
            || !group.members()?.contains(&peer.peer_id)
        {
            return Err(Error::GroupConflict);
        }
        self.stage_outbound_inner(
            Some((group.id(), &peer.peer_id)),
            sealed_device,
            writes,
            true,
        )
    }

    /// IDs needed to reopen every conversation from the saved Device snapshot.
    pub fn known_groups(&self) -> Result<Vec<(Vec<u8>, String)>, Error> {
        let mut query = self.db.prepare(
            "SELECT `group_id`, `peer_id` FROM `conversation_groups` ORDER BY `group_id`",
        )?;
        let rows = query.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect::<Result<_, _>>().map_err(Error::from)
    }

    /// Reopen and check every cataloged group against the restored sealed Device.
    pub fn load_groups(&self, device: &Device) -> Result<Vec<(Group, String)>, Error> {
        self.known_groups()?
            .into_iter()
            .map(|(id, peer_id)| {
                let group = Group::load(device, &id)?.ok_or(Error::GroupMismatch)?;
                if peer_id == self.owner_peer_id || !group.members()?.contains(&peer_id) {
                    return Err(Error::GroupMismatch);
                }
                Ok((group, peer_id))
            })
            .collect()
    }

    fn stage_outbound_inner(
        &mut self,
        group: Option<(&[u8], &str)>,
        sealed_device: &[u8],
        writes: &[PreparedEnvelope],
        awaits_receipt: bool,
    ) -> Result<(), Error> {
        let tx = self.db.transaction()?;
        Self::stage_outbound_tx(&tx, group, sealed_device, writes, awaits_receipt)?;
        tx.commit()?;
        Ok(())
    }

    /// Stage the sealed MLS snapshot and exact writes inside a caller-owned transaction.
    fn stage_outbound_tx(
        tx: &rusqlite::Transaction<'_>,
        group: Option<(&[u8], &str)>,
        sealed_device: &[u8],
        writes: &[PreparedEnvelope],
        awaits_receipt: bool,
    ) -> Result<(), Error> {
        if sealed_device.is_empty() {
            return Err(Error::EmptyMlsState);
        }
        if let Some((group_id, peer_id)) = group {
            catalog(tx, group_id, peer_id)?;
        }
        tx.execute(
            "INSERT INTO `mls_state` (`slot`, `sealed`, `revision`) VALUES (1, ?1, 1)
             ON CONFLICT (`slot`) DO UPDATE SET `sealed` = excluded.`sealed`,
             `revision` = `mls_state`.`revision` + 1",
            [sealed_device],
        )?;
        for write in writes {
            let inserted = tx.execute(
                "INSERT INTO `outbox` (`mailbox`, `envelope_id`, `signed_envelope`,
                 `retry_at_ms`, `attempts`, `awaits_receipt`) VALUES (?1, ?2, ?3, 0, 0, ?4)
                 ON CONFLICT (`mailbox`, `envelope_id`) DO NOTHING",
                params![
                    write.mailbox().as_slice(),
                    write.id().as_slice(),
                    write.as_bytes(),
                    awaits_receipt
                ],
            )?;
            if inserted == 0 {
                let saved: Vec<u8> = tx.query_row(
                    "SELECT `signed_envelope` FROM `outbox` WHERE `mailbox` = ?1 AND `envelope_id` = ?2",
                    params![write.mailbox().as_slice(), write.id().as_slice()],
                    |row| row.get(0),
                )?;
                if saved != write.as_bytes() {
                    return Err(Error::OutboxConflict);
                }
            }
        }
        Ok(())
    }

    /// The last sealed MLS snapshot, if one was stored; unwrap its key outside SQLite.
    pub fn sealed_device(&self) -> Result<Option<Vec<u8>>, Error> {
        Ok(self
            .db
            .query_row(
                "SELECT `sealed` FROM `mls_state` WHERE `slot` = 1",
                [],
                |row| row.get(0),
            )
            .optional()?)
    }

    /// Pending writes due by `now_ms`, reopened from exact signed bytes.
    pub fn due_outbound(&self, now_ms: i64) -> Result<Vec<PreparedEnvelope>, Error> {
        let mut query = self.db.prepare(
            "SELECT `mailbox`, `envelope_id`, `signed_envelope` FROM `outbox`
             WHERE `retry_at_ms` <= ?1 ORDER BY `retry_at_ms`, `mailbox`, `envelope_id`",
        )?;
        let rows = query.query_map([now_ms], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, Vec<u8>>(2)?,
            ))
        })?;
        rows.map(|row| {
            let (mailbox, id, bytes) = row?;
            let write = PreparedEnvelope::from_bytes(bytes).map_err(|_| Error::OutboxConflict)?;
            if write.mailbox().as_slice() != mailbox || write.id().as_slice() != id {
                return Err(Error::OutboxConflict);
            }
            Ok(write)
        })
        .collect()
    }

    /// The Peer ID this database is bound to.
    pub fn owner_peer_id(&self) -> &str {
        &self.owner_peer_id
    }
}

/// Add a conversation, or confirm an existing one belongs to the same peer.
fn catalog(tx: &rusqlite::Transaction, group_id: &[u8], peer_id: &str) -> Result<(), Error> {
    let inserted = tx.execute(
        "INSERT INTO `conversation_groups` (`group_id`, `peer_id`) VALUES (?1, ?2)
         ON CONFLICT (`group_id`) DO NOTHING",
        params![group_id, peer_id],
    )?;
    if inserted == 0 {
        let saved: String = tx.query_row(
            "SELECT `peer_id` FROM `conversation_groups` WHERE `group_id` = ?1",
            [group_id],
            |row| row.get(0),
        )?;
        if saved != peer_id {
            return Err(Error::GroupConflict);
        }
    }
    Ok(())
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

const SCHEMA_V2: &str = r"
CREATE TABLE `conversation_groups` (
    `group_id` BLOB PRIMARY KEY CHECK (length(`group_id`) > 0),
    `peer_id` TEXT NOT NULL CHECK (length(`peer_id`) > 0)
) STRICT;
PRAGMA user_version = 2;
";

const SCHEMA_V3: &str = r"
CREATE TABLE `interaction_ledger` (
    `sender` TEXT NOT NULL CHECK (length(`sender`) > 0),
    `event_id` BLOB NOT NULL CHECK (length(`event_id`) = 16),
    `target` TEXT NOT NULL CHECK (length(`target`) > 0),
    `device` BLOB NOT NULL CHECK (length(`device`) > 0),
    `payload` BLOB NOT NULL CHECK (length(`payload`) = 26),
    PRIMARY KEY (`sender`, `event_id`)
) STRICT;
CREATE TABLE `like_state` (
    `sender` TEXT NOT NULL,
    `target` TEXT NOT NULL,
    `liked` INTEGER NOT NULL CHECK (`liked` IN (0, 1)),
    `hlc` INTEGER NOT NULL CHECK (`hlc` >= 0),
    `device` BLOB NOT NULL,
    `event_id` BLOB NOT NULL CHECK (length(`event_id`) = 16),
    PRIMARY KEY (`sender`, `target`)
) STRICT;
PRAGMA user_version = 3;
";

const SCHEMA_V4: &str = r"
CREATE TABLE `view_days` (
    `sender` TEXT NOT NULL,
    `target` TEXT NOT NULL,
    `day` INTEGER NOT NULL CHECK (`day` >= 0),
    PRIMARY KEY (`sender`, `target`, `day`)
) STRICT;
PRAGMA user_version = 4;
";

const SCHEMA_V5: &str = r"
CREATE TABLE `held_tokens` (
    `mailbox` BLOB NOT NULL CHECK (length(`mailbox`) = 32),
    `epoch` INTEGER NOT NULL CHECK (`epoch` >= 0),
    `sealed_key` BLOB NOT NULL,
    `token` BLOB,
    PRIMARY KEY (`mailbox`, `epoch`)
) STRICT;
CREATE TABLE `issued_tokens` (
    `mailbox` BLOB NOT NULL CHECK (length(`mailbox`) = 32),
    `token_key` BLOB NOT NULL CHECK (length(`token_key`) = 32),
    `epoch` INTEGER NOT NULL CHECK (`epoch` >= 0),
    `contact` TEXT NOT NULL CHECK (length(`contact`) > 0),
    `revoked` INTEGER NOT NULL DEFAULT 0 CHECK (`revoked` IN (0, 1)),
    PRIMARY KEY (`mailbox`, `token_key`, `epoch`)
) STRICT;
CREATE INDEX `issued_tokens_contact` ON `issued_tokens` (`contact`);
PRAGMA user_version = 5;
";

#[cfg(test)]
pub(crate) mod tests {
    use super::{Error, InboxKey, InboxKind, Likes, Store, SCHEMA_V1, VERSION};
    use crate::{prepare_put, PreparedEnvelope};
    use dyapp_identity::{Domain, Identity, SignedRecord};
    use dyapp_messaging::hlc::Timestamp;
    use dyapp_messaging::mls::Device;
    use dyapp_p2p_net::proto;
    use dyapp_profile::{Profile, VerifiedProfile};
    use prost::Message as _;
    use rusqlite::Connection;
    use std::path::PathBuf;

    pub(crate) fn temp_db_path() -> PathBuf {
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
        db.pragma_update(None, "user_version", VERSION + 1).unwrap();
        drop(db);
        assert!(matches!(
            Store::open(&path, &alice),
            Err(Error::FutureVersion(v)) if v == VERSION + 1
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
    fn migrates_bound_v1_database_without_losing_records() {
        let path = temp_db_path();
        let alice = Identity::generate();
        let db = Connection::open(&path).unwrap();
        db.execute_batch(SCHEMA_V1).unwrap();
        db.execute(
            "INSERT INTO `profile_binding` (`slot`, `peer_id`) VALUES (1, ?1)",
            [alice.peer_id()],
        )
        .unwrap();
        db.execute(
            "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, 1, ?2)",
            rusqlite::params!["send_profile_views", vec![1_u8]],
        )
        .unwrap();
        drop(db);
        assert!(matches!(
            Store::open(&path, &Identity::generate()),
            Err(Error::WrongIdentity)
        ));
        let db = Connection::open(&path).unwrap();
        assert_eq!(
            db.pragma_query_value::<i64, _>(None, "user_version", |row| row.get(0))
                .unwrap(),
            1
        );
        drop(db);
        let store = Store::open(&path, &alice).unwrap();
        assert!(store.send_profile_views().unwrap());
        assert!(store.known_groups().unwrap().is_empty());
        assert_eq!(store.likes(&alice.peer_id()).unwrap(), Likes::default());
        assert!(store.write_token_epochs(&[0; 32], 0).unwrap().is_empty());
        drop(store);
        let db = Connection::open(&path).unwrap();
        assert_eq!(
            db.pragma_query_value::<i64, _>(None, "user_version", |row| row.get(0))
                .unwrap(),
            VERSION
        );
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
        store.set_send_profile_views(false).unwrap();
        drop(store);
        let store = Store::open(&path, &alice).unwrap();
        assert!(!store.send_profile_views().unwrap());
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
        store
            .db
            .execute(
                "UPDATE `settings` SET `format_version` = 1, `value` = ?1 WHERE `key` = ?2",
                rusqlite::params![vec![2_u8], "send_profile_views"],
            )
            .unwrap();
        assert!(matches!(
            store.send_profile_views(),
            Err(Error::InvalidSetting)
        ));
    }

    fn person() -> (Identity, Device, VerifiedProfile) {
        let identity = Identity::generate();
        let device = Device::new(&identity, Identity::generate().public_key(), 1).unwrap();
        let profile = Profile {
            version: 1,
            devices: vec![device.certificate().clone()],
            ..Default::default()
        }
        .sign(&identity);
        (identity, device, dyapp_profile::verify(&profile).unwrap())
    }

    #[test]
    fn group_catalog_reopens_a_sealed_group_and_rejects_mismatches() {
        let path = temp_db_path();
        let (alice_id, alice, alice_profile) = person();
        let (_, bob, bob_profile) = person();
        let (_, _, carol_profile) = person();
        let package = bob.key_package(false).unwrap();
        let (group, welcome) = alice
            .create_group(&[package], &[alice_profile.clone(), bob_profile.clone()])
            .unwrap();
        let group_id = group.id().to_vec();
        let seal_key = [9; 32];
        let sealed = alice.seal(&seal_key).unwrap();
        let write = prepare_put(&[7; 32], welcome).unwrap();
        let mut failed = Store::open(&temp_db_path(), &alice_id).unwrap();
        failed
            .db
            .execute_batch(
                "CREATE TRIGGER `fail_mls` BEFORE INSERT ON `mls_state`
             BEGIN SELECT RAISE(ABORT, 'test failure'); END",
            )
            .unwrap();
        assert!(matches!(
            failed.stage_group(&group, &bob_profile, &sealed, std::slice::from_ref(&write)),
            Err(Error::Sql(_))
        ));
        assert!(failed.known_groups().unwrap().is_empty());
        assert!(failed.due_outbound(0).unwrap().is_empty());
        let mut store = Store::open(&path, &alice_id).unwrap();
        assert!(matches!(
            store.stage_group(&group, &carol_profile, &sealed, &[]),
            Err(Error::GroupConflict)
        ));
        assert!(store.known_groups().unwrap().is_empty());
        assert!(matches!(
            store.stage_group(&group, &alice_profile, &sealed, &[]),
            Err(Error::GroupConflict)
        ));
        store
            .stage_group(&group, &bob_profile, &sealed, std::slice::from_ref(&write))
            .unwrap();
        drop(store);
        let store = Store::open(&path, &alice_id).unwrap();
        assert_eq!(
            store.known_groups().unwrap(),
            [(group_id.clone(), bob_profile.peer_id.clone())]
        );
        assert_eq!(
            store.due_outbound(0).unwrap()[0].as_bytes(),
            write.as_bytes()
        );
        let opened = Device::open(&store.sealed_device().unwrap().unwrap(), &seal_key).unwrap();
        let loaded = store.load_groups(&opened).unwrap();
        assert_eq!(loaded[0].0.id(), group_id);
        assert_eq!(loaded[0].1, bob_profile.peer_id);
        assert!(matches!(store.load_groups(&bob), Err(Error::GroupMismatch)));
        store
            .db
            .execute(
                "UPDATE `conversation_groups` SET `peer_id` = ?1 WHERE `group_id` = ?2",
                rusqlite::params![carol_profile.peer_id, group_id],
            )
            .unwrap();
        assert!(matches!(
            store.load_groups(&opened),
            Err(Error::GroupMismatch)
        ));
    }

    #[test]
    fn outbound_and_mls_snapshot_survive_restart_and_roll_back_together() {
        let path = temp_db_path();
        let owner = Identity::generate();
        let device = Device::new(&owner, Identity::generate().public_key(), 1).unwrap();
        device.key_package(false).unwrap();
        let seal_key = [3; 32];
        let sealed = device.seal(&seal_key).unwrap();
        let write = prepare_put(&[7; 32], vec![5]).unwrap();
        let mut store = Store::open(&path, &owner).unwrap();
        store
            .stage_outbound(&sealed, std::slice::from_ref(&write))
            .unwrap();
        assert_eq!(
            store.due_outbound(0).unwrap()[0].as_bytes(),
            write.as_bytes()
        );
        store.schedule_retry(&write, 0).unwrap();
        drop(store);

        let mut store = Store::open(&path, &owner).unwrap();
        let saved = store.sealed_device().unwrap().unwrap();
        assert_eq!(
            Device::open(&saved, &seal_key).unwrap().certificate(),
            device.certificate()
        );
        assert!(store.due_outbound(999).unwrap().is_empty());
        assert_eq!(
            store.due_outbound(1_000).unwrap()[0].as_bytes(),
            write.as_bytes()
        );

        let record = SignedRecord::decode(write.as_bytes()).unwrap();
        let mut envelope = proto::Envelope::decode(record.payload.as_slice()).unwrap();
        envelope.ciphertext = vec![9];
        let conflicting = Identity::generate().sign(Domain::Envelope, envelope.encode_to_vec());
        let conflicting = PreparedEnvelope::from_bytes(conflicting.encode_to_vec()).unwrap();
        assert!(matches!(
            store.stage_outbound(b"different state", &[conflicting]),
            Err(Error::OutboxConflict)
        ));
        assert_eq!(store.sealed_device().unwrap(), Some(sealed));
        assert_eq!(
            store.due_outbound(1_000).unwrap()[0].as_bytes(),
            write.as_bytes()
        );
    }

    #[test]
    fn inbound_ack_waits_for_durable_mls_and_deduplication() {
        let path = temp_db_path();
        let owner = Identity::generate();
        let key = InboxKey {
            mailbox: [4; 32],
            id: [5; 16],
        };
        assert_eq!(format!("{key:?}"), "InboxKey { .. }");
        let mut store = Store::open(&path, &owner).unwrap();
        assert!(!store.has_inbox(&key).unwrap());
        assert!(store.pending_acks().unwrap().is_empty());
        assert!(store
            .stage_inbound(b"sealed-v1", &key, InboxKind::Message, b"hi", None)
            .unwrap());
        assert!(store.has_inbox(&key).unwrap());
        assert!(!store
            .stage_inbound(b"stale", &key, InboxKind::Message, b"hi", None)
            .unwrap());
        assert!(matches!(
            store.stage_inbound(b"stale", &key, InboxKind::Message, b"different", None),
            Err(Error::InboxConflict)
        ));
        drop(store);

        let store = Store::open(&path, &owner).unwrap();
        assert_eq!(store.sealed_device().unwrap().unwrap(), b"sealed-v1");
        assert_eq!(store.pending_acks().unwrap(), [key]);
        assert!(store.mark_acked(&key).unwrap());
        assert!(!store.mark_acked(&key).unwrap());
        assert!(store.pending_acks().unwrap().is_empty());
    }

    #[test]
    fn failed_mls_write_rolls_back_inbox_and_ack() {
        let mut store = Store::open(&temp_db_path(), &Identity::generate()).unwrap();
        store
            .db
            .execute_batch(
                "CREATE TRIGGER `fail_mls` BEFORE INSERT ON `mls_state`
                 BEGIN SELECT RAISE(ABORT, 'test failure'); END",
            )
            .unwrap();
        let key = InboxKey {
            mailbox: [1; 32],
            id: [2; 16],
        };
        assert!(matches!(
            store.stage_inbound(b"sealed", &key, InboxKind::StateChange, b"", None),
            Err(Error::Sql(_))
        ));
        assert!(!store.has_inbox(&key).unwrap());
        assert!(store.pending_acks().unwrap().is_empty());
        assert!(store.sealed_device().unwrap().is_none());
    }

    #[test]
    fn outbox_rejects_a_mismatch_in_either_key_part() {
        let mut store = Store::open(&temp_db_path(), &Identity::generate()).unwrap();
        let write = prepare_put(&[7; 32], vec![1]).unwrap();
        store
            .stage_outbound(b"sealed", std::slice::from_ref(&write))
            .unwrap();
        store
            .db
            .execute("UPDATE `outbox` SET `mailbox` = ?1", [vec![8_u8; 32]])
            .unwrap();
        assert!(matches!(store.due_outbound(0), Err(Error::OutboxConflict)));
        store
            .db
            .execute(
                "UPDATE `outbox` SET `mailbox` = ?1",
                [write.mailbox().to_vec()],
            )
            .unwrap();
        store
            .db
            .execute("UPDATE `outbox` SET `envelope_id` = ?1", [vec![9_u8; 16]])
            .unwrap();
        assert!(matches!(store.due_outbound(0), Err(Error::OutboxConflict)));
    }
}
