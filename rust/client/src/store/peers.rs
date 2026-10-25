//! Local block and mute flags, contact names and verified marks per peer identity; never
//! published or signalled.

use super::{Error, Store};
use dyapp_profile::{is_clean_text, DeviceCertificate, VerifiedProfile, MAX_FIELD_CHARS};
use prost::Message as _;
use rusqlite::{params, OptionalExtension};

pub(super) const SCHEMA_V8: &str = r"
CREATE TABLE `peers` (
    `peer_id` TEXT PRIMARY KEY NOT NULL,
    `blocked` INTEGER NOT NULL DEFAULT 0 CHECK (`blocked` IN (0, 1)),
    `muted` INTEGER NOT NULL DEFAULT 0 CHECK (`muted` IN (0, 1))
) STRICT;
PRAGMA user_version = 8;
";

pub(super) const SCHEMA_V9: &str = r"
ALTER TABLE `peers` ADD COLUMN `local_name` TEXT NOT NULL DEFAULT '';
PRAGMA user_version = 9;
";

// The device keys the contact listed when it was verified, 32 bytes each; NULL is unverified.
pub(super) const SCHEMA_V10: &str = r"
ALTER TABLE `peers` ADD COLUMN `verified_devices` BLOB;
PRAGMA user_version = 10;
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

    /// Mark the owner of `profile` verified after its safety number or QR code matched.
    pub fn set_verified(&self, profile: &VerifiedProfile) -> Result<(), Error> {
        self.db.execute(
            "INSERT INTO `peers` (`peer_id`, `verified_devices`) VALUES (?1, ?2)
             ON CONFLICT (`peer_id`) DO UPDATE SET `verified_devices` = excluded.`verified_devices`",
            params![profile.peer_id, device_keys(profile)],
        )?;
        Ok(())
    }

    /// Whether the owner of `profile` is verified; a device it added since clears the mark.
    pub fn verified(&self, profile: &VerifiedProfile) -> Result<bool, Error> {
        let known: Option<Vec<u8>> = self
            .db
            .query_row(
                "SELECT `verified_devices` FROM `peers` WHERE `peer_id` = ?1",
                [&profile.peer_id],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        let Some(known) = known else {
            return Ok(false);
        };
        let current = device_keys(profile);
        if current
            .chunks(32)
            .all(|key| known.chunks(32).any(|seen| seen == key))
        {
            return Ok(true);
        }
        self.db.execute(
            "UPDATE `peers` SET `verified_devices` = NULL WHERE `peer_id` = ?1",
            [&profile.peer_id],
        )?;
        Ok(false)
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

    // `column` is one of two literals above, never caller input.
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
            version: 1,
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

        // Dropping a device keeps the mark; a new one clears it for good.
        let mut one = two.clone();
        one.profile.devices.truncate(1);
        assert!(store.verified(&one).unwrap());
        let three = bob(&bob_id, &[3]);
        assert!(!store.verified(&three).unwrap());
        assert!(!store.verified(&two).unwrap());
    }
}
