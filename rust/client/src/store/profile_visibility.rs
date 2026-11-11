//! Atomic local search hiding, using current draft data instead of a stale request snapshot.
use super::{Error, Store};
use dyapp_identity::Identity;
use dyapp_profile::Profile;
use rusqlite::{Transaction, TransactionBehavior};

impl Store {
    /// Hide the latest draft and hold its signed search tombstone in one local transaction.
    ///
    /// The request is a fallback for an absent draft and a floor for version/proof progress;
    /// it never replaces newer local fields, photos or device data.
    ///
    /// # Errors
    /// Returns owner mismatch, version exhaustion, malformed state or database errors.
    /// All draft, search and visibility writes roll back together on failure.
    pub fn hide_profile_draft(
        &self,
        identity: &Identity,
        request: &Profile,
    ) -> Result<Profile, Error> {
        if identity.peer_id() != self.owner_peer_id {
            return Err(Error::WrongIdentity);
        }
        let transaction = Transaction::new_unchecked(&self.db, TransactionBehavior::Immediate)?;
        let mut latest = self.profile_draft()?.unwrap_or_else(|| request.clone());
        let version = latest
            .version
            .max(request.version)
            .max(self.own_profile_seen()?)
            .max(self.known_search_version()?)
            .checked_add(1)
            .ok_or(Error::ProfileVersionExhausted)?;
        if request.pow_bits > latest.pow_bits {
            (latest.pow_nonce, latest.pow_bits) = (request.pow_nonce, request.pow_bits);
        }
        latest.version = version;
        let tombstone = Profile::tombstone(version, latest.pow_nonce, latest.pow_bits);
        self.save_profile_draft(&latest)?;
        self.hold_search_profile_in_tx(&tombstone.sign(identity))?;
        self.set_profile_hidden(true)?;
        transaction.commit()?;
        Ok(latest)
    }
}

#[cfg(test)]
#[path = "profile_visibility_tests.rs"]
mod tests;
