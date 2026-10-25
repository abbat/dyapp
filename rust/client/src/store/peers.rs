//! Local block and mute flags and contact names per peer identity; never published or signalled.

use super::{Error, Store};
use dyapp_profile::{is_clean_text, MAX_FIELD_CHARS};
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

/// The name an app shows for a peer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContactName {
    /// The name this profile gave the peer; it replaces the public name everywhere.
    Local(String),
    /// The peer's own public name, shown with the verified or unverified mark.
    Public {
        /// The profile's `name` field.
        name: String,
        /// Whether the contact was verified in person; always false until verification exists.
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

    /// The name to show for `peer_id`: the local name, else `public_name` from its verified profile.
    pub fn contact_name(&self, peer_id: &str, public_name: &str) -> Result<ContactName, Error> {
        let local = self.local_name(peer_id)?;
        Ok(if !local.is_empty() {
            ContactName::Local(local)
        } else if !public_name.is_empty() {
            ContactName::Public {
                name: public_name.to_owned(),
                verified: false,
            }
        } else {
            ContactName::Unnamed
        })
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

#[cfg(test)]
mod tests {
    use super::super::tests::temp_db_path;
    use super::super::{Error, Store};
    use super::ContactName;
    use dyapp_identity::Identity;

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

    #[test]
    fn a_local_name_replaces_the_public_name_and_survives_reopen() {
        let path = temp_db_path();
        let alice = Identity::generate();
        let store = Store::open(&path, &alice).unwrap();
        assert_eq!(store.contact_name("bob", "").unwrap(), ContactName::Unnamed);
        assert_eq!(
            store.contact_name("bob", "Bob").unwrap(),
            ContactName::Public {
                name: "Bob".into(),
                verified: false
            }
        );
        store.set_blocked("bob", true).unwrap();
        store.set_local_name("bob", "Боб с работы").unwrap();
        assert!(matches!(
            store.set_local_name("bob", "B\u{202E}ob"),
            Err(Error::InvalidName)
        ));
        assert!(matches!(
            store.set_local_name("bob", &"x".repeat(65)),
            Err(Error::InvalidName)
        ));
        drop(store);
        let store = Store::open(&path, &alice).unwrap();
        assert!(store.blocked("bob").unwrap());
        assert_eq!(
            store.contact_name("bob", "Bob").unwrap(),
            ContactName::Local("Боб с работы".into())
        );
        store.set_local_name("bob", "").unwrap();
        assert_eq!(store.local_name("bob").unwrap(), "");
    }
}
