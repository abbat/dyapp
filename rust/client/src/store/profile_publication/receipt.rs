//! A confirmed ordinary readback is replayable only while its exact search projection is current.
use super::{Error, Store};
use dyapp_identity::{sha256, Identity, SignedRecord};
use dyapp_profile::Profile;
use prost::Message as _;
use rusqlite::{params, OptionalExtension};

const KEY: &str = "profile_completion_receipt";

impl Store {
    // Called only under the completion writer; an offset alone never proves legacy readback.
    pub(super) fn migrate_legacy_profile_completion(
        &self,
        identity: &Identity,
        published: &Profile,
        current: &Profile,
    ) -> Result<bool, Error> {
        let exists: bool = self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM `settings` WHERE `key` = ?1)",
            [KEY],
            |row| row.get(0),
        )?;
        if exists
            || !self.profile_hidden()?
            || published.deleted
            || published.version.checked_add(1) != Some(current.version)
            || self.device_listed()? != published.version
            || current.pow_bits < published.pow_bits
        {
            return Ok(false);
        }
        let mut content = current.clone();
        content.version = published.version;
        content.pow_bits = published.pow_bits;
        content.pow_nonce = published.pow_nonce;
        if content != *published {
            return Ok(false);
        }
        for proof in [current, published] {
            let queued = Profile::tombstone(current.version, proof.pow_nonce, proof.pow_bits);
            if self.search_profile_known(&queued.sign(identity))? {
                self.remember_profile_completion(identity, published, &queued)?;
                return Ok(true);
            }
        }
        Ok(false)
    }

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
