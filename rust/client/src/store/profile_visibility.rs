//! Atomic hide/withdrawal projections, preserving current draft data across network requests.
use super::{Error, Store};
use dyapp_identity::Identity;
use dyapp_profile::Profile;
use rusqlite::{Transaction, TransactionBehavior};

impl Store {
    /// Prepare a withdrawal tombstone from one snapshot of all known version/proof floors.
    ///
    /// Network work follows after the read transaction ends.
    ///
    /// # Errors
    /// Returns owner mismatch, version exhaustion, invalid state or database errors.
    pub fn prepare_profile_withdrawal(
        &self,
        identity: &Identity,
        request: &Profile,
        remote_version: u64,
    ) -> Result<Profile, Error> {
        if identity.peer_id() != self.owner_peer_id {
            return Err(Error::WrongIdentity);
        }
        let snapshot = Transaction::new_unchecked(&self.db, TransactionBehavior::Deferred)?;
        let latest = self.profile_draft()?.unwrap_or_else(|| request.clone());
        let version = latest
            .version
            .max(request.version)
            .max(remote_version)
            .max(self.own_profile_seen()?)
            .max(self.known_search_version()?)
            .checked_add(1)
            .ok_or(Error::ProfileVersionExhausted)?;
        let proof = if latest.pow_bits >= request.pow_bits {
            &latest
        } else {
            request
        };
        let tombstone = Profile::tombstone(version, proof.pow_nonce, proof.pow_bits);
        snapshot.commit()?;
        Ok(tombstone)
    }

    /// Project a verified withdrawal readback without replacing current draft fields or devices.
    ///
    /// The caller must have checked the node's signed readback against this exact tombstone.
    /// Remote success can precede a refused/failed local commit; no local projection is partial.
    ///
    /// # Errors
    /// Returns owner mismatch, noncanonical tombstone, superseded completion or storage errors.
    pub fn commit_profile_withdrawal(
        &self,
        identity: &Identity,
        request: &Profile,
        tombstone: &Profile,
    ) -> Result<Profile, Error> {
        if identity.peer_id() != self.owner_peer_id {
            return Err(Error::WrongIdentity);
        }
        let canonical = Profile {
            previous: tombstone.previous.clone(),
            ..Profile::tombstone(tombstone.version, tombstone.pow_nonce, tombstone.pow_bits)
        };
        if tombstone.version == 0 || *tombstone != canonical {
            return Err(Error::InvalidSetting);
        }
        let transaction = Transaction::new_unchecked(&self.db, TransactionBehavior::Immediate)?;
        let mut latest = self.profile_draft()?.unwrap_or_else(|| request.clone());
        if self.withdrawal_superseded(latest.version, request.version, tombstone.version)? {
            return Err(Error::ProfileCompletionSuperseded);
        }
        latest.version = tombstone.version;
        latest.previous.clone_from(&tombstone.previous);
        if tombstone.pow_bits > latest.pow_bits {
            (latest.pow_nonce, latest.pow_bits) = (tombstone.pow_nonce, tombstone.pow_bits);
        }
        self.save_profile_draft(&latest)?;
        self.hold_search_profile_in_tx(&tombstone.sign(identity))?;
        self.set_profile_hidden(true)?;
        transaction.commit()?;
        Ok(latest)
    }

    // Check under writer ownership; an equal ordinary draft may be mid-publication before its
    // search/visibility writes, whereas an equal hidden tombstone is an idempotent completion.
    fn withdrawal_superseded(
        &self,
        local_version: u64,
        request_version: u64,
        remote_version: u64,
    ) -> Result<bool, Error> {
        let floor = local_version
            .max(request_version)
            .max(self.own_profile_seen()?)
            .max(self.known_search_version()?);
        Ok(
            floor > remote_version
                || (local_version == remote_version && !self.profile_hidden()?),
        )
    }

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

#[cfg(test)]
#[path = "profile_withdrawal_tests.rs"]
mod withdrawal_tests;
