//! Before the first listing request, bind its authorization to the verified ordinary source.
use super::{Error, ProfilePublication, Store};
use dyapp_identity::{sha256, Identity, SignedRecord};
use prost::Message as _;
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};

const KEY: &str = "link_enrollment";

#[cfg(test)]
mod tests;

impl Store {
    // A newly accepted/replaced pending link grants one enrollment. Reopening the same pending
    // bytes must never reset an attempted enrollment, including legacy rows without this marker.
    pub(in crate::store) fn reset_link_enrollment_in_tx(&self, state: &[u8]) -> Result<(), Error> {
        self.save_link_enrollment(&sha256(state))
    }

    // No public request may precede this commit. An unchanged source permits retry; membership
    // in a later readback proves acceptance separately. A changed source without this certificate
    // is ambiguous (possibly accepted then revoked), so only new owner authorization can proceed.
    pub(crate) fn admit_link_enrollment(
        &self,
        identity: &Identity,
        certificate: &SignedRecord,
        attempt: &ProfilePublication,
        state: &[u8],
    ) -> Result<(), Error> {
        let source = attempt.directory_source().ok_or(Error::InvalidSetting)?;
        if identity.peer_id() != self.owner_peer_id()
            || certificate.public_key != identity.public_key()
        {
            return Err(Error::WrongIdentity);
        }
        let transaction = Transaction::new_unchecked(&self.db, TransactionBehavior::Immediate)?;
        if self.link_pending()?.as_deref() != Some(state) {
            return Err(Error::LinkStateChanged);
        }
        // This entry point is only for an absent certificate. Previously observed membership
        // cannot become fresh enrollment, including a same-version contradictory directory.
        if self.device_listed()? != 0 {
            return Err(Error::LinkEnrollmentReviewRequired);
        }
        let row: Option<(i64, Vec<u8>)> = self
            .db
            .query_row(
                "SELECT `format_version`, `value` FROM `settings` WHERE `key` = ?1",
                [KEY],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((format, saved)) = row else {
            return Err(Error::LinkEnrollmentReviewRequired);
        };
        if format != 1 || !matches!(saved.len(), 32 | 96) || saved[..32] != sha256(state) {
            return Err(Error::InvalidSetting);
        }
        let mut intent = sha256(state).to_vec();
        intent.extend(sha256(&certificate.encode_to_vec()));
        // This is a semantic ordinary snapshot, not a wire-record digest or a server CAS token.
        intent.extend(sha256(&source.sign(identity).encode_to_vec()));
        if saved.len() == 96 && saved != intent {
            return Err(Error::LinkEnrollmentReviewRequired);
        }
        self.save_link_enrollment(&intent)?;
        transaction.commit()?;
        Ok(())
    }

    fn save_link_enrollment(&self, value: &[u8]) -> Result<(), Error> {
        self.db.execute(
            "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, 1, ?2)
             ON CONFLICT (`key`) DO UPDATE SET `format_version` = 1, `value` = excluded.`value`",
            params![KEY, value],
        )?;
        Ok(())
    }
}
