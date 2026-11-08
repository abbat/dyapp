//! This device's one last-resort `KeyPackage` publication. The exact signed record stays here so a
//! lost reply or a restart repeats that record instead of minting another revision.

use super::{Error, Store};
use dyapp_identity::SignedRecord;
use prost::Message as _;
use rusqlite::{params, OptionalExtension};

const KEY: &str = "last_resort_key_package";
const HEADER_BYTES: usize = 4 + 8 + 1;

/// The newest last-resort publication of this device.
#[derive(Clone, Debug, PartialEq)]
pub struct LastResort {
    /// Grows with every new package; the node keeps the highest.
    pub revision: u32,
    /// Unix second to replace it from, before its 30-day expiry.
    pub rotate_at: u64,
    /// A node accepted `record`; until then every attempt repeats it.
    pub published: bool,
    /// The signed `KeyPackage` record exactly as sent.
    pub record: SignedRecord,
}

impl Store {
    /// The saved publication; none before this device's first one.
    ///
    /// # Errors
    ///
    /// Returns an error if reading the saved publication fails, its format or
    /// header length is invalid, or its signed record cannot be decoded.
    pub fn last_resort(&self) -> Result<Option<LastResort>, Error> {
        let row: Option<(u32, Vec<u8>)> = self
            .db
            .query_row(
                "SELECT `format_version`, `value` FROM `settings` WHERE `key` = ?1",
                [KEY],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let value = match row {
            None => return Ok(None),
            Some((1, value)) if value.len() >= HEADER_BYTES => value,
            Some(_) => return Err(Error::InvalidSetting),
        };
        let (header, record) = value.split_at(HEADER_BYTES);
        let (revision, rest) = header.split_at(4);
        let (rotate_at, published) = rest.split_at(8);
        Ok(Some(LastResort {
            revision: u32::from_be_bytes(revision.try_into().unwrap_or_default()),
            rotate_at: u64::from_be_bytes(rotate_at.try_into().unwrap_or_default()),
            published: published == [1],
            record: SignedRecord::decode(record).map_err(|_| Error::InvalidSetting)?,
        }))
    }

    /// Replace the saved publication. Seal the private MLS state first.
    ///
    /// # Errors
    ///
    /// Returns an error if writing the saved publication to the database fails.
    pub fn save_last_resort(&self, state: &LastResort) -> Result<(), Error> {
        let mut value = Vec::with_capacity(HEADER_BYTES + state.record.encoded_len());
        value.extend_from_slice(&state.revision.to_be_bytes());
        value.extend_from_slice(&state.rotate_at.to_be_bytes());
        value.push(u8::from(state.published));
        value.extend_from_slice(&state.record.encode_to_vec());
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
    use dyapp_identity::{Domain, Identity};

    #[test]
    fn a_publication_survives_a_restart_byte_for_byte() {
        let identity = Identity::generate();
        let path =
            std::env::temp_dir().join(format!("dyapp-last-resort-{}.db", identity.peer_id()));
        let store = Store::open(&path, &identity).unwrap();
        assert_eq!(store.last_resort().unwrap(), None);
        let state = LastResort {
            revision: 7,
            rotate_at: 1_000,
            published: false,
            record: identity.sign(Domain::KeyPackage, b"record".to_vec()),
        };
        store.save_last_resort(&state).unwrap();
        drop(store);
        let store = Store::open(&path, &identity).unwrap();
        assert_eq!(store.last_resort().unwrap(), Some(state.clone()));
        let published = LastResort {
            published: true,
            ..state
        };
        store.save_last_resort(&published).unwrap();
        assert_eq!(store.last_resort().unwrap(), Some(published));
    }
}
