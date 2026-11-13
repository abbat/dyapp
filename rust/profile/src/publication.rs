//! Signed per-replica fences survive re-signing and cannot be detached as legacy authorization.
use crate::{verify, Domain, Error, Profile, ProfileCondition, SignedRecord, VerifiedProfile};
use dyapp_identity::sha256;
use prost::Message as _;

// Authenticate bounded raw bytes before protobuf allocation, including for legacy records.
pub(crate) fn verify_signature(record: &SignedRecord) -> Result<(String, Domain), Error> {
    Ok(record
        .verify(Domain::Profile)
        .map(|peer| (peer, Domain::Profile))
        .or_else(|_| {
            record
                .verify(Domain::ProfileConditional)
                .map(|peer| (peer, Domain::ProfileConditional))
        })?)
}

impl ProfileCondition {
    /// Capture the exact verified ordinary record, or confirmed absence.
    ///
    /// # Errors
    /// Returns the profile verification error for an invalid source.
    pub fn observed(record: Option<&SignedRecord>) -> Result<Self, Error> {
        record.map_or_else(
            || Ok(Self::default()),
            |record| {
                Ok(Self {
                    version: verify(record)?.profile.version,
                    record_hash: sha256(&record.encode_to_vec()).to_vec(),
                })
            },
        )
    }
}

impl Profile {
    pub(crate) fn signature_domain(&self) -> Domain {
        if self.search_only {
            Domain::SearchWithdrawal
        } else if self.previous.is_some() {
            Domain::ProfileConditional
        } else {
            Domain::Profile
        }
    }

    pub(crate) fn validate_condition(&self) -> Result<(), Error> {
        if self.search_only && (!self.deleted || self.previous.is_some()) {
            return Err(Error::Invalid("invalid search withdrawal purpose"));
        }
        let Some(previous) = &self.previous else {
            return Ok(());
        };
        let hash_len = if previous.version == 0 { 0 } else { 32 };
        if self.version <= previous.version || previous.record_hash.len() != hash_len {
            return Err(Error::Invalid("invalid profile publication condition"));
        }
        Ok(())
    }
}

impl VerifiedProfile {
    /// Refuse replacing unreviewed newer or forked state at this replica.
    /// Missing and older replicas may seed the signed target without discarding a newer record.
    /// The caller must hold its read/compare/write lock through persistence.
    ///
    /// # Errors
    /// Returns verification errors for invalid current state, or `Error::Stale` for a conflict.
    pub fn check_condition(&self, current: Option<&SignedRecord>) -> Result<(), Error> {
        let (Some(previous), Some(current)) = (&self.profile.previous, current) else {
            return Ok(());
        };
        let version = crate::verify_historical(current)?.profile.version;
        if version > previous.version
            || (version == previous.version
                && sha256(&current.encode_to_vec()).as_slice() != previous.record_hash)
        {
            return Err(Error::Stale);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
