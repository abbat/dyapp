//! Local block, mute and favorite flags, contact names, passes and verified marks per peer. Never
//! published; only the owner's other devices hear of them, and the newest change wins there.

use super::likes::{load_hlc, save_hlc};
use super::{Error, Store};
use crate::chat::PeerChange;
use crate::PreparedEnvelope;
use dyapp_messaging::hlc::{HybridClock, Timestamp};
use dyapp_profile::{is_clean_text, DeviceCertificate, VerifiedProfile, MAX_FIELD_CHARS};
use prost::Message as _;
use rusqlite::types::Value;
use rusqlite::{params, Connection, OptionalExtension};

// `seen_devices`: the device keys of the newest profile seen or verified, 32 bytes each; NULL
// before the first. `seen_version`: that profile's version. `verified`: the safety number or QR
// code was compared for those keys. `new_device`: a profile added a device, until acknowledged.
// Each *_hlc column keeps the newest private field change shared with own devices.
pub(super) const SCHEMA: &str = r"
CREATE TABLE `peers` (
    `peer_id` TEXT PRIMARY KEY NOT NULL,
    `blocked` INTEGER NOT NULL DEFAULT 0 CHECK (`blocked` IN (0, 1)),
    `muted` INTEGER NOT NULL DEFAULT 0 CHECK (`muted` IN (0, 1)),
    `local_name` TEXT NOT NULL DEFAULT '',
    `seen_devices` BLOB,
    `seen_version` INTEGER,
    `verified` INTEGER NOT NULL DEFAULT 0 CHECK (`verified` IN (0, 1)),
    `new_device` INTEGER NOT NULL DEFAULT 0 CHECK (`new_device` IN (0, 1)),
    `blocked_hlc` INTEGER NOT NULL DEFAULT 0,
    `muted_hlc` INTEGER NOT NULL DEFAULT 0,
    `name_hlc` INTEGER NOT NULL DEFAULT 0,
    `favorite` INTEGER NOT NULL DEFAULT 0 CHECK (`favorite` IN (0, 1)),
    `favorite_hlc` INTEGER NOT NULL DEFAULT 0,
    `blocked_device` BLOB NOT NULL DEFAULT x'',
    `muted_device` BLOB NOT NULL DEFAULT x'',
    `name_device` BLOB NOT NULL DEFAULT x'',
    `favorite_device` BLOB NOT NULL DEFAULT x'',
    `note` TEXT NOT NULL DEFAULT '',
    `note_hlc` INTEGER NOT NULL DEFAULT 0,
    `note_device` BLOB NOT NULL DEFAULT x''
) STRICT;
";

/// The name an app shows for a peer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContactName {
    /// The name this profile gave the peer; it replaces the public name everywhere.
    Local(String),
    /// The peer's own public name, shown with the verified or unverified mark.
    Public {
        /// The profile's `name` field.
        name: String,
        /// Whether the safety number or QR code was compared and no device was added since.
        verified: bool,
    },
    /// Neither a local nor a public name.
    Unnamed,
}

/// Merge by HLC, authenticated device key, then value for conflicting clock reuse.
fn apply_change(
    db: &Connection,
    peer_id: &str,
    change: &PeerChange,
    at: Timestamp,
    source_device: &[u8],
) -> Result<bool, Error> {
    let (column, stamp, actor, value) = match change {
        PeerChange::Blocked(on) => ("blocked", "blocked_hlc", "blocked_device", Value::from(*on)),
        PeerChange::Muted(on) => ("muted", "muted_hlc", "muted_device", Value::from(*on)),
        PeerChange::Favorite(on) => (
            "favorite",
            "favorite_hlc",
            "favorite_device",
            Value::from(*on),
        ),
        PeerChange::Name(name) => (
            "local_name",
            "name_hlc",
            "name_device",
            Value::from(name.clone()),
        ),
        PeerChange::Note(note) => {
            if !crate::chat::valid_note(note) {
                return Err(Error::InvalidSetting);
            }
            ("note", "note_hlc", "note_device", Value::from(note.clone()))
        }
        PeerChange::Passed(on) => return write_pass(db, peer_id, *on, at, source_device),
    };
    let at = i64::try_from(u64::from(at)).map_err(|_| Error::ClockRange)?;
    // Column identifiers are the literals above, never caller input.
    let changed = db.execute(
        &format!(
            "INSERT INTO `peers` (`peer_id`, `{column}`, `{stamp}`, `{actor}`) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (`peer_id`) DO UPDATE SET `{column}` = excluded.`{column}`,
             `{stamp}` = excluded.`{stamp}`, `{actor}` = excluded.`{actor}`
             WHERE (excluded.`{stamp}`, excluded.`{actor}`, excluded.`{column}`)
                 > (`peers`.`{stamp}`, `peers`.`{actor}`, `peers`.`{column}`)"
        ),
        params![peer_id, value, at, source_device],
    )?;
    Ok(changed != 0)
}

