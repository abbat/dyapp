//! Local block and mute flags, contact names and verified marks per peer identity; never
//! published or signalled.

use super::{Error, Store};
use dyapp_profile::{is_clean_text, DeviceCertificate, VerifiedProfile, MAX_FIELD_CHARS};
use prost::Message as _;
use rusqlite::{params, OptionalExtension};

// `seen_devices`: the device keys of the newest profile seen or verified, 32 bytes each; NULL
// before the first. `seen_version`: that profile's version. `verified`: the safety number or QR
// code was compared for those keys. `new_device`: a profile added a device, until acknowledged.
pub(super) const SCHEMA: &str = r"
CREATE TABLE `peers` (
    `peer_id` TEXT PRIMARY KEY NOT NULL,
    `blocked` INTEGER NOT NULL DEFAULT 0 CHECK (`blocked` IN (0, 1)),
    `muted` INTEGER NOT NULL DEFAULT 0 CHECK (`muted` IN (0, 1)),
    `local_name` TEXT NOT NULL DEFAULT '',
    `seen_devices` BLOB,
    `seen_version` INTEGER,
    `verified` INTEGER NOT NULL DEFAULT 0 CHECK (`verified` IN (0, 1)),
    `new_device` INTEGER NOT NULL DEFAULT 0 CHECK (`new_device` IN (0, 1))
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

impl Store {
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
    use dyapp_identity::{key_hash, Identity};
    use dyapp_messaging::mls::Device;
    use dyapp_profile::{Profile, VerifiedProfile};

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
