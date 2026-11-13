//! A confirmed ordinary readback is replayable only while its exact search projection is current.
use super::{Error, Store};
use dyapp_identity::{sha256, Identity, SignedRecord};
use dyapp_profile::Profile;
use prost::Message as _;
use rusqlite::{params, OptionalExtension};

const KEY: &str = "profile_completion_receipt";

impl Store {
    pub(super) fn remember_profile_completion(
        &self,
        identity: &Identity,
        published: &Profile,
        queued: &Profile,
    ) -> Result<(), Error> {
        let mut receipt = vec![u8::from(self.profile_hidden()?)];
        receipt.extend(sha256(&published.sign(identity).encode_to_vec()));
        receipt.extend(queued.sign(identity).encode_to_vec());
        self.db.execute(
            "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, 1, ?2)
             ON CONFLICT (`key`) DO UPDATE SET `format_version` = 1, `value` = excluded.`value`",
            params![KEY, receipt],
        )?;
        Ok(())
    }

    pub(super) fn completed_profile_version(
        &self,
        identity: &Identity,
        published: &Profile,
    ) -> Result<Option<u64>, Error> {
        let row: Option<(i64, Vec<u8>)> = self
            .db
            .query_row(
                "SELECT `format_version`, `value` FROM `settings` WHERE `key` = ?1",
                [KEY],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((format, bytes)) = row else {
            return Ok(None);
        };
        if format != 1 || bytes.len() <= 33 || bytes[0] > 1 {
            return Err(Error::InvalidSetting);
        }
        if self.profile_hidden()? != (bytes[0] != 0) {
            return Ok(None);
        }
        let (digest, record) = bytes[1..].split_at(32);
        if digest != sha256(&published.sign(identity).encode_to_vec()) {
            return Ok(None);
        }
        let record = SignedRecord::decode(record).map_err(|_| Error::InvalidSetting)?;
        if !self.search_profile_known(&record)? {
            return Ok(None);
        }
        Ok(Some(
            dyapp_profile::stored_version(&record).map_err(|_| Error::InvalidSetting)?,
        ))
    }
}
