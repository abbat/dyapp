//! Bounded mailbox fetch position for one holder, stored with the profile's other settings.
//! It is a hint for replay-safe catch-up, not an acknowledgement or MLS state.
use super::{Error, Store};
use libp2p::PeerId;
use rusqlite::{params, OptionalExtension};

const KEY: &str = "mailbox_fetch_cursor";
const FORMAT_VERSION: i64 = 2;
const MAX_CURSOR: usize = 64;
const MAX_PEER_ID: usize = 64;
const MAX_UNKNOWN_OWNERS: usize = 20;
const OWNER_CHARS: usize = 64;
type CursorState = (Vec<u8>, Vec<String>);

impl Store {
    /// The last saved cursor for `node`. Switching holders clears the old position.
    ///
    /// # Errors
    ///
    /// Returns an error if reading the saved cursor fails, its encoding or peer ID
    /// is invalid, or clearing a different holder's cursor fails.
    pub fn mailbox_cursor_for(&self, node: PeerId) -> Result<Option<Vec<u8>>, Error> {
        Ok(self.mailbox_cursor_state(node)?.map(|(cursor, _)| cursor))
    }

    /// Read the cursor and owners whose profiles blocked earlier pages.
    ///
    /// # Errors
    /// Returns an error if the stored cursor is malformed or a holder switch cannot be saved.
    pub fn mailbox_cursor_state(&self, node: PeerId) -> Result<Option<CursorState>, Error> {
        let row: Option<(i64, Vec<u8>)> = self
            .db
            .query_row(
                "SELECT `format_version`, `value` FROM `settings` WHERE `key` = ?1",
                [KEY],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((format, rest)) = row else {
            return Ok(None);
        };
        let mut rest = rest.as_slice();
        if rest.is_empty() {
            return Ok(None);
        }
        if format != 1 && format != FORMAT_VERSION {
            return Err(Error::InvalidSetting);
        }
        let peer_len = take_byte(&mut rest)?;
        let saved = take(&mut rest, peer_len)?;
        if peer_len == 0 || peer_len > MAX_PEER_ID {
            return Err(Error::InvalidSetting);
        }
        let saved = PeerId::from_bytes(saved).map_err(|_| Error::InvalidSetting)?;
        if saved != node {
            self.clear_mailbox_cursor()?;
            return Ok(None);
        }
        let (cursor, owners) = decode_cursor(format, rest)?;
        Ok(Some((cursor, owners)))
    }

    /// Save a cursor only after the page's durable processing and ACK attempt.
    ///
    /// # Errors
    ///
    /// Returns an error if the peer ID or cursor is empty or oversized, or writing
    /// the cursor setting fails.
    pub fn save_mailbox_cursor(&self, node: PeerId, cursor: &[u8]) -> Result<(), Error> {
        self.save_mailbox_cursor_with_unknown(node, cursor, &[])
    }

    /// Save a cursor together with owners that must trigger a head pass once resolved.
    ///
    /// # Errors
    /// Returns an error if a peer, cursor or owner is malformed, or the setting cannot be written.
    pub fn save_mailbox_cursor_with_unknown(
        &self,
        node: PeerId,
        cursor: &[u8],
        owners: &[String],
    ) -> Result<(), Error> {
        let peer = node.to_bytes();
        if peer.is_empty()
            || peer.len() > MAX_PEER_ID
            || cursor.is_empty()
            || cursor.len() > MAX_CURSOR
            || owners.len() > MAX_UNKNOWN_OWNERS
            || owners.iter().any(|owner| {
                owner.len() != OWNER_CHARS || !owner.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
        {
            return Err(Error::InvalidSetting);
        }
        let mut value = Vec::new();
        value.push(u8::try_from(peer.len()).map_err(|_| Error::InvalidSetting)?);
        value.extend_from_slice(&peer);
        value.push(u8::try_from(cursor.len()).map_err(|_| Error::InvalidSetting)?);
        value.extend_from_slice(cursor);
        value.push(u8::try_from(owners.len()).map_err(|_| Error::InvalidSetting)?);
        for owner in owners {
            value.push(u8::try_from(OWNER_CHARS).map_err(|_| Error::InvalidSetting)?);
            value.extend_from_slice(owner.as_bytes());
        }
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
            "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, ?2, ?3)
             ON CONFLICT (`key`) DO UPDATE SET `format_version` = excluded.`format_version`, `value` = excluded.`value`",
            params![KEY, FORMAT_VERSION, value],
        )?;
        Ok(())
    }
}

fn take<'a>(bytes: &mut &'a [u8], length: usize) -> Result<&'a [u8], Error> {
    let (value, rest) = bytes
        .split_at_checked(length)
        .ok_or(Error::InvalidSetting)?;
    *bytes = rest;
    Ok(value)
}