fn write_pass(
    db: &Connection,
    peer_id: &str,
    passed: bool,
    at: Timestamp,
    source_device: &[u8],
) -> Result<bool, Error> {
    let at = i64::try_from(u64::from(at)).map_err(|_| Error::ClockRange)?;
    let changed = db.execute(
        "INSERT INTO `pass_history` (`peer_id`, `passed`, `updated_hlc`, `updated_device`) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (`peer_id`) DO UPDATE SET `passed` = excluded.`passed`,
         `updated_hlc` = excluded.`updated_hlc`, `updated_device` = excluded.`updated_device`
         WHERE (excluded.`updated_hlc`, excluded.`updated_device`, excluded.`passed`)
             > (`pass_history`.`updated_hlc`, `pass_history`.`updated_device`, `pass_history`.`passed`)",
        params![peer_id, i64::from(passed), at, source_device],
    )?;
    Ok(changed != 0)
}

impl Store {
    /// Set or undo a local pass. An older event cannot overwrite a newer one.
    /// Returns whether the stored state changed.
    pub fn set_pass(&self, peer_id: &str, passed: bool, at: Timestamp) -> Result<bool, Error> {
        write_pass(&self.db, peer_id, passed, at, &[])
    }

    /// The next own clock value.
    pub fn tick(&mut self, wall_ms: u64) -> Result<Timestamp, Error> {
        let at = HybridClock::resume(load_hlc(&self.db)?).now_at(wall_ms);
        let tx = self.db.transaction()?;
        save_hlc(&tx, at)?;
        tx.commit()?;
        Ok(at)
    }

    /// Apply `change`, made here at `at`, on a device with no own group to send it in.
    pub fn apply_peer_change(
        &self,
        peer_id: &str,
        change: &PeerChange,
        at: Timestamp,
        source_device: &[u8],
    ) -> Result<(), Error> {
        apply_change(&self.db, peer_id, change, at, source_device).map(drop)
    }

    /// Apply `change`, made here at `hlc`, in the commit that stages it for the own devices.
    pub fn stage_peer_change(
        &mut self,
        peer_id: &str,
        change: &PeerChange,
        hlc: u64,
        sealed_device: &[u8],
        writes: &[PreparedEnvelope],
        source_device: &[u8],
    ) -> Result<(), Error> {
        let tx = self.db.transaction()?;
        apply_change(&tx, peer_id, change, Timestamp::from(hlc), source_device)?;
        Self::stage_outbound_tx(&tx, None, sealed_device, writes, true)?;
        tx.commit()?;
        Ok(())
    }

    /// Apply `change` from another own device and move the clock past it, unless a newer change
    /// is held. Returns whether it applied.
    pub fn receive_peer_change(
        &mut self,
        peer_id: &str,
        change: &PeerChange,
        hlc: u64,
        wall_ms: u64,
        source_device: &[u8],
    ) -> Result<bool, Error> {
        let at = Timestamp::from(hlc);
        let tx = self.db.transaction()?;
        let clock = HybridClock::resume(load_hlc(&tx)?);
        save_hlc(&tx, clock.observe_at(at, wall_ms)?)?;
        let changed = apply_change(&tx, peer_id, change, at, source_device)?;
        tx.commit()?;
        Ok(changed)
    }

    /// Block or unblock `peer_id` on this profile.
    pub fn set_blocked(&self, peer_id: &str, blocked: bool) -> Result<(), Error> {
        self.set_flag("blocked", peer_id, blocked)
    }

    /// Mute or unmute `peer_id` on this profile.
    pub fn set_muted(&self, peer_id: &str, muted: bool) -> Result<(), Error> {
        self.set_flag("muted", peer_id, muted)
    }

