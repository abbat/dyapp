//! Local block and mute flags per peer identity; never published or signalled.

use super::{Error, Store};
use rusqlite::{params, OptionalExtension};

pub(super) const SCHEMA_V8: &str = r"
CREATE TABLE `peers` (
    `peer_id` TEXT PRIMARY KEY NOT NULL,
    `blocked` INTEGER NOT NULL DEFAULT 0 CHECK (`blocked` IN (0, 1)),
    `muted` INTEGER NOT NULL DEFAULT 0 CHECK (`muted` IN (0, 1))
) STRICT;
PRAGMA user_version = 8;
";

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
    use super::super::Store;
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
}