fn take_byte(bytes: &mut &[u8]) -> Result<usize, Error> {
    Ok(usize::from(
        *take(bytes, 1)?.first().ok_or(Error::InvalidSetting)?,
    ))
}

fn decode_cursor(format: i64, mut bytes: &[u8]) -> Result<(Vec<u8>, Vec<String>), Error> {
    let cursor = if format == 1 {
        bytes.to_vec()
    } else {
        let length = take_byte(&mut bytes)?;
        take(&mut bytes, length)?.to_vec()
    };
    if cursor.is_empty() || cursor.len() > MAX_CURSOR {
        return Err(Error::InvalidSetting);
    }
    let owners = if format == FORMAT_VERSION {
        decode_owners(&mut bytes)?
    } else {
        Vec::new()
    };
    if !bytes.is_empty() {
        return Err(Error::InvalidSetting);
    }
    Ok((cursor, owners))
}

fn decode_owners(bytes: &mut &[u8]) -> Result<Vec<String>, Error> {
    let count = take_byte(bytes)?;
    if count > MAX_UNKNOWN_OWNERS {
        return Err(Error::InvalidSetting);
    }
    let mut owners = Vec::with_capacity(count);
    for _ in 0..count {
        let length = take_byte(bytes)?;
        let owner =
            String::from_utf8(take(bytes, length)?.to_vec()).map_err(|_| Error::InvalidSetting)?;
        if owner.len() != OWNER_CHARS || !owner.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(Error::InvalidSetting);
        }
        owners.push(owner);
    }
    Ok(owners)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dyapp_identity::Identity;

    #[test]
    fn cursor_survives_restart_only_for_its_holder_and_clears_at_end() {
        let test_dir = dyapp_test_support::test_dir().unwrap();
        let (path, owner) = (
            test_dir.path().join("client-1.db"),
            Identity::try_generate().unwrap(),
        );
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
    fn cursor_retains_unknown_owners_until_the_head_pass() {
        let test_dir = dyapp_test_support::test_dir().unwrap();
        let owner = Identity::try_generate().unwrap();
        let store = Store::open(&test_dir.path().join("client-1.db"), &owner).unwrap();
        let node = PeerId::random();
        let unknown = owner.peer_id();
        store
            .save_mailbox_cursor_with_unknown(node, &[7; 10], std::slice::from_ref(&unknown))
            .unwrap();
        assert_eq!(
            store.mailbox_cursor_state(node).unwrap(),
            Some((vec![7; 10], vec![unknown]))
        );
        store.clear_mailbox_cursor().unwrap();
        assert_eq!(store.mailbox_cursor_state(node).unwrap(), None);
    }

    #[test]
    fn corrupt_and_oversized_cursors_fail_safely() {
        let test_dir = dyapp_test_support::test_dir().unwrap();
        let store = Store::open(
            &test_dir.path().join("client-1.db"),
            &Identity::try_generate().unwrap(),
        )
        .unwrap();
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
                params![99, KEY],
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