    /// Whether this profile blocks `peer_id`; absent means not blocked.
    pub fn blocked(&self, peer_id: &str) -> Result<bool, Error> {
        self.flag("blocked", peer_id)
    }

    /// Whether this profile mutes `peer_id`; absent means not muted.
    pub fn muted(&self, peer_id: &str) -> Result<bool, Error> {
        self.flag("muted", peer_id)
    }

    /// Whether this identity has saved a peer; blocking hides it without erasing this flag.
    pub fn favorite(&self, peer_id: &str) -> Result<bool, Error> {
        self.flag("favorite", peer_id)
    }

    /// Saved peers visible to the app, excluding blocked peers in stable Peer ID order.
    pub fn favorite_peers(&self) -> Result<Vec<String>, Error> {
        let mut query = self.db.prepare(
            "SELECT `peer_id` FROM `peers` WHERE `favorite` = 1 AND `blocked` = 0 ORDER BY `peer_id`",
        )?;
        let rows = query.query_map([], |row| row.get(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Private note of this identity about a peer; absent means empty.
    pub fn contact_note(&self, peer_id: &str) -> Result<String, Error> {
        Ok(self
            .db
            .query_row(
                "SELECT `note` FROM `peers` WHERE `peer_id` = ?1",
                [peer_id],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or_default())
    }

    /// Name `peer_id` on this profile only; an empty name clears it.
    pub fn set_local_name(&self, peer_id: &str, name: &str) -> Result<(), Error> {
        if name.chars().count() > MAX_FIELD_CHARS || !is_clean_text(name) {
            return Err(Error::InvalidName);
        }
        self.db.execute(
            "INSERT INTO `peers` (`peer_id`, `local_name`) VALUES (?1, ?2)
             ON CONFLICT (`peer_id`) DO UPDATE SET `local_name` = excluded.`local_name`",
            params![peer_id, name],
        )?;
        Ok(())
    }

    /// The name this profile gave `peer_id`; empty means none.
    pub fn local_name(&self, peer_id: &str) -> Result<String, Error> {
        let name = self
            .db
            .query_row(
                "SELECT `local_name` FROM `peers` WHERE `peer_id` = ?1",
                [peer_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(name.unwrap_or_default())
    }

    /// The name to show for the owner of `profile`: the local name, else its public name.
    pub fn contact_name(&self, profile: &VerifiedProfile) -> Result<ContactName, Error> {
        let local = self.local_name(&profile.peer_id)?;
        Ok(if !local.is_empty() {
            ContactName::Local(local)
        } else if !profile.profile.name.is_empty() {
            ContactName::Public {
                name: profile.profile.name.clone(),
                verified: self.verified(profile)?,
            }
        } else {
            ContactName::Unnamed
        })
    }

    /// Mark the owner of `profile` verified after its safety number or QR code matched. This is
    /// a fresh check, so it also ends a new-device notice.
    pub fn set_verified(&self, profile: &VerifiedProfile) -> Result<(), Error> {
        self.db.execute(
            "INSERT INTO `peers` (`peer_id`, `seen_devices`, `seen_version`, `verified`)
             VALUES (?1, ?2, ?3, 1)
             ON CONFLICT (`peer_id`) DO UPDATE SET
                 `seen_devices` = excluded.`seen_devices`,
                 `seen_version` = excluded.`seen_version`,
                 `verified` = 1,
                 `new_device` = 0",
            params![profile.peer_id, device_keys(profile), version(profile)],
        )?;
        Ok(())
    }

    /// Whether the owner of `profile` is verified, after comparing it with the devices seen. The
    /// first profile only records them, and an older one proves nothing. Any other that lists an
    /// unseen device clears the mark and raises the new-device notice in one statement, and those
    /// devices count as seen, so one link raises one notice. A newer profile replaces what was
    /// seen, so a removed device that returns is new again; one of the same version adds to it, so
    /// two signed forks cannot raise the notice in turn.
    pub fn verified(&self, profile: &VerifiedProfile) -> Result<bool, Error> {
        let known: Option<(Vec<u8>, i64, bool)> = self
            .db
            .query_row(
                "SELECT `seen_devices`, `seen_version`, `verified` FROM `peers`
                 WHERE `peer_id` = ?1 AND `seen_devices` IS NOT NULL",
                [&profile.peer_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let (devices, version) = (device_keys(profile), version(profile));
        let Some((seen, seen_version, verified)) = known else {
            self.db.execute(
                "INSERT INTO `peers` (`peer_id`, `seen_devices`, `seen_version`)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT (`peer_id`) DO UPDATE SET
                     `seen_devices` = excluded.`seen_devices`,
                     `seen_version` = excluded.`seen_version`",
                params![profile.peer_id, devices, version],
            )?;
            return Ok(false);
        };
        let added: Vec<u8> = devices
            .chunks(32)
            .filter(|key| !seen.chunks(32).any(|known| known == *key))
            .flatten()
            .copied()
            .collect();
        let unchanged = added.is_empty();
        if version < seen_version || (version == seen_version && unchanged) {
            return Ok(verified);
        }
        let devices = if version == seen_version {
            [seen, added].concat()
        } else {
            devices
        };
        let verified = verified && unchanged;
        self.db.execute(
            "UPDATE `peers`
             SET `seen_devices` = ?2, `seen_version` = ?3, `verified` = ?4,
                 `new_device` = `new_device` OR ?5
             WHERE `peer_id` = ?1",
            params![profile.peer_id, devices, version, verified, !unchanged],
        )?;
        Ok(verified)
    }

    /// Take the verified mark off `peer_id`. The devices seen stay, so the notice still works.
    pub fn clear_verified(&self, peer_id: &str) -> Result<(), Error> {
        self.db.execute(
            "UPDATE `peers` SET `verified` = 0 WHERE `peer_id` = ?1",
            [peer_id],
        )?;
        Ok(())
    }

    /// Whether the chat with `peer_id` shows the new-device notice.
    pub fn new_device(&self, peer_id: &str) -> Result<bool, Error> {
        self.flag("new_device", peer_id)
    }

    /// End the new-device notice for `peer_id` after the user acknowledged it.
    pub fn acknowledge_new_device(&self, peer_id: &str) -> Result<(), Error> {
        self.set_flag("new_device", peer_id, false)
    }

    /// Every peer this profile blocks, in Peer ID order.
    pub fn blocked_peers(&self) -> Result<Vec<String>, Error> {
        let mut query = self
            .db
            .prepare("SELECT `peer_id` FROM `peers` WHERE `blocked` = 1 ORDER BY `peer_id`")?;
        let peers = query
            .query_map([], |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        Ok(peers)
    }

    // `column` is one of the literals above, never caller input.
    fn set_flag(&self, column: &str, peer_id: &str, value: bool) -> Result<(), Error> {
        self.db.execute(
            &format!(
                "INSERT INTO `peers` (`peer_id`, `{column}`) VALUES (?1, ?2)
                 ON CONFLICT (`peer_id`) DO UPDATE SET `{column}` = excluded.`{column}`"
            ),
            params![peer_id, value],
        )?;
        Ok(())
    }

    fn flag(&self, column: &str, peer_id: &str) -> Result<bool, Error> {
        let value = self
            .db
            .query_row(
                &format!("SELECT `{column}` FROM `peers` WHERE `peer_id` = ?1"),
                [peer_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(value.unwrap_or(false))
    }
}

// SQLite integers are signed; a version beyond them still compares as the newest.
fn version(profile: &VerifiedProfile) -> i64 {
    i64::try_from(profile.profile.version).unwrap_or(i64::MAX)
}

// A verified profile's certificates are valid, so every device key is 32 bytes.
fn device_keys(profile: &VerifiedProfile) -> Vec<u8> {
    profile
        .profile
        .devices
        .iter()
        .filter_map(|signed| DeviceCertificate::decode(signed.payload.as_slice()).ok())
        .flat_map(|certificate| certificate.device_key)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::tests::temp_db_path;
    use super::super::{Error, Store};
    use super::ContactName;
    use crate::chat::PeerChange;
    use crate::prepare_put;
    use dyapp_identity::{key_hash, Identity};
    use dyapp_messaging::hlc::Timestamp;
    use dyapp_messaging::mls::Device;
    use dyapp_profile::{Profile, VerifiedProfile};

    #[test]
    fn equal_clock_changes_converge_by_device_and_survive_linking() {
        let owner = Identity::generate();
        let mut left = Store::open(&temp_db_path(), &owner).unwrap();
        let mut right = Store::open(&temp_db_path(), &owner).unwrap();
        let at = u64::from(Timestamp::new(1_000, 0));
        let state = |store: &Store| {
            (
                store.blocked("bob").unwrap(),
                store.muted("bob").unwrap(),
                store.favorite("bob").unwrap(),
                store.local_name("bob").unwrap(),
                store.passed_peers().unwrap(),
                store.contact_note("bob").unwrap(),
            )
        };
        for (a, b) in [
            (PeerChange::Blocked(true), PeerChange::Blocked(false)),
            (PeerChange::Muted(true), PeerChange::Muted(false)),
            (PeerChange::Favorite(true), PeerChange::Favorite(false)),
            (PeerChange::Passed(true), PeerChange::Passed(false)),
            (
                PeerChange::Note("Private".into()),
                PeerChange::Note(String::new()),
            ),
            (
                PeerChange::Name("zulu".into()),
                PeerChange::Name("alpha".into()),
            ),
        ] {
            left.receive_peer_change("bob", &a, at, 1_000, &[1; 32])
                .unwrap();
            left.receive_peer_change("bob", &b, at, 1_000, &[2; 32])
                .unwrap();
            right
                .receive_peer_change("bob", &b, at, 1_000, &[2; 32])
                .unwrap();
            assert!(!right
                .receive_peer_change("bob", &a, at, 1_000, &[1; 32])
                .unwrap());
            assert_eq!(state(&left), state(&right));
            assert!(!left
                .receive_peer_change("bob", &b, at, 1_000, &[2; 32])
                .unwrap());
            let mut copy = Store::open(&temp_db_path(), &owner).unwrap();
            copy.import_link_state(&left.link_state().unwrap()).unwrap();
            assert!(!copy
                .receive_peer_change("bob", &a, at, 1_000, &[1; 32])
                .unwrap());
            assert_eq!(state(&copy), state(&left));
        }
        // A device reusing a timestamp with conflicting values also has a stable fallback.
        let next = u64::from(Timestamp::new(2_000, 0));
        for flag in [false, true] {
            left.receive_peer_change("bob", &PeerChange::Favorite(flag), next, 2_000, &[3; 32])
                .unwrap();
        }
        for flag in [true, false] {
            right
                .receive_peer_change("bob", &PeerChange::Favorite(flag), next, 2_000, &[3; 32])
                .unwrap();
        }
        assert!(left.favorite("bob").unwrap() && right.favorite("bob").unwrap());
    }

    #[test]
    fn the_newest_change_from_any_own_device_wins() {
        let at = |ms| u64::from(Timestamp::new(ms, 0));
        let wall = 1_000_000;
        let alice = Identity::generate();
        let mut store = Store::open(&temp_db_path(), &alice).unwrap();
        for (new, old) in [
            (PeerChange::Blocked(true), PeerChange::Blocked(false)),
            (PeerChange::Muted(true), PeerChange::Muted(false)),
            (PeerChange::Passed(true), PeerChange::Passed(false)),
            (PeerChange::Favorite(true), PeerChange::Favorite(false)),
            (
                PeerChange::Name("New".to_owned()),
                PeerChange::Name("Old".to_owned()),
            ),
        ] {
            assert!(store
                .receive_peer_change("bob", &new, at(2_000), wall, &[2; 32])
                .unwrap());
            for stamp in [at(1_000), at(2_000)] {
                assert!(!store
                    .receive_peer_change("bob", &old, stamp, wall, &[1; 32])
                    .unwrap());
            }
        }
        assert!(store.blocked("bob").unwrap());
        assert!(store.muted("bob").unwrap());
        assert_eq!(store.passed_peers().unwrap(), ["bob"]);
        assert!(store.favorite("bob").unwrap());
        assert_eq!(store.local_name("bob").unwrap(), "New");

        let unblock = PeerChange::Blocked(false);
        assert!(store
            .receive_peer_change("bob", &unblock, at(3_000), wall, &[])
            .unwrap());
        assert!(!store.blocked("bob").unwrap());
        assert!(matches!(
            store.receive_peer_change("bob", &PeerChange::Blocked(true), at(wall * 10), wall, &[]),
            Err(Error::ClockSkew(_))
        ));
        assert!(!store.blocked("bob").unwrap());
        assert!(u64::from(store.tick(0).unwrap()) > at(3_000));

        // A linked device keeps the stamps, so a change older than the copy stays refused.
        let mut linked = Store::open(&temp_db_path(), &alice).unwrap();
        linked
            .import_link_state(&store.link_state().unwrap())
            .unwrap();
        let block = PeerChange::Blocked(true);
        assert!(!linked
            .receive_peer_change("bob", &block, at(2_500), wall, &[])
            .unwrap());
        assert!(!linked.blocked("bob").unwrap());
        assert!(linked.favorite("bob").unwrap());
        assert!(!linked
            .receive_peer_change("bob", &PeerChange::Favorite(false), at(1_500), wall, &[])
            .unwrap());
        assert!(linked.favorite("bob").unwrap());
        assert!(linked
            .receive_peer_change("bob", &PeerChange::Favorite(false), at(3_500), wall, &[])
            .unwrap());
        assert!(!linked.favorite("bob").unwrap());
    }

    #[test]
    fn favorites_survive_reopen_hide_blocks_and_stay_identity_local() {
        let owner = Identity::generate();
        let path = temp_db_path();
        let store = Store::open(&path, &owner).unwrap();
        assert!(!store.favorite("bob").unwrap());
        store
            .apply_peer_change(
                "bob",
                &PeerChange::Favorite(true),
                Timestamp::new(1_000, 0),
                &[],
            )
            .unwrap();
        store
            .apply_peer_change(
                "carol",
                &PeerChange::Favorite(true),
                Timestamp::new(1_001, 0),
                &[],
            )
            .unwrap();
        store.set_blocked("bob", true).unwrap();
        assert_eq!(store.favorite_peers().unwrap(), ["carol"]);
        drop(store);
        let store = Store::open(&path, &owner).unwrap();
        assert!(store.favorite("bob").unwrap());
        store.set_blocked("bob", false).unwrap();
        assert_eq!(store.favorite_peers().unwrap(), ["bob", "carol"]);
        store
            .apply_peer_change(
                "bob",
                &PeerChange::Favorite(false),
                Timestamp::new(2_000, 0),
                &[],
            )
            .unwrap();
        assert_eq!(store.favorite_peers().unwrap(), ["carol"]);
        let other = Store::open(&temp_db_path(), &Identity::generate()).unwrap();
        assert!(other.favorite_peers().unwrap().is_empty());
        assert!(Store::open(&path, &Identity::generate()).is_err());
    }

    #[test]
    fn notes_survive_reopen_and_clear_without_replaying_old_text() {
        let path = temp_db_path();
        let owner = Identity::generate();
        let store = Store::open(&path, &owner).unwrap();
        assert_eq!(store.contact_note("bob").unwrap(), "");
        store
            .apply_peer_change(
                "bob",
                &PeerChange::Note("Case\nLine".into()),
                10u64.into(),
                &[1; 32],
            )
            .unwrap();
        drop(store);
        let mut store = Store::open(&path, &owner).unwrap();
        assert_eq!(store.contact_note("bob").unwrap(), "Case\nLine");
        let invalid = PeerChange::Note("x".repeat(crate::chat::MAX_NOTE_CHARS + 1));
        let write = prepare_put(&[7; 32], vec![1]).unwrap();
        assert!(matches!(
            store.stage_peer_change("bob", &invalid, 11, b"sealed", &[write], &[1; 32]),
            Err(Error::InvalidSetting)
        ));
        assert!(store.due_outbound(0).unwrap().is_empty());
        assert!(store.sealed_device().unwrap().is_none());
        assert_eq!(store.contact_note("bob").unwrap(), "Case\nLine");
        store
            .apply_peer_change(
                "bob",
                &PeerChange::Note(String::new()),
                12u64.into(),
                &[1; 32],
            )
            .unwrap();
        assert!(!store
            .receive_peer_change("bob", &PeerChange::Note("old".into()), 10, 0, &[2; 32])
            .unwrap());
        assert_eq!(store.contact_note("bob").unwrap(), "");
        let other = Store::open(&temp_db_path(), &Identity::generate()).unwrap();
        assert_eq!(other.contact_note("bob").unwrap(), "");
        assert!(Store::open(&path, &Identity::generate()).is_err());
    }

    #[test]
    fn an_own_change_applies_and_stages_in_one_commit() {
        let alice = Identity::generate();
        let mut store = Store::open(&temp_db_path(), &alice).unwrap();
        let at = store.tick(1_000).unwrap();
        store
            .apply_peer_change("bob", &PeerChange::Muted(true), at, &[])
            .unwrap();
        assert!(store.muted("bob").unwrap());

        let write = prepare_put(&[7; 32], vec![1]).unwrap();
        let hlc = store.tick(1_001).unwrap().into();
        let change = PeerChange::Blocked(true);
        store
            .stage_peer_change(
                "bob",
                &change,
                hlc,
                b"sealed",
                std::slice::from_ref(&write),
                &[],
            )
            .unwrap();
        assert!(store.blocked("bob").unwrap());
        assert_eq!(store.sealed_device().unwrap(), Some(b"sealed".to_vec()));
        assert_eq!(store.due_outbound(0).unwrap().len(), 1);
    }

    #[test]
    fn block_and_mute_are_independent_and_survive_reopen() {
        let path = temp_db_path();
        let alice = Identity::generate();
        let store = Store::open(&path, &alice).unwrap();
        assert!(!store.blocked("bob").unwrap());
        store.set_blocked("bob", true).unwrap();
        store.set_muted("carol", true).unwrap();
        store.set_blocked("carol", true).unwrap();
        store.set_blocked("carol", false).unwrap();
        drop(store);
        let store = Store::open(&path, &alice).unwrap();
        assert!(store.blocked("bob").unwrap());
        assert!(!store.muted("bob").unwrap());
        assert!(store.muted("carol").unwrap());
        assert!(!store.blocked("carol").unwrap());
        assert_eq!(store.blocked_peers().unwrap(), ["bob"]);
    }

    // Bob's verified profile listing one device per generation in `generations`.
    fn bob(identity: &Identity, generations: &[u64]) -> VerifiedProfile {
        bob_at(identity, generations, 1)
    }

    fn bob_at(identity: &Identity, generations: &[u64], version: u64) -> VerifiedProfile {
        let mut devices: Vec<_> = generations
            .iter()
            .map(|generation| {
                let key = Identity::generate().public_key();
                let device = Device::new(identity, key, *generation).unwrap();
                (key_hash(&key), device.certificate().clone())
            })
            .collect();
        devices.sort_by_key(|(mailbox, _)| *mailbox);
        let profile = Profile {
            version,
            name: "Bob".into(),
            devices: devices.into_iter().map(|(_, signed)| signed).collect(),
            ..Default::default()
        };
        dyapp_profile::verify(&profile.sign(identity)).unwrap()
    }

    #[test]
    fn a_local_name_replaces_the_public_name_and_survives_reopen() {
        let path = temp_db_path();
        let alice = Identity::generate();
        let store = Store::open(&path, &alice).unwrap();
        let bob_id = Identity::generate();
        let mut bob = bob(&bob_id, &[1]);
        let id = bob.peer_id.clone();
        assert_eq!(
            store.contact_name(&bob).unwrap(),
            ContactName::Public {
                name: "Bob".into(),
                verified: false
            }
        );
        store.set_blocked(&id, true).unwrap();
        store.set_local_name(&id, "Боб с работы").unwrap();
        assert!(matches!(
            store.set_local_name(&id, "B\u{202E}ob"),
            Err(Error::InvalidName)
        ));
        assert!(matches!(
            store.set_local_name(&id, &"x".repeat(65)),
            Err(Error::InvalidName)
        ));
        drop(store);
        let store = Store::open(&path, &alice).unwrap();
        assert!(store.blocked(&id).unwrap());
        assert_eq!(
            store.contact_name(&bob).unwrap(),
            ContactName::Local("Боб с работы".into())
        );
        store.set_local_name(&id, "").unwrap();
        assert_eq!(store.local_name(&id).unwrap(), "");
        bob.profile.name.clear();
        assert_eq!(store.contact_name(&bob).unwrap(), ContactName::Unnamed);
    }

    #[test]
    fn a_device_added_to_a_verified_contact_clears_the_mark() {
        let path = temp_db_path();
        let store = Store::open(&path, &Identity::generate()).unwrap();
        let bob_id = Identity::generate();
        let two = bob(&bob_id, &[1, 2]);
        assert!(!store.verified(&two).unwrap());
        store.set_verified(&two).unwrap();
        assert!(store.verified(&two).unwrap());
        assert_eq!(
            store.contact_name(&two).unwrap(),
            ContactName::Public {
                name: "Bob".into(),
                verified: true
            }
        );

        store.clear_verified(&two.peer_id).unwrap();
        assert!(!store.verified(&two).unwrap());
        store.set_verified(&two).unwrap();

        // Dropping a device keeps the mark; a new one clears it for good.
        let mut one = two.clone();
        one.profile.devices.truncate(1);
        assert!(store.verified(&one).unwrap());
        let three = bob(&bob_id, &[3]);
        assert!(!store.verified(&three).unwrap());
        assert!(!store.verified(&two).unwrap());
    }

    #[test]
    fn only_a_current_profile_with_an_unseen_device_raises_a_durable_notice() {
        let path = temp_db_path();
        let alice = Identity::generate();
        let store = Store::open(&path, &alice).unwrap();
        let bob_id = Identity::generate();
        let verified = bob_at(&bob_id, &[1], 5);
        let peer = verified.peer_id.clone();

        // An older profile proves no change.
        store.set_verified(&verified).unwrap();
        assert!(store.verified(&bob_at(&bob_id, &[2], 4)).unwrap());
        assert!(!store.new_device(&peer).unwrap());

        // The same version with another device is a signed fork, not stale data.
        let added = bob_at(&bob_id, &[2], 5);
        assert!(!store.verified(&added).unwrap());
        assert!(store.new_device(&peer).unwrap());
        drop(store);
        let store = Store::open(&path, &alice).unwrap();
        assert!(store.new_device(&peer).unwrap());
        assert!(!store.verified(&verified).unwrap());
        store.acknowledge_new_device(&peer).unwrap();
        assert!(!store.new_device(&peer).unwrap());
        assert!(!store.verified(&verified).unwrap());

        // Checking again ends a raised notice too.
        store.set_verified(&verified).unwrap();
        assert!(!store.verified(&added).unwrap());
        store.set_verified(&added).unwrap();
        assert!(store.verified(&added).unwrap());
        assert!(!store.new_device(&peer).unwrap());
    }

    #[test]
    fn a_contact_never_verified_gets_the_notice_once_per_change() {
        let path = temp_db_path();
        let alice = Identity::generate();
        let store = Store::open(&path, &alice).unwrap();
        let bob_id = Identity::generate();
        let first = bob_at(&bob_id, &[1], 5);
        let peer = first.peer_id.clone();

        // A first contact and an older profile raise nothing, across a restart too.
        assert!(!store.verified(&first).unwrap());
        assert!(!store.new_device(&peer).unwrap());
        drop(store);
        let store = Store::open(&path, &alice).unwrap();
        assert!(!store.verified(&bob_at(&bob_id, &[2], 4)).unwrap());
        assert!(!store.new_device(&peer).unwrap());

        // A newer profile raises a notice that survives a restart and marks nothing verified.
        let added = bob_at(&bob_id, &[2], 6);
        assert!(!store.verified(&added).unwrap());
        assert!(store.new_device(&peer).unwrap());
        drop(store);
        let store = Store::open(&path, &alice).unwrap();
        assert!(store.new_device(&peer).unwrap());
        assert!(!store.verified(&added).unwrap());

        // The acknowledged change stays quiet, however often the profile is read.
        store.acknowledge_new_device(&peer).unwrap();
        assert!(!store.verified(&added).unwrap());
        assert!(!store.new_device(&peer).unwrap());

        // The next link is a new change, even with the mark taken back by hand.
        let mut linked = added.clone();
        linked.profile.version = 7;
        let more = bob_at(&bob_id, &[3], 7).profile.devices;
        linked.profile.devices.extend(more);
        store.set_verified(&added).unwrap();
        store.clear_verified(&peer).unwrap();
        assert!(!store.verified(&linked).unwrap());
        assert!(store.new_device(&peer).unwrap());
        store.acknowledge_new_device(&peer).unwrap();

        // A device removed by a newer profile is new again when it returns.
        let mut removed = added.clone();
        removed.profile.version = 8;
        assert!(!store.verified(&removed).unwrap());
        assert!(!store.new_device(&peer).unwrap());
        linked.profile.version = 9;
        assert!(!store.verified(&linked).unwrap());
        assert!(store.new_device(&peer).unwrap());
    }
}
