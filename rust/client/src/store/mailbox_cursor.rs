//! Bounded mailbox fetch position for one holder, stored with the profile's other settings.
//! It is a hint for replay-safe catch-up, not an acknowledgement or MLS state.
use super::{Error, Store};
use libp2p::PeerId;
use rusqlite::{params, OptionalExtension};

const KEY: &str = "mailbox_fetch_cursor";
const MAX_CURSOR: usize = 64;
const MAX_PEER_ID: usize = 64;

impl Store {
    /// The last saved cursor for `node`. Switching holders clears the old position.
    ///
    /// # Errors
    ///
    /// Returns an error if reading the saved cursor fails, its encoding or peer ID
    /// is invalid, or clearing a different holder's cursor fails.
    pub fn mailbox_cursor_for(&self, node: PeerId) -> Result<Option<Vec<u8>>, Error> {
        let row: Option<(i64, Vec<u8>)> = self
            .db
            .query_row(
                "SELECT `format_version`, `value` FROM `settings` WHERE `key` = ?1",
                [KEY],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((format, value)) = row else {
            return Ok(None);
        };
        if format != 1 {
            return Err(Error::InvalidSetting);
        }
        if value.is_empty() {
            return Ok(None);
        }
        let peer_len = usize::from(value[0]);
        if peer_len == 0
            || peer_len > MAX_PEER_ID
            || value.len() <= 1 + peer_len
            || value.len() > 1 + peer_len + MAX_CURSOR
        {
            return Err(Error::InvalidSetting);
        }
        let saved =
            PeerId::from_bytes(&value[1..1 + peer_len]).map_err(|_| Error::InvalidSetting)?;
        if saved != node {
            self.clear_mailbox_cursor()?;
            return Ok(None);
        }
        Ok(Some(value[1 + peer_len..].to_vec()))
    }

    /// Save a cursor only after the page's durable processing and ACK attempt.
    ///
    /// # Errors
    ///
    /// Returns an error if the peer ID or cursor is empty or oversized, or writing
    /// the cursor setting fails.
    pub fn save_mailbox_cursor(&self, node: PeerId, cursor: &[u8]) -> Result<(), Error> {
        let peer = node.to_bytes();
        if peer.is_empty()
            || peer.len() > MAX_PEER_ID
            || cursor.is_empty()
            || cursor.len() > MAX_CURSOR
        {
            return Err(Error::InvalidSetting);
        }
        let mut value = Vec::with_capacity(1 + peer.len() + cursor.len());
        value.push(u8::try_from(peer.len()).map_err(|_| Error::InvalidSetting)?);
        value.extend_from_slice(&peer);
        value.extend_from_slice(cursor);
        self.write_mailbox_cursor(&value)
    }

    /// Start the next pass from the mailbox head. This keeps one bounded settings row.
    ///
    /// # Errors
    ///
    /// Returns an error if writing the empty cursor setting fails.
    pub fn clear_mailbox_cursor(&self) -> Result<(), Error> {
        self.write_mailbox_cursor(&[])
    }

    fn write_mailbox_cursor(&self, value: &[u8]) -> Result<(), Error> {
        self.db.execute(
            "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, 1, ?2)
             ON CONFLICT (`key`) DO UPDATE SET `format_version` = 1, `value` = excluded.`value`",
            params![KEY, value],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::tests::temp_db_path;
    use dyapp_identity::Identity;

    #[test]
    fn cursor_survives_restart_only_for_its_holder_and_clears_at_end() {
        let (path, owner) = (temp_db_path(), Identity::generate());
        let (first, second) = (PeerId::random(), PeerId::random());
        let store = Store::open(&path, &owner).unwrap();
        assert_eq!(store.mailbox_cursor_for(first).unwrap(), None);
        store.save_mailbox_cursor(first, &[7; 10]).unwrap();
        drop(store);

        let store = Store::open(&path, &owner).unwrap();
        assert_eq!(store.mailbox_cursor_for(first).unwrap(), Some(vec![7; 10]));
        assert_eq!(store.mailbox_cursor_for(second).unwrap(), None);
        assert_eq!(store.mailbox_cursor_for(first).unwrap(), None);
        store.save_mailbox_cursor(second, &[8; 10]).unwrap();
        store.clear_mailbox_cursor().unwrap();
        assert_eq!(store.mailbox_cursor_for(second).unwrap(), None);
    }

    #[test]
    fn corrupt_and_oversized_cursors_fail_safely() {
        let store = Store::open(&temp_db_path(), &Identity::generate()).unwrap();
        let node = PeerId::random();
        for value in [vec![0], vec![1, 0], vec![255, 1, 2], vec![1, 0, 7]] {
            store.write_mailbox_cursor(&value).unwrap();
            assert!(matches!(
                store.mailbox_cursor_for(node),
                Err(Error::InvalidSetting)
            ));
        }
        store
            .db
            .execute(
                "UPDATE `settings` SET `format_version` = ?1 WHERE `key` = ?2",
                params![2, KEY],
            )
            .unwrap();
        assert!(matches!(
            store.mailbox_cursor_for(node),
            Err(Error::InvalidSetting)
        ));
        store.clear_mailbox_cursor().unwrap();
        assert!(matches!(
            store.save_mailbox_cursor(node, &[]),
            Err(Error::InvalidSetting)
        ));
        assert!(matches!(
            store.save_mailbox_cursor(node, &[1; MAX_CURSOR + 1]),
            Err(Error::InvalidSetting)
        ));
    }
}
